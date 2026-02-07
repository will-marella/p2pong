//! Common utilities shared across all game modes
//!
//! This module contains helper functions used by local, AI, and network game modes
//! to avoid code duplication and improve maintainability.

use std::time::Instant;

use crate::FRAME_DURATION;

/// Apply frame rate limiting to maintain consistent game speed.
///
/// This function should be called at the end of each game loop iteration.
/// It sleeps for the remaining time if the frame finished early, ensuring
/// a consistent frame rate across all game modes.
///
/// # Arguments
/// * `frame_start` - The `Instant` when the frame began (typically from `Instant::now()`)
///
/// # Example
/// ```rust,no_run
/// use std::time::Instant;
/// # use p2pong::game_modes::common::limit_frame_rate;
/// let frame_start = Instant::now();
/// // ... game loop logic ...
/// limit_frame_rate(frame_start);
/// ```
pub fn limit_frame_rate(frame_start: Instant) {
    let elapsed = frame_start.elapsed();
    if elapsed < FRAME_DURATION {
        std::thread::sleep(FRAME_DURATION - elapsed);
    }
}
