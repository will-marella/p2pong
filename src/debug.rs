// Debug logging module for P2Pong
// Provides file-based logging that can be enabled via --debug flag

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

// Global flag to track whether debug logging is enabled
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

const LOG_FILE_PATH: &str = "/tmp/p2pong-debug.log";

/// Initialize debug logging to file
///
/// # Arguments
/// * `enabled` - Whether debug logging should be enabled (controlled by --debug flag)
///
/// # Behavior
/// - Stores enabled state globally for log() to check
/// - If enabled=false: Returns immediately, no file created
/// - If enabled=true: Creates/truncates log file and writes header
pub fn init(enabled: bool) -> io::Result<()> {
    // Store the enabled state globally
    DEBUG_ENABLED.store(enabled, Ordering::Relaxed);

    if !enabled {
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(LOG_FILE_PATH)?;

    writeln!(file, "=== P2Pong Debug Log ===")?;
    writeln!(file, "Session started: {:?}", SystemTime::now())?;
    writeln!(file, "To monitor: tail -f {}", LOG_FILE_PATH)?;
    writeln!(file, "========================================\n")?;

    Ok(())
}

/// Log a debug message to file
///
/// # Arguments
/// * `category` - Log category (e.g., "GAME_START", "NETWORK", "WEBRTC")
/// * `message` - Log message content
///
/// # Behavior
/// - If debug not enabled: Returns immediately (no-op)
/// - If enabled: Appends to log file with format: [timestamp] [CATEGORY] message
/// - Thread-safe through file system append operation
pub fn log(category: &str, message: &str) {
    // Early return if debug logging is not enabled
    if !DEBUG_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let timestamp = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE_PATH)
    {
        let _ = writeln!(file, "[{:013}] [{}] {}", timestamp, category, message);
    }
}
