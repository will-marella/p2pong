mod game;
mod ui;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::{Duration, Instant};

use game::{GameState, InputAction, InputState};

const TARGET_FPS: u64 = 60;
const FRAME_DURATION: Duration = Duration::from_millis(1000 / TARGET_FPS);

fn main() -> Result<(), io::Error> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run game
    let result = run_game(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_game<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> Result<(), io::Error> {
    let mut last_frame = Instant::now();
    let mut input_state = InputState::new();
    
    // Initialize game state with terminal dimensions
    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height);

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;

        // Check for terminal resize
        let size = terminal.size()?;
        if size.width as f32 != game_state.field_width || size.height as f32 != game_state.field_height {
            game_state.resize(size.width, size.height);
        }

        // Handle input
        let actions = input_state.poll(Duration::from_millis(1))?;
        for action in actions {
            match action {
                InputAction::Quit => return Ok(()),
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.left_paddle);
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.left_paddle);
                }
                InputAction::LeftPaddleStop => {
                    game::physics::stop_paddle(&mut game_state.left_paddle);
                }
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.right_paddle);
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.right_paddle);
                }
                InputAction::RightPaddleStop => {
                    game::physics::stop_paddle(&mut game_state.right_paddle);
                }
            }
        }

        // Update game physics
        game::update(&mut game_state, dt);

        // Render
        terminal.draw(|f| ui::render(f, &game_state))?;

        // Frame rate limiting
        let elapsed = now.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }
}
