// AI module for bot opponents

mod backboard_bot;
mod bot;
mod prediction;
mod predictive_bot;

pub use backboard_bot::BackboardBot;
pub use bot::Bot;
pub use predictive_bot::PredictiveBot;

/// Bot type selection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BotType {
    /// Easy bot - predictive with large errors, beatable by beginners
    Easy,
    /// Hard bot - predictive with moderate errors, requires skill
    Hard,
    /// Backboard - instant tracker for training mode
    Backboard,
}

impl BotType {
    /// Get display name for bot type
    pub fn display_name(&self) -> &str {
        match self {
            BotType::Easy => "Easy",
            BotType::Hard => "Hard",
            BotType::Backboard => "Backboard",
        }
    }

    /// Get description for bot type
    pub fn description(&self) -> &str {
        match self {
            BotType::Easy => "Beginner-friendly - makes frequent mistakes",
            BotType::Hard => "Competitive opponent - occasional errors",
            BotType::Backboard => "Training mode - perfect tracking",
        }
    }

    /// Get all available bot types
    pub fn all() -> Vec<BotType> {
        vec![BotType::Easy, BotType::Hard, BotType::Backboard]
    }
}

/// Create a bot instance from a bot type
pub fn create_bot(bot_type: BotType) -> Box<dyn Bot> {
    match bot_type {
        BotType::Easy => Box::new(PredictiveBot::easy()),
        BotType::Hard => Box::new(PredictiveBot::hard()),
        BotType::Backboard => Box::new(BackboardBot::new()),
    }
}
