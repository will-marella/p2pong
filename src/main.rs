mod game;
mod ui;
mod network;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::{Duration, Instant};

use game::{GameState, InputAction, poll_input};
use network::ConnectionMode;

const TARGET_FPS: u64 = 60;
const FRAME_DURATION: Duration = Duration::from_millis(1000 / TARGET_FPS);

fn main() -> Result<(), io::Error> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let network_mode = parse_args(&args)?;
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run game
    let result = run_game(&mut terminal, network_mode);

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

/// Parse command line arguments for network mode
fn parse_args(args: &[String]) -> Result<Option<ConnectionMode>, io::Error> {
    if args.len() == 1 {
        // No arguments - local mode (no networking)
        return Ok(None);
    }
    
    match args[1].as_str() {
        "--listen" | "-l" => {
            let port = if args.len() > 2 {
                args[2].parse().unwrap_or(4001)
            } else {
                4001
            };
            Ok(Some(ConnectionMode::Listen { port }))
        }
        "--connect" | "-c" => {
            if args.len() < 3 {
                eprintln!("Error: --connect requires a multiaddr argument");
                eprintln!("Usage: {} --connect <multiaddr>", args[0]);
                std::process::exit(1);
            }
            Ok(Some(ConnectionMode::Connect {
                multiaddr: args[2].clone(),
            }))
        }
        "--help" | "-h" => {
            print_usage(&args[0]);
            std::process::exit(0);
        }
        _ => {
            eprintln!("Unknown argument: {}", args[1]);
            print_usage(&args[0]);
            std::process::exit(1);
        }
    }
}

fn print_usage(program: &str) {
    println!("P2Pong - Peer-to-Peer Terminal Pong");
    println!();
    println!("Usage:");
    println!("  {}                    # Local mode (no networking)", program);
    println!("  {} --listen [port]    # Host a game (default port: 4001)", program);
    println!("  {} --connect <addr>   # Connect to a game", program);
    println!();
    println!("Examples:");
    println!("  {}  --listen", program);
    println!("  {}  --connect /ip4/127.0.0.1/tcp/4001/p2p/12D3Koo...", program);
}

fn run_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    network_mode: Option<ConnectionMode>,
) -> Result<(), io::Error> {
    let mut last_frame = Instant::now();
    
    // Initialize network if in multiplayer mode
    let network_client = if let Some(mode) = network_mode {
        Some(network::start_network(mode)?)
    } else {
        None
    };
    
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

        // Handle local input
        let local_actions = poll_input(Duration::from_millis(1))?;
        
        // Handle remote input (if networked)
        let remote_actions = if let Some(ref client) = network_client {
            client.recv_inputs()
        } else {
            Vec::new()
        };
        
        // Process all actions (local + remote)
        for action in local_actions.iter().chain(remote_actions.iter()) {
            match action {
                InputAction::Quit => return Ok(()),
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.left_paddle, game_state.field_height);
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.left_paddle, game_state.field_height);
                }
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.right_paddle, game_state.field_height);
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.right_paddle, game_state.field_height);
                }
            }
        }
        
        // Send local inputs to opponent (if networked)
        if let Some(ref client) = network_client {
            for action in &local_actions {
                if *action != InputAction::Quit {
                    let _ = client.send_input(*action);
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
