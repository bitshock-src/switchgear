pub mod client;
pub mod context;
pub mod helpers;
pub mod step_functions;

#[macro_export]
macro_rules! bail_log {
    ($msg:literal $(,)?) => {
        anyhow::bail!(format!("{}:{}: {}", file!(), line!(), format_args!($msg)))
    };
    ($err:expr $(,)?) => {
        anyhow::bail!($err)
    };
    ($fmt:expr, $($arg:tt)*) => {
        anyhow::bail!(format!("{}:{}: {}", file!(), line!(), format!($fmt, $($arg)*)))
    };
}

#[macro_export]
macro_rules! anyhow_log {
    ($msg:literal $(,)?) => {
        anyhow::anyhow!(format!("{}:{}: {}", file!(), line!(), format_args!($msg)))
    };
    ($err:expr $(,)?) => {
        anyhow::anyhow!($err)
    };
    ($fmt:expr, $($arg:tt)*) => {
        anyhow::anyhow!(format!("{}:{}: {}", file!(), line!(), format!($fmt, $($arg)*)))
    };
}
