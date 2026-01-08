# Real Network Testing Guide

## Phase A Complete: 0.0.0.0 Listening ✅

The game now listens on `0.0.0.0` instead of `127.0.0.1`, which means it accepts connections from any network interface (localhost, LAN, etc.).

## Testing on Two Computers (Same LAN)

### Step 1: Find Your LAN IP (Computer 1 - Host)

**macOS/Linux:**
```bash
ifconfig | grep 'inet ' | grep -v 127.0.0.1
```
Example output: `inet 192.168.1.179`

**Windows:**
```bash
ipconfig
```
Look for "IPv4 Address" under your WiFi adapter (e.g., `192.168.1.179`)

### Step 2: Start Host (Computer 1)

```bash
cd /path/to/p2pong
cargo run --release -- --listen
```

**Expected output:**
```
Local peer id: 12D3KooWABC123...
🎧 Listening on /ip4/0.0.0.0/tcp/4001/p2p/12D3KooWABC123...
📻 Subscribed to topic: p2pong-game

Share this address with your opponent:
  Replace 0.0.0.0 with your LAN IP address:
  /ip4/<YOUR_IP>/tcp/4001/p2p/12D3KooWABC123...

💡 Find your LAN IP:
  macOS/Linux: ifconfig | grep 'inet ' | grep -v 127.0.0.1
  Windows:     ipconfig

⠋ Waiting for opponent to connect...
```

**Copy the peer ID** (the long string starting with `12D3Koo...`)

### Step 3: Build the Connection Address

Combine your LAN IP with the peer ID:
```
/ip4/192.168.1.179/tcp/4001/p2p/12D3KooWABC123...
```

### Step 4: Transfer P2Pong to Computer 2

**Option A - Git Clone:**
```bash
git clone <your-repo> p2pong
cd p2pong
cargo build --release
```

**Option B - Copy Binary:**
```bash
# On Computer 1
scp target/release/p2pong user@computer2:~/

# On Computer 2
chmod +x ~/p2pong
```

### Step 5: Connect from Computer 2 (Client)

```bash
./p2pong --connect /ip4/192.168.1.179/tcp/4001/p2p/12D3KooWABC123...
```

**Expected output:**
```
Local peer id: 12D3KooXYZ789...
Connecting to /ip4/192.168.1.179/tcp/4001/p2p/12D3KooWABC123...
📻 Subscribed to topic: p2pong-game
⠋ Connecting to host...
```

Then:
```
✅ Connection established with 12D3KooWABC123...
✅ Connected! Starting game...

[TUI launches - both screens show Pong game]
```

### Step 6: Play!

- **Computer 1 (Host):** Controls left paddle with W/S
- **Computer 2 (Client):** Controls right paddle with ↑/↓
- Both screens should show:
  - Synchronized ball movement
  - Real-time paddle updates
  - Same score (MIGHT have edge case bugs - we'll fix in Phase C)

## Troubleshooting

### "Connection timeout" or "No route to host"

**Cause:** Firewall blocking TCP port 4001

**Solution (macOS):**
```bash
# This is usually not needed on macOS for LAN connections
# But if issues persist, check System Preferences > Security & Privacy > Firewall
```

**Solution (Linux):**
```bash
sudo ufw allow 4001/tcp
# OR disable firewall temporarily for testing
sudo ufw disable
```

**Solution (Windows):**
1. Windows Defender Firewall > Advanced Settings
2. Inbound Rules > New Rule
3. Port > TCP > Specific local ports: 4001
4. Allow the connection

### "Connection refused"

**Cause:** Host isn't listening yet

**Solution:** Make sure Computer 1 shows "Waiting for opponent..." before Computer 2 connects

### Computers can't see each other

**Cause:** Different networks (one on WiFi, one on Ethernet, etc.)

**Solution:** Make sure both computers on same network
```bash
# On both computers, check you're on same subnet
ifconfig  # Look for 192.168.1.x or similar
```

### Score desyncs

**Expected:** This is a known issue we'll fix in Phase C!

The client might show different scores than the host if there's network lag during scoring.

**Workaround:** Restart the game (this is temporary until we add ScoreSync)

## Success Criteria

✅ Both computers connect successfully  
✅ Spinner shows while connecting  
✅ TUI launches on both screens  
✅ Ball moves on both screens (roughly synchronized)  
✅ Paddles move in real-time  
✅ Q exits cleanly on both sides  

⚠️ Scores MIGHT desync (fix coming in Phase C)

## Next Steps

Once real network testing works:
- **Phase C:** Add explicit score sync to fix desync bugs
- **Phase B:** Add mDNS auto-discovery (no more copy/paste addresses!)

## Notes

- Default port: 4001 (can change with `--listen 5000`)
- Listening on 0.0.0.0 means all network interfaces (safe on private LAN)
- libp2p uses TCP transport with Noise encryption and Yamux multiplexing
- Ball is synced every 30 frames (~0.5 seconds at 60 FPS)

Have fun! 🎮
