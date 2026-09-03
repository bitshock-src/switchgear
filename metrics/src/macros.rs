#[doc(hidden)]
#[macro_export]
macro_rules! __metric {
    (@munch [$lvl:expr] [$field:expr] [$value:expr] [$($out:tt)*]) => {
        $crate::__private::tracing::event!(
            target: $crate::__private::TARGET,
            $lvl,
            { $($out)* { $field } = $value }
        )
    };

    (@munch [$lvl:expr] [$field:expr] [$value:expr] [$($out:tt)*]
        $key:expr => %$val:expr $(, $($rest:tt)*)?) => {
        $crate::__metric!(@munch [$lvl] [$field] [$value] [$($out)* { $key } = %$val,] $($($rest)*)?)
    };

    (@munch [$lvl:expr] [$field:expr] [$value:expr] [$($out:tt)*]
        $key:expr => ?$val:expr $(, $($rest:tt)*)?) => {
        $crate::__metric!(@munch [$lvl] [$field] [$value] [$($out)* { $key } = ?$val,] $($($rest)*)?)
    };

    (@munch [$lvl:expr] [$field:expr] [$value:expr] [$($out:tt)*]
        $key:expr => $val:expr $(, $($rest:tt)*)?) => {
        $crate::__metric!(@munch [$lvl] [$field] [$value] [$($out)* { $key } = $val,] $($($rest)*)?)
    };

    ($lvl:expr, $field:expr, $value:expr $(, $($labels:tt)*)?) => {
        $crate::__metric!(@munch [$lvl] [$field] [$value] [] $($($labels)*)?)
    };
}

/// Records a monotonically increasing count, as `monotonic_counter.<name>`.
///
/// ```
/// switchgear_metrics::monotonic_counter!("swgr_x_total", 1u64, "outcome" => "success");
/// ```
///
/// Takes an unsigned integer, `usize`, a float, or a `Duration` (recorded as
/// milliseconds). See [Value types](crate#value-types).
#[macro_export]
macro_rules! monotonic_counter {
    (level: $lvl:expr, $name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::__metric!(
            $lvl,
            ::core::concat!("monotonic_counter.", $name),
            $crate::value::monotonic_counter_value($value)
            $(, $($labels)*)?
        )
    };
    ($name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::monotonic_counter!(
            level: $crate::__private::tracing::Level::INFO,
            $name,
            $value
            $(, $($labels)*)?
        )
    };
}

/// Records a count that can go up or down, as `counter.<name>`.
///
/// ```
/// switchgear_metrics::counter!("swgr_x_inflight", -1i64, "outcome" => "success");
/// ```
///
/// Takes a signed integer, `isize`, a `u8`/`u16`/`u32`, or a float. See
/// [Value types](crate#value-types).
#[macro_export]
macro_rules! counter {
    (level: $lvl:expr, $name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::__metric!(
            $lvl,
            ::core::concat!("counter.", $name),
            $crate::value::counter_value($value)
            $(, $($labels)*)?
        )
    };
    ($name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::counter!(
            level: $crate::__private::tracing::Level::INFO,
            $name,
            $value
            $(, $($labels)*)?
        )
    };
}

/// Records an observation in a distribution, as `histogram.<name>`.
///
/// ```
/// use std::time::Instant;
/// let started = Instant::now();
/// switchgear_metrics::histogram!(
///     "swgr_ln_grpc_invoice_request_ms",
///     started.elapsed(),
///     "ln.backend" => "cln",
/// );
/// ```
///
/// Takes an unsigned integer, `usize`, a float, or a `Duration` (recorded as
/// milliseconds). See [Value types](crate#value-types).
#[macro_export]
macro_rules! histogram {
    (level: $lvl:expr, $name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::__metric!(
            $lvl,
            ::core::concat!("histogram.", $name),
            $crate::value::histogram_value($value)
            $(, $($labels)*)?
        )
    };
    ($name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::histogram!(
            level: $crate::__private::tracing::Level::INFO,
            $name,
            $value
            $(, $($labels)*)?
        )
    };
}

/// Records the current value of something that goes up and down, as
/// `gauge.<name>`.
///
/// ```
/// switchgear_metrics::gauge!("swgr_x_open", 3i32, "ln.backend" => "lnd");
/// ```
///
/// Takes any integer, a float, or a `Duration` (recorded as milliseconds).
/// See [Value types](crate#value-types).
#[macro_export]
macro_rules! gauge {
    (level: $lvl:expr, $name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::__metric!(
            $lvl,
            ::core::concat!("gauge.", $name),
            $crate::value::gauge_value($value)
            $(, $($labels)*)?
        )
    };
    ($name:literal, $value:expr $(, $($labels:tt)*)?) => {
        $crate::gauge!(
            level: $crate::__private::tracing::Level::INFO,
            $name,
            $value
            $(, $($labels)*)?
        )
    };
}
