# P2Pong Testing Guide

## Day 3 - gossipsub with Host-Authoritative Ball Physics

### Quick Test (Localhost)

**Terminal 1 - Host (controls left paddle):**
```bash
cargo run --release -- --listen
```
- Prints libp2p info to stderr (Local peer id, listening address)
- Shows braille spinner: `⠋ Waiting for opponent to connect...`
- Game does NOT start until client connects (no TUI yet)
- Copy the multiaddr from the output (format: `/ip4/127.0.0.1/tcp/4001/p2p/12D3Koo...`)
- Once connected: TUI launches, controls left paddle ONLY with `W` (up) / `S` (down)
- Arrow keys are DISABLED (opponent controls right paddle)
- Runs ball physics and broadcasts ball state every 30 frames (~0.5s)

**Terminal 2 - Client (controls right paddle):**
```bash
cargo run --release -- --connect /ip4/127.0.0.1/tcp/4001/p2p/<PEER_ID>
```
- Shows braille spinner: `⠋ Connecting to host...`
- Once connected: TUI launches automatically
- Controls right paddle ONLY with `↑` (up) / `↓` (down)
- W/S keys are DISABLED (opponent controls left paddle)
- Receives ball state from host
- Sends right paddle inputs to host

**Expected Behavior:**
1. Host terminal shows:
   ```
   Local peer id: 12D3Koo...
   🎧 Listening on /ip4/127.0.0.1/tcp/4001/p2p/12D3Koo...
   📻 Subscribed to topic: p2pong-game
   Share this address with your opponent:
     /ip4/127.0.0.1/tcp/4001/p2p/12D3Koo...
   ⠋ Waiting for opponent to connect...
   ```
2. Client terminal shows:
   ```
   Local peer id: 12D3Koo...
   Connecting to /ip4/127.0.0.1/tcp/4001/p2p/12D3Koo...
   📻 Subscribed to topic: p2pong-game
   ⠋ Connecting to host...
   ```
3. Once connected, both terminals show:
   ```
   ✅ Connection established with 12D3Koo...
   ✅ Connected! Starting game...
   ```
4. TUI launches on both screens simultaneously
5. Game starts fresh (ball at center, paddles at middle, score 0-0)
6. Host (Terminal 1) can ONLY control left paddle with W/S (arrow keys ignored)
7. Client (Terminal 2) can ONLY control right paddle with arrow keys (W/S ignored)
8. Each player's paddle movement appears on both screens
9. Ball moves and bounces (physics run by host)
10. Ball position syncs to client every 30 frames
11. Paddles move in real-time on both screens
12. Scoring works correctly
13. Pressing Q exits the game cleanly on either side

### What Was Implemented (Day 3)

#### 0. Connection Waiting (Before TUI)
- **Files**: `src/network/client.rs`, `src/network/mod.rs`, `src/network/runtime.rs`, `src/main.rs`
- Added `is_connected()` method to NetworkClient using atomic bool
- Wait for connection happens BEFORE entering TUI mode
- Shows braille spinner on stderr: `⠋ Waiting for opponent to connect...`
- No TUI until connection established - prevents weird state issues
- Cannot accidentally start game by pressing Q while waiting
- Fixes issue where host would start playing alone

#### 1. gossipsub Protocol
- **File**: `src/network/behaviour.rs`
- Replaced ping-only behaviour with gossipsub pub/sub
- Topic: `"p2pong-game"`
- Message authentication: Signed with peer keypair
- Much simpler than request-response (140 fewer lines!)

#### 2. Network Messages
- **File**: `src/network/protocol.rs`
- `NetworkMessage::Input(InputAction)` - Paddle movements
- `NetworkMessage::BallSync(BallState)` - Ball position/velocity from host
- `BallState` struct with `x, y, vx, vy` fields

#### 3. Host-Authoritative Ball Physics
- **File**: `src/main.rs`
- Host runs full physics (paddles + ball)
- Client runs paddle physics, receives ball state from host
- Host broadcasts `BallSync` every 30 frames
- Client overwrites local ball state with host's authoritative state
- Prevents physics divergence due to floating-point drift

#### 4. Fixed Player Roles
- Host always controls **left paddle** (W/S keys)
- Client always controls **right paddle** (↑/↓ keys)
- Input filtering: Players only send inputs for their own paddle
- Local mode: Single player controls both paddles

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    GAME LOOP (main.rs)                      │
│  - Polls local input (keyboard)                             │
│  - Receives remote input via channels                       │
│  - Updates physics (host: full, client: ball sync)          │
│  - Renders at 60 FPS                                        │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  │ mpsc channels
                  │
┌─────────────────▼───────────────────────────────────────────┐
│              NETWORK THREAD (runtime.rs)                    │
│  - Async tokio runtime                                      │
│  - libp2p Swarm with gossipsub                              │
│  - Subscribes to "p2pong-game" topic                        │
│  - Publishes messages (InputAction, BallSync)               │
│  - Filters loopback messages (ignore own)                   │
└─────────────────────────────────────────────────────────────┘
```

### Message Flow

**Input Messages:**
```
Host presses W → SendInput(LeftPaddleUp) → gossipsub.publish()
                                          ↓
Client receives → ReceivedInput(LeftPaddleUp) → moves left paddle
```

**Ball Sync Messages (every 30 frames):**
```
Host runs physics → BallSync{x,y,vx,vy} → gossipsub.publish()
                                        ↓
Client receives → ReceivedBallState → overwrites ball position
```

### Known Limitations

1. **Best-effort delivery**: gossipsub doesn't guarantee message delivery
   - Acceptable for game inputs at 60 FPS (occasional drops are fine)
   - Ball syncs every 0.5s provide error correction

2. **No late-join**: Client must connect before game starts
   - No state snapshot on connection
   - Future: Could send full game state on connect

3. **Host advantage**: Host sees ball with zero latency
   - Client sees ball ~0.5s delayed (sync interval)
   - Trade-off for simple implementation

4. **No collision prediction**: Client doesn't predict paddle-ball collisions
   - Future: Could run speculative physics and reconcile on sync

### Next Steps (Day 4?)

- [ ] Implement connection handshake (exchange player IDs)
- [ ] Add scoreboard sync (currently local only)
- [ ] Test over real network (not just localhost)
- [ ] Add lag compensation / prediction
- [ ] Implement graceful reconnection
- [ ] Add matchmaking / peer discovery (mDNS or DHT)

### Debug Tips

If things don't work:
1. Check both terminals show "📻 Subscribed to topic: p2pong-game"
2. Verify connection: "✅ Connection established"
3. Check firewall allows TCP on port 4001
4. Try increasing ball sync rate (change `frame_count % 30` to `frame_count % 10`)
5. Add debug prints to see messages being sent/received
