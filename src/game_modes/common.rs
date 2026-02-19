//! Common utilities shared across all game modes
//!
//! This module contains helper functions used by local, AI, and network game modes
//! to avoid code duplication and improve maintainability.

use std::time::{Duration, Instant};

/// Apply frame rate limiting to maintain consistent game speed.
///
/// This function should be called at the end of each game loop iteration.
/// It sleeps for the remaining time if the frame finished early, ensuring
/// a consistent frame rate across all game modes.
///
/// # Arguments
/// * `frame_start` - The `Instant` when the frame began (typically from `Instant::now()`)
/// * `frame_duration` - The target duration for each frame (e.g. `Duration::from_millis(1000 / 60)`)
pub fn limit_frame_rate(frame_start: Instant, frame_duration: Duration) {
    let elapsed = frame_start.elapsed();
    if elapsed < frame_duration {
        std::thread::sleep(frame_duration - elapsed);
    }
}
