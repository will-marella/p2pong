use std::io;
use std::time::Instant;

use ratatui::Terminal;

use crate::ai;
use crate::config::Config;
use crate::debug;
use crate::game::{self, poll_input_player_left, GameState, InputAction};
use crate::ui;
use crate::FIXED_TIMESTEP;

use super::common::limit_frame_rate;

/// Run single-player game against AI
pub fn run_game_vs_ai<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
    bot_type: ai::BotType,
) -> Result<(), io::Error> {
    debug::log(
        "GAME_START",
        &format!("Single player vs AI mode: {:?}", bot_type),
    );

    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height, &config.physics);

    // Create bot instance using factory
    let mut bot = ai::create_bot(bot_type);

    loop {
        let now = Instant::now();

        // Handle player input (left paddle only)
        let actions = poll_input_player_left(config)?;

        for action in &actions {
            match action {
                InputAction::Quit => return Ok(()),
                InputAction::Rematch => {
                    if game_state.game_over {
                        game_state.reset_game();
                        bot.reset();
                    }
                }
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(
                        &mut game_state.left_paddle,
                        game_state.tap_distance,
                    );
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(
                        &mut game_state.left_paddle,
                        game_state.field_height,
                        game_state.tap_distance,
                    );
                }
                _ => {} // Ignore right paddle inputs
            }
        }

        // Bot input (right paddle)
        if let Some(bot_action) = bot.get_action(&game_state, FIXED_TIMESTEP) {
            match bot_action {
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(
                        &mut game_state.right_paddle,
                        game_state.tap_distance,
                    );
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(
                        &mut game_state.right_paddle,
                        game_state.field_height,
                        game_state.tap_distance,
                    );
                }
                _ => {} // Bot should only move right paddle
            }
        }

        // Update physics
        let events = game::update_with_events(&mut game_state, FIXED_TIMESTEP);

        // Reset bot state on new round (but keep rendering game over state)
        if events.goal_scored && !game_state.game_over {
            bot.reset();
        }

        // Create overlay message if game is over
        let overlay = if game_state.game_over {
            let winner_text = match game_state
                .winner
                .expect("game_over is true but winner is None")
            {
                game::Player::Left => "YOU WIN!",
                game::Player::Right => "BOT WINS",
            };
            Some(ui::OverlayMessage::info(vec![
                winner_text.to_string(),
                "".to_string(),
                "R to Rematch  |  Q to Quit".to_string(),
            ]))
        } else {
            None
        };

        terminal.draw(|f| {
            ui::render(
                f,
                &game_state,
                None,
                overlay.as_ref(),
                Some(game::Player::Left),
            )
        })?;

        // Frame rate limiting
        limit_frame_rate(now);
    }
}
