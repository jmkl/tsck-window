#![allow(unused)]
use std::fmt::Display;

/// Log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
    Trace,
}

impl LogLevel {
    fn color_code(&self) -> &str {
        match self {
            LogLevel::Info => "\x1b[32m",  // Green
            LogLevel::Warn => "\x1b[33m",  // Yellow
            LogLevel::Error => "\x1b[31m", // Red
            LogLevel::Debug => "\x1b[36m", // Cyan
            LogLevel::Trace => "\x1b[35m", // Magenta
        }
    }

    fn label(&self) -> &str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
}

/// Internal logging function
pub fn log_internal(level: LogLevel, args: Vec<String>) {
    let timestamp = chrono::Local::now().format("%H:%M:%S");
    let reset = "\x1b[0m";
    let color = level.color_code();

    println!(
        "{}[{}] [{}]{} {}",
        color,
        timestamp,
        level.label(),
        reset,
        args.join(" ")
    );
}

/// Convert any type implementing Display to String
pub fn to_string<T: Display>(val: T) -> String {
    val.to_string()
}

/// Convert any type implementing Debug to String (for internal use)
pub fn to_debug_internal<T: std::fmt::Debug>(val: T) -> String {
    format!("{:?}", val)
}

// ============================================================================
// Helper macro to convert Debug types to Display-compatible strings
// ============================================================================

/// Convert a Debug type to a displayable string
/// Usage: log_info!("Config:", d!(config))
#[macro_export]
macro_rules! d {
    ($val:expr) => {
        format!("{:?}", $val)
    };
}

/// Pretty-print Debug types (multi-line for complex structures)
/// Usage: log_info!("Config:", dp!(config))
#[macro_export]
macro_rules! dp {
    ($val:expr) => {
        format!("{:#?}", $val)
    };
}

// ============================================================================
// Public API - Macro-based for flexibility
// ============================================================================

/// Log info message - accepts any number of Display types
#[macro_export]
macro_rules! log_info {
    ($($arg:expr),+ $(,)?) => {{
        let args = vec![$($crate::win::logger::to_string($arg)),+];
        $crate::win::logger::log_internal($crate::win::logger::LogLevel::Info, args);
    }};
}

/// Log warning message
#[macro_export]
macro_rules! log_warn {
    ($($arg:expr),+ $(,)?) => {{
        let args = vec![$($crate::win::logger::to_string($arg)),+];
        $crate::win::logger::log_internal($crate::win::logger::LogLevel::Warn, args);
    }};
}

/// Log error message
#[macro_export]
macro_rules! log_error {
    ($($arg:expr),+ $(,)?) => {{
        let args = vec![$($crate::win::logger::to_string($arg)),+];
        $crate::win::logger::log_internal($crate::win::logger::LogLevel::Error, args);
    }};
}

/// Log debug message
#[macro_export]
macro_rules! log_debug {
    ($($arg:expr),+ $(,)?) => {{
        let args = vec![$($crate::win::logger::to_string($arg)),+];
        $crate::win::logger::log_internal($crate::win::logger::LogLevel::Debug, args);
    }};
}

/// Log trace message
#[macro_export]
macro_rules! log_trace {
    ($($arg:expr),+ $(,)?) => {{
        let args = vec![$($crate::win::logger::to_string($arg)),+];
        $crate::win::logger::log_internal($crate::win::logger::LogLevel::Trace, args);
    }};
}

pub struct Logger;

impl Logger {
    /// Log with any types implementing Display
    pub fn info<T: Display>(args: &[T]) {
        let strings: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        log_internal(LogLevel::Info, strings);
    }

    pub fn warn<T: Display>(args: &[T]) {
        let strings: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        log_internal(LogLevel::Warn, strings);
    }

    pub fn error<T: Display>(args: &[T]) {
        let strings: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        log_internal(LogLevel::Error, strings);
    }

    pub fn debug<T: Display>(args: &[T]) {
        let strings: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        log_internal(LogLevel::Debug, strings);
    }

    pub fn trace<T: Display>(args: &[T]) {
        let strings: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        log_internal(LogLevel::Trace, strings);
    }
}
