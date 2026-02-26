// Menu state management and game mode definitions

use crate::ai::BotType;

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
    /// Host P2P game (will display peer ID for others to join) - WebRTC (Legacy)
    NetworkHost,
    /// Join P2P game with peer ID - WebRTC (Legacy)
    NetworkClient(String),
    /// Host TCP game (create room on server)
    TcpHost,
    /// Join TCP game (join room by ID)
    TcpClient(String),
    /// Single player vs AI opponent
    SinglePlayerAI(BotType),
}

/// Menu items
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuItem {
    LocalTwoPlayer,
    HostTcp,
    JoinTcp,
    HostP2P, // Legacy WebRTC
    JoinP2P, // Legacy WebRTC
    SinglePlayerAI,
    Quit,
}

impl MenuItem {
    /// Get display text for menu item
    pub fn display_text(&self) -> &str {
        match self {
            MenuItem::LocalTwoPlayer => "Local 2-Player",
            MenuItem::HostTcp => "Host Online Game",
            MenuItem::JoinTcp => "Join Online Game",
            MenuItem::HostP2P => "Host P2P Game (Legacy)",
            MenuItem::JoinP2P => "Join P2P Game (Legacy)",
            MenuItem::SinglePlayerAI => "Single Player vs AI",
            MenuItem::Quit => "Quit",
        }
    }

    /// Get all menu items in order
    pub fn all() -> Vec<MenuItem> {
        vec![
            MenuItem::LocalTwoPlayer,
            MenuItem::HostTcp,
            MenuItem::JoinTcp,
            MenuItem::HostP2P,
            MenuItem::JoinP2P,
            MenuItem::SinglePlayerAI,
            MenuItem::Quit,
        ]
    }
}

/// Input mode type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    TcpRoomId,
    P2pPeerId,
}

/// Menu state
pub struct MenuState {
    /// Currently selected menu item index
    pub selected_index: usize,
    /// All menu items
    pub items: Vec<MenuItem>,
    /// Peer/Room ID input buffer
    pub peer_id_input: String,
    /// Whether currently in input mode
    pub in_input_mode: bool,
    /// Type of input being entered
    pub input_mode_type: Option<InputMode>,
    /// Whether currently in bot selection mode
    pub in_bot_selection_mode: bool,
    /// Selected bot index during selection
    pub selected_bot_index: usize,
    /// Available bots
    pub available_bots: Vec<BotType>,
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            items: MenuItem::all(),
            peer_id_input: String::new(),
            in_input_mode: false,
            input_mode_type: None,
            in_bot_selection_mode: false,
            selected_bot_index: 0,
            available_bots: BotType::all(),
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

    /// Enter peer ID input mode (for P2P)
    pub fn start_peer_id_input(&mut self) {
        self.in_input_mode = true;
        self.input_mode_type = Some(InputMode::P2pPeerId);
        self.peer_id_input.clear();
    }

    /// Enter room ID input mode (for TCP)
    pub fn start_room_id_input(&mut self) {
        self.in_input_mode = true;
        self.input_mode_type = Some(InputMode::TcpRoomId);
        self.peer_id_input.clear();
    }

    /// Exit input mode
    pub fn cancel_peer_id_input(&mut self) {
        self.in_input_mode = false;
        self.input_mode_type = None;
        self.peer_id_input.clear();
    }

    /// Get input and exit input mode (converts to uppercase for P2P peer IDs)
    pub fn submit_peer_id(&mut self) -> String {
        self.in_input_mode = false;
        let result = match self.input_mode_type {
            Some(InputMode::P2pPeerId) => self.peer_id_input.to_uppercase(),
            Some(InputMode::TcpRoomId) => self.peer_id_input.clone(),
            None => self.peer_id_input.clone(),
        };
        self.input_mode_type = None;
        result
    }

    /// Add character to peer ID input
    pub fn add_char_to_peer_id(&mut self, c: char) {
        self.peer_id_input.push(c);
    }

    /// Remove last character from peer ID input
    pub fn backspace_peer_id(&mut self) {
        self.peer_id_input.pop();
    }

    /// Enter bot selection mode
    pub fn start_bot_selection(&mut self) {
        self.in_bot_selection_mode = true;
        self.selected_bot_index = 0;
        self.available_bots = BotType::all();
    }

    /// Exit bot selection mode
    pub fn cancel_bot_selection(&mut self) {
        self.in_bot_selection_mode = false;
    }

    /// Move selection up in bot list
    pub fn select_previous_bot(&mut self) {
        if self.selected_bot_index > 0 {
            self.selected_bot_index -= 1;
        } else {
            self.selected_bot_index = self.available_bots.len() - 1;
        }
    }

    /// Move selection down in bot list
    pub fn select_next_bot(&mut self) {
        if self.selected_bot_index < self.available_bots.len() - 1 {
            self.selected_bot_index += 1;
        } else {
            self.selected_bot_index = 0;
        }
    }

    /// Get bot type and exit selection mode
    pub fn submit_bot_selection(&mut self) -> BotType {
        self.in_bot_selection_mode = false;
        self.available_bots[self.selected_bot_index]
    }
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}
