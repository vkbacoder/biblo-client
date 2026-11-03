//! Local observability and logging for the Rust core.
//! External error reporting is disabled.

#[derive(Clone, Copy, Debug)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}
/// No-op retained for compatibility with existing callers.
pub fn capture_error<E: std::error::Error + ?Sized>(_err: &E) {}

/// No-op retained for compatibility with existing callers.
pub fn set_user(_email: &str, _business_id: Option<&str>) {}

/// No-op retained for compatibility with existing callers.
pub fn clear_user() {}

/// Write a local log line to stderr.
pub fn log(_level: LogLevel, tag: &str, message: &str) {
    eprintln!("[{tag}] {message}");
}
/// Log an informational message locally.
/// Log a warning locally.
/// Log an error locally.
#[macro_export]
macro_rules! log_info {
    ($tag:expr, $($arg:tt)*) => {
        $crate::obs::log($crate::obs::LogLevel::Info, $tag, &format!($($arg)*))
    };
}
#[macro_export]
macro_rules! log_warn {
    ($tag:expr, $($arg:tt)*) => {
       $crate::obs::log($crate::obs::LogLevel::Warning, $tag, &format!($($arg)*))
    };
}
#[macro_export]
macro_rules! log_err {
    ($tag:expr, $($arg:tt)*) => {
        $crate::obs::log($crate::obs::LogLevel::Error, $tag, &format!($($arg)*))
    };
}

/// No-op retained for compatibility with existing callers.
pub fn capture_message(
    _level: LogLevel,
    _message: &str,
    _source: &str,
    _extras: &[(&str, String)],
) {
}
