use std::io;
use std::time::Instant;

use ratatui::Terminal;

use crate::config::Config;
use crate::debug;
use crate::game::{self, poll_input_local_2p, GameState, InputAction};
use crate::ui;
use crate::FIXED_TIMESTEP;

use super::common::limit_frame_rate;

/// Run local 2-player game (no networking)
pub fn run_game_local<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
) -> Result<(), io::Error> {
    debug::log("GAME_START", "Local 2-player mode");

    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height);

    loop {
        let now = Instant::now();

        // Handle input (both paddles)
        let actions = poll_input_local_2p(config)?;

        for action in &actions {
            match action {
                InputAction::Quit => return Ok(()),
                InputAction::Rematch => {
                    if game_state.game_over {
                        game_state.reset_game();
                    }
                }
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(
                        &mut game_state.left_paddle,
                        game_state.field_height,
                    );
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(
                        &mut game_state.left_paddle,
                        game_state.field_height,
                    );
                }
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(
                        &mut game_state.right_paddle,
                        game_state.field_height,
                    );
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(
                        &mut game_state.right_paddle,
                        game_state.field_height,
                    );
                }
            }
        }

        // Update physics
        let _events = game::update_with_events(&mut game_state, FIXED_TIMESTEP);

        // Create overlay message if game is over
        let overlay = if game_state.game_over {
            let winner_text = match game_state.winner.unwrap() {
                game::Player::Left => "LEFT WINS",
                game::Player::Right => "RIGHT WINS",
            };
            Some(ui::OverlayMessage::info(vec![
                winner_text.to_string(),
                "".to_string(),
                "R to Rematch  |  Q to Quit".to_string(),
            ]))
        } else {
            None
        };

        terminal.draw(|f| ui::render(f, &game_state, None, overlay.as_ref(), None))?;

        // Frame rate limiting
        limit_frame_rate(now);
    }
}
