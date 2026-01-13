// Menu state management and game mode definitions

/// Application state machine
#[derive(Debug, Clone)]
pub enum AppState {
    /// Currently in the main menu
    Menu,
    /// Currently playing a game
    Game(GameMode),
    /// Graceful shutdown
    Exiting,
}

/// Game mode selection
#[derive(Debug, Clone)]
pub enum GameMode {
    /// Local 2-player on same keyboard
    LocalTwoPlayer,
    /// Host P2P game (will display peer ID for others to join)
    NetworkHost,
    /// Join P2P game with peer ID
    NetworkClient(String),
    /// Single player vs AI opponent
    SinglePlayerAI,
}

/// Menu items
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuItem {
    LocalTwoPlayer,
    HostP2P,
    JoinP2P,
    SinglePlayerAI,
    Quit,
}

impl MenuItem {
    /// Get display text for menu item
    pub fn display_text(&self) -> &str {
        match self {
            MenuItem::LocalTwoPlayer => "Local 2-Player",
            MenuItem::HostP2P => "Host P2P Game",
            MenuItem::JoinP2P => "Join P2P Game",
            MenuItem::SinglePlayerAI => "Single Player vs AI",
            MenuItem::Quit => "Quit",
        }
    }

    /// Get all menu items in order
    pub fn all() -> Vec<MenuItem> {
        vec![
            MenuItem::LocalTwoPlayer,
            MenuItem::HostP2P,
            MenuItem::JoinP2P,
            MenuItem::SinglePlayerAI,
            MenuItem::Quit,
        ]
    }
}

/// Menu state
pub struct MenuState {
    /// Currently selected menu item index
    pub selected_index: usize,
    /// All menu items
    pub items: Vec<MenuItem>,
    /// Peer ID input buffer (for Join mode)
    pub peer_id_input: String,
    /// Whether currently in peer ID input mode
    pub in_input_mode: bool,
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            items: MenuItem::all(),
            peer_id_input: String::new(),
            in_input_mode: false,
        }
    }

    /// Get currently selected menu item
    pub fn selected_item(&self) -> MenuItem {
        self.items[self.selected_index]
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = self.items.len() - 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected_index < self.items.len() - 1 {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }
    }

    /// Enter peer ID input mode
    pub fn start_peer_id_input(&mut self) {
        self.in_input_mode = true;
        self.peer_id_input.clear();
    }

    /// Exit peer ID input mode
    pub fn cancel_peer_id_input(&mut self) {
        self.in_input_mode = false;
        self.peer_id_input.clear();
    }

    /// Get peer ID and exit input mode
    pub fn submit_peer_id(&mut self) -> String {
        self.in_input_mode = false;
        self.peer_id_input.clone()
    }

    /// Add character to peer ID input
    pub fn add_char_to_peer_id(&mut self, c: char) {
        self.peer_id_input.push(c);
    }

    /// Remove last character from peer ID input
    pub fn backspace_peer_id(&mut self) {
        self.peer_id_input.pop();
    }
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}
