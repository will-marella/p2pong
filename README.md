# P2Pong - Terminal Pong Game

A classic Pong game implemented in Rust with a beautiful terminal UI.

## Phase 1: Solo Terminal Pong (COMPLETED)

Single-player Pong where one person controls both paddles.

### Features

- **60 FPS smooth gameplay**
- **Virtual coordinate system** - Game runs in 200×100 virtual space, scales to ANY terminal size
- **Adaptive rendering** - Automatically adjusts to terminal dimensions (supports quarter screen to fullscreen)
- **Classic Pong aesthetics** - White paddles (3 rows × 1-2 chars scaled), white ball
- **Angle-based ball physics** - Bounce angle depends on where ball hits paddle
- **First to 5 points wins**
- **Smooth paddle movement**
- **Unicode graphics** - Clean terminal rendering with box drawing characters

### Controls

- **W/S**: Move left paddle up/down
- **↑/↓**: Move right paddle up/down
- **Q/ESC**: Quit game

### Build & Run

```bash
# Build the project
cargo build --release

# Run the game
cargo run --release

# Test with different terminal sizes
./test_sizes.sh
```

**Tip:** Try resizing your terminal while playing! The game will adapt in real-time.

### Architecture

#### Virtual Coordinate System

The game uses a **virtual coordinate system (200×100)** that is independent of terminal size:

- **Physics layer** runs in virtual coordinates (deterministic, same for all players)
- **Render layer** maps virtual → screen coordinates (adaptive to terminal size)
- **Benefits:**
  - Works on any terminal size (60×20 to 200×50+)
  - P2P-ready: Players with different terminal sizes see the same game
  - No fractional coordinate limitations

```
p2pong/
├── src/
│   ├── main.rs           # Entry point & game loop (60 FPS)
│   ├── game/
│   │   ├── mod.rs        # Module exports
│   │   ├── state.rs      # Game state in VIRTUAL coordinates
│   │   ├── physics.rs    # Physics engine (virtual space)
│   │   └── input.rs      # Keyboard input handling
│   └── ui/
│       ├── mod.rs        # Module exports
│       └── render.rs     # CoordMapper: virtual → screen coords
```

### Tech Stack

- **Rust** - Systems programming language
- **ratatui 0.28** - Terminal UI library
- **crossterm 0.28** - Terminal manipulation

### Next Steps (Planned)

- Phase 2: Local P2P (connect two terminals on same machine)
- Phase 3: libp2p integration (DHT discovery, NAT traversal)
- Phase 4: Matchmaking (Rendezvous protocol)
- Phase 5: Polish (error handling, rankings, stats)
