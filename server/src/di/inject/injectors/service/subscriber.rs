use crate::config::{OtlpConfig, OtlpExportConfig, OtlpMetricsConfig, OtlpTracingConfig};
use crate::di::error::DiError;
use crate::di::inject::injectors::service::otlp;
use crate::level::Level;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use switchgear_error::{ErrorOrigin, ForeignContext};
use tracing::{Dispatch, Subscriber};
use tracing_ecs_formatter::EcsFormatter;
use tracing_opentelemetry::MetricsLayer;
use tracing_subscriber::filter::{Directive, LevelFilter, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer};

fn resolve_tracing(otlp: &OtlpConfig) -> Option<(&OtlpExportConfig, &OtlpTracingConfig)> {
    let tracing = otlp.tracing.as_ref()?;
    Some((tracing.export.as_ref().unwrap_or(&otlp.export), tracing))
}

fn resolve_metrics(otlp: &OtlpConfig) -> Option<(&OtlpExportConfig, &OtlpMetricsConfig)> {
    let metrics = otlp.metrics.as_ref()?;
    Some((metrics.export.as_ref().unwrap_or(&otlp.export), metrics))
}

pub fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

pub fn ecs_layer<S>(service_name: &'static str) -> impl Layer<S> + Send + Sync
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .event_format(EcsFormatter::new(service_name, env!("CARGO_PKG_VERSION")))
}

pub fn metrics_env_filter(level: LevelFilter) -> Result<EnvFilter, DiError> {
    let target = switchgear_metrics::metrics_target();
    let directive = format!("{target}={level}");
    let parsed: Directive = directive.parse().with_foreign_context(
        || format!("parsing metrics filter directive {directive}"),
        ErrorOrigin::Internal,
    )?;
    Ok(env_filter().add_directive(parsed))
}

pub struct ServiceTracing {
    dispatch: Dispatch,
    provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl ServiceTracing {
    pub fn build(service_name: &'static str, otlp: Option<&OtlpConfig>) -> Result<Self, DiError> {
        let metrics_off = std::env::var("OTEL_METRICS_EXPORTER").as_deref() == Ok("none");

        let tracing_signal = otlp.and_then(resolve_tracing);
        let metrics_signal = match otlp {
            Some(otlp) if !metrics_off => resolve_metrics(otlp),
            _ => None,
        };

        let shared = matches!(
            (tracing_signal, metrics_signal),
            (Some((t, _)), Some((m, _))) if std::ptr::eq(t, m)
        );

        let (provider, otel_layer, tracing_transport) = match tracing_signal {
            None => (None, None, None),
            Some((export, tracing)) => {
                let (channel, secrets) = otlp::transport(export)?;
                let provider = otlp::tracer_provider(
                    service_name,
                    export,
                    channel.clone(),
                    secrets.clone(),
                    &tracing.sampler,
                )?;
                let layer = tracing_opentelemetry::layer()
                    .with_tracer(provider.tracer(service_name))
                    .with_location(false)
                    .with_tracked_inactivity(false)
                    .with_threads(false);
                (Some(provider), Some(layer), Some((channel, secrets)))
            }
        };

        let (meter_provider, metrics_layer, metrics_level) = match metrics_signal {
            None => (None, None, LevelFilter::OFF),
            Some((export, metrics)) => {
                let (channel, secrets) = match (shared, &tracing_transport) {
                    (true, Some(transport)) => transport.clone(),
                    _ => otlp::transport(export)?,
                };
                let provider = otlp::meter_provider(
                    service_name,
                    export,
                    channel,
                    secrets,
                    &metrics.temporality,
                )?;
                let layer = MetricsLayer::new(provider.clone());
                let level = LevelFilter::from(metrics.level.unwrap_or(Level::Info));
                (Some(provider), Some(layer), level)
            }
        };

        let metrics_target = switchgear_metrics::metrics_target();

        let subscriber = tracing_subscriber::registry()
            .with(otel_layer.with_filter(filter_fn(|m| !m.is_event())))
            .with(metrics_layer.with_filter(filter_fn(move |m| m.target() == metrics_target)))
            .with(metrics_env_filter(metrics_level)?)
            .with(
                ecs_layer(service_name)
                    .with_filter(filter_fn(move |m| m.target() != metrics_target)),
            );

        Ok(Self {
            dispatch: Dispatch::new(subscriber),
            provider,
            meter_provider,
        })
    }

    pub fn dispatch(&self) -> &Dispatch {
        &self.dispatch
    }

    pub async fn shutdown(self) -> Result<(), DiError> {
        drop(self.dispatch);
        let (tracer, meter) = (self.provider, self.meter_provider);
        let (t, m) = tokio::join!(
            async {
                match tracer {
                    None => Ok(()),
                    Some(p) => tokio::task::spawn_blocking(move || p.shutdown())
                        .await
                        .foreign_context(
                            "joining otel tracer provider shutdown task",
                            ErrorOrigin::Internal,
                        )?
                        .foreign_context(
                            "shutting down otel tracer provider",
                            ErrorOrigin::Internal,
                        ),
                }
            },
            async {
                match meter {
                    None => Ok(()),
                    Some(p) => tokio::task::spawn_blocking(move || p.shutdown())
                        .await
                        .foreign_context(
                            "joining otel meter provider shutdown task",
                            ErrorOrigin::Internal,
                        )?
                        .foreign_context(
                            "shutting down otel meter provider",
                            ErrorOrigin::Internal,
                        ),
                }
            },
        );
        t.and(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, MutexGuard};
    use tracing::{Event, Level, Metadata};
    use tracing_subscriber::layer::Context;

    /// Every value `otlp.metrics.level` can hold.
    const ALL: [LevelFilter; 6] = [
        LevelFilter::OFF,
        LevelFilter::ERROR,
        LevelFilter::WARN,
        LevelFilter::INFO,
        LevelFilter::DEBUG,
        LevelFilter::TRACE,
    ];

    /// Records nothing. What matters is that its filter votes on every
    /// callsite, because a callsite's `Interest` is the sum of all of them.
    struct Silent;

    impl<S: Subscriber> Layer<S> for Silent {}

    /// Stands where the metrics layer goes, and keeps what reaches it.
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<Level>>>);

    impl Capture {
        fn levels(&self) -> Vec<Level> {
            lock(&self.0).clone()
        }
    }

    impl<S: Subscriber> Layer<S> for Capture {
        fn on_event(&self, event: &Event<'_>, _cx: Context<'_, S>) {
            lock(&self.0).push(*event.metadata().level());
        }
    }

    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Reproduces `tracing_opentelemetry::MetricsFilter`, the per-layer filter
    /// `MetricsLayer` carries internally.
    ///
    /// The real `MetricsLayer` cannot be used here. It owns an
    /// `SdkMeterProvider` whose `Drop` logs through `tracing`; when a stale
    /// dispatcher is reaped inside `rebuild_callsite_interest`, that log
    /// registers a callsite and re-enters `tracing-core`'s dispatcher lock,
    /// which the rebuild already holds. The deadlock is intermittent, because
    /// a callsite only registers once. `tests/features/otlp_metrics.rs` covers
    /// the real layer end to end instead.
    fn is_metrics_event(meta: &Metadata<'_>) -> bool {
        meta.is_event()
            && meta.fields().iter().any(|field| {
                let name = field.name();
                name.starts_with("monotonic_counter.")
                    || name.starts_with("counter.")
                    || name.starts_with("histogram.")
                    || name.starts_with("gauge.")
            })
    }

    /// Emits one `INFO` and one `DEBUG` metric under `filter`, in the layer
    /// stack `build` assembles, and reports which of them reached the metrics
    /// layer.
    ///
    /// The whole stack matters, not just the metrics layer: a callsite's
    /// `Interest` is the sum of every per-layer filter's answer, and three of
    /// these four exclude metric events.
    fn record(filter: EnvFilter) -> Vec<Level> {
        let metrics_target = switchgear_metrics::metrics_target();
        let capture = Capture::default();

        let subscriber = tracing_subscriber::registry()
            .with(Silent.with_filter(filter_fn(|m| !m.is_event())))
            .with(capture.clone().with_filter(filter_fn(move |m| {
                m.target() == metrics_target && is_metrics_event(m)
            })))
            .with(filter)
            .with(
                ecs_layer("swgr.test")
                    .with_filter(filter_fn(move |m| m.target() != metrics_target)),
            );

        tracing::subscriber::with_default(subscriber, || {
            switchgear_metrics::histogram!("swgr_gate_info_ms", 1u64);
            switchgear_metrics::histogram!(level: Level::DEBUG, "swgr_gate_debug_ms", 1u64);
        });

        capture.levels()
    }

    #[test]
    fn metrics_env_filter_builds_for_every_level_filter() {
        for level in ALL {
            metrics_env_filter(level).unwrap_or_else(|e| panic!("{level}: {e}"));
        }
    }

    #[test]
    fn the_directive_gates_the_metrics_target_at_the_configured_level() {
        for (level, expected) in [
            (LevelFilter::INFO, vec![Level::INFO]),
            (LevelFilter::DEBUG, vec![Level::INFO, Level::DEBUG]),
            (LevelFilter::OFF, vec![]),
        ] {
            let filter = metrics_env_filter(level).unwrap_or_else(|e| panic!("{level}: {e}"));
            assert_eq!(record(filter), expected, "level: {level}");
        }
    }

    #[test]
    fn the_directive_replaces_one_from_rust_log_for_the_same_target() {
        let target = switchgear_metrics::metrics_target();
        let rust_log = format!("info,{target}=trace");

        assert_eq!(
            record(EnvFilter::new(&rust_log)),
            vec![Level::INFO, Level::DEBUG],
            "RUST_LOG alone should have opened the metrics target"
        );

        let directive = format!("{target}=off");
        let filter = EnvFilter::new(&rust_log).add_directive(
            directive
                .parse()
                .unwrap_or_else(|e| panic!("parsing {directive}: {e}")),
        );

        assert!(record(filter).is_empty());
    }
}
