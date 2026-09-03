use std::time::Duration;

#[inline]
fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be recorded as a monotonic counter value",
    label = "monotonic counters take an unsigned integer, a float, or a Duration",
    note = "tracing-opentelemetry casts a signed value to u64 for `monotonic_counter.`, so a \
            negative one wraps to an enormous positive count",
    note = "cast explicitly if you meant it, e.g. `value as u64` or `value as f64`"
)]
pub trait MonotonicCounterValue {
    type Out: tracing::Value;
    fn coerce(self) -> Self::Out;
}

impl MonotonicCounterValue for u8 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl MonotonicCounterValue for u16 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl MonotonicCounterValue for u32 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl MonotonicCounterValue for u64 {
    type Out = u64;
    fn coerce(self) -> u64 {
        self
    }
}

impl MonotonicCounterValue for usize {
    type Out = u64;
    fn coerce(self) -> u64 {
        self as u64
    }
}

impl MonotonicCounterValue for f32 {
    type Out = f64;
    fn coerce(self) -> f64 {
        f64::from(self)
    }
}

impl MonotonicCounterValue for f64 {
    type Out = f64;
    fn coerce(self) -> f64 {
        self
    }
}

impl MonotonicCounterValue for Duration {
    type Out = f64;
    fn coerce(self) -> f64 {
        millis(self)
    }
}
#[inline]
pub fn monotonic_counter_value<V: MonotonicCounterValue>(v: V) -> V::Out {
    v.coerce()
}
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be recorded as a counter value",
    label = "counters take a signed integer, a `u8`/`u16`/`u32`, or a float",
    note = "tracing-opentelemetry drops a `counter.` u64 above i64::MAX, so `u64` and `usize` \
            are not accepted",
    note = "a Duration is not a counter value: time an operation with `histogram!`, or report \
            a level with `gauge!`",
    note = "cast explicitly if you meant it, e.g. `value as i64` or `value as f64`"
)]
pub trait CounterValue {
    type Out: tracing::Value;
    fn coerce(self) -> Self::Out;
}

impl CounterValue for i8 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl CounterValue for i16 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl CounterValue for i32 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl CounterValue for i64 {
    type Out = i64;
    fn coerce(self) -> i64 {
        self
    }
}

impl CounterValue for isize {
    type Out = i64;
    fn coerce(self) -> i64 {
        self as i64
    }
}

impl CounterValue for u8 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl CounterValue for u16 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl CounterValue for u32 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl CounterValue for f32 {
    type Out = f64;
    fn coerce(self) -> f64 {
        f64::from(self)
    }
}

impl CounterValue for f64 {
    type Out = f64;
    fn coerce(self) -> f64 {
        self
    }
}
#[inline]
pub fn counter_value<V: CounterValue>(v: V) -> V::Out {
    v.coerce()
}
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be recorded as a histogram value",
    label = "histograms take an unsigned integer, a float, or a Duration",
    note = "tracing-opentelemetry has no i64 histogram: a signed value is silently recorded as \
            an attribute, not a metric",
    note = "cast explicitly if you meant it, e.g. `value as u64` or `value as f64`"
)]
pub trait HistogramValue {
    type Out: tracing::Value;
    fn coerce(self) -> Self::Out;
}

impl HistogramValue for u8 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl HistogramValue for u16 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl HistogramValue for u32 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl HistogramValue for u64 {
    type Out = u64;
    fn coerce(self) -> u64 {
        self
    }
}

impl HistogramValue for usize {
    type Out = u64;
    fn coerce(self) -> u64 {
        self as u64
    }
}

impl HistogramValue for f32 {
    type Out = f64;
    fn coerce(self) -> f64 {
        f64::from(self)
    }
}

impl HistogramValue for f64 {
    type Out = f64;
    fn coerce(self) -> f64 {
        self
    }
}

impl HistogramValue for Duration {
    type Out = f64;
    fn coerce(self) -> f64 {
        millis(self)
    }
}
#[inline]
pub fn histogram_value<V: HistogramValue>(v: V) -> V::Out {
    v.coerce()
}
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be recorded as a gauge value",
    label = "gauges take any integer, a float, or a Duration",
    note = "cast explicitly if you meant it, e.g. `value as i64` or `value as f64`"
)]
pub trait GaugeValue {
    type Out: tracing::Value;
    fn coerce(self) -> Self::Out;
}

impl GaugeValue for u8 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl GaugeValue for u16 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl GaugeValue for u32 {
    type Out = u64;
    fn coerce(self) -> u64 {
        u64::from(self)
    }
}

impl GaugeValue for u64 {
    type Out = u64;
    fn coerce(self) -> u64 {
        self
    }
}

impl GaugeValue for usize {
    type Out = u64;
    fn coerce(self) -> u64 {
        self as u64
    }
}

impl GaugeValue for i8 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl GaugeValue for i16 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl GaugeValue for i32 {
    type Out = i64;
    fn coerce(self) -> i64 {
        i64::from(self)
    }
}

impl GaugeValue for i64 {
    type Out = i64;
    fn coerce(self) -> i64 {
        self
    }
}

impl GaugeValue for isize {
    type Out = i64;
    fn coerce(self) -> i64 {
        self as i64
    }
}

impl GaugeValue for f32 {
    type Out = f64;
    fn coerce(self) -> f64 {
        f64::from(self)
    }
}

impl GaugeValue for f64 {
    type Out = f64;
    fn coerce(self) -> f64 {
        self
    }
}

impl GaugeValue for Duration {
    type Out = f64;
    fn coerce(self) -> f64 {
        millis(self)
    }
}
#[inline]
pub fn gauge_value<V: GaugeValue>(v: V) -> V::Out {
    v.coerce()
}
