# P2Pong Changelog

## Day 3 - Part 2: Connection Waiting Screen

### Fixed: Game starting before connection established

**Problem:**
- When running `cargo run --release -- --listen`, the game started immediately
- Host would be playing alone while waiting for opponent
- Ball could already be in motion, score could be non-zero when client connects
- Pressing Q while waiting would start the game (because TUI was already running)
- Poor user experience

**Solution:**
Wait for connection BEFORE entering TUI mode. Show simple braille spinner on stderr.

**Changes:**

1. **`src/network/client.rs`:**
   - Added `connected: Arc<AtomicBool>` field to `NetworkClient`
   - Added `is_connected()` method to check connection state
   - Updated constructor to accept connection flag

2. **`src/network/mod.rs`:**
   - Create shared `Arc<AtomicBool>` for connection state
   - Pass to both runtime and client

3. **`src/network/runtime.rs`:**
   - Accept `connected` flag in `spawn_network_thread()` and `run_network()`
   - Set `connected.store(true)` on `ConnectionEstablished` event
   - Set `connected.store(false)` on `ConnectionClosed` event
   - Added atomic imports

4. **`src/main.rs`:**
   - Moved network initialization to BEFORE terminal setup
   - Added `wait_for_connection()` function (runs before TUI)
   - Shows braille spinner on stderr with appropriate message:
     - Host: `⠋ Waiting for opponent to connect...`
     - Client: `⠋ Connecting to host...`
   - Polls for connection state and network events
   - TUI only starts AFTER connection is established
   - Changed `run_game()` to accept network client and player role as parameters

**Result:**
- Host sees spinner on stderr until client connects (no TUI yet)
- Client sees spinner on stderr until connection established
- TUI launches simultaneously on both terminals once connected
- Game starts fresh (ball at center, score 0-0) for both players
- Cannot accidentally start game by pressing Q while waiting
- Much cleaner UX!

**Testing:**
```bash
# Terminal 1 - Host
cargo run --release -- --listen
# Prints peer ID and listening address to stderr
# Shows: ⠋ Waiting for opponent to connect...
# NO TUI yet - just spinner on stderr

# Terminal 2 - Client  
cargo run --release -- --connect /ip4/127.0.0.1/tcp/4001/p2p/<PEER_ID>
# Shows: ⠋ Connecting to host...
# NO TUI yet - just spinner on stderr

# Both terminals
# - See "✅ Connection established with 12D3Koo..."
# - See "✅ Connected! Starting game..."
# - TUI launches
# - Game starts fresh (ball at center, score 0-0)
```

---

## Day 3 - Part 1: gossipsub Message Exchange

### Implemented P2P message exchange with host-authoritative ball physics

**Changes:**
- Replaced ping-only behaviour with gossipsub pub/sub
- Added `NetworkMessage::BallSync(BallState)` for ball synchronization
- Host runs ball physics, client receives synced state
- Fixed player roles: Host = left paddle, Client = right paddle
- Input filtering: only send your own paddle movements

**Files:**
- Created `src/network/behaviour.rs` (replaced `simple_behaviour.rs`)
- Updated `src/network/runtime.rs` with gossipsub subscription
- Updated `src/network/client.rs` with new event/command types
- Updated `src/network/protocol.rs` with BallState
- Updated `src/main.rs` with host-authoritative physics
- Updated `Cargo.toml` with gossipsub feature

See `TESTING.md` for full details.

---

## Day 2: libp2p Connectivity

- Implemented basic P2P connection using libp2p
- TCP transport with Noise encryption and Yamux multiplexing
- CLI arguments: `--listen` (host) and `--connect` (client)
- Ping protocol for connection verification
- Channel-based communication between network thread and game loop

---

## Day 1: P2P Foundation

- Added network module structure
- Created message protocol with serialization
- Made `InputAction` serializable
- Designed async/sync bridge architecture
