# Testing the Professional Netcode

## Quick Test (Same Computer)

**Terminal 1 (Host):**
```bash
cargo run --release -- --listen
```

**Terminal 2 (Client):**
```bash
cargo run --release -- --connect /ip4/127.0.0.1/tcp/4001/p2p/<PEER_ID>
```

### What to Look For

✅ **Ball movement should be:**
- Smooth and continuous (no stuttering)
- Same on both screens
- No visible "jumps" or "teleporting"

✅ **Scores should:**
- Match exactly on both screens
- Update simultaneously when goal scored
- Never desync (even if you play for 10 minutes)

✅ **Paddle controls should:**
- Be responsive (no lag on your own paddle)
- Show opponent's paddle moving smoothly

---

## LAN Test (Two Computers)

**Computer 1 (Host):**
```bash
# Find your IP
ifconfig | grep 'inet ' | grep -v 127.0.0.1
# Example: inet 192.168.1.179

cargo run --release -- --listen
```

**Computer 2 (Client):**
```bash
cargo run --release -- --connect /ip4/192.168.1.179/tcp/4001/p2p/<PEER_ID>
```

### Expected Behavior

**On LAN (same WiFi):**
- Ball movement: Buttery smooth, indistinguishable from local play
- Prediction errors: <2 virtual units (invisible)
- Corrections: So gentle you won't notice them
- Sync latency: 1-2 frames (~16-33ms)

**What you should NOT see:**
- ❌ Stuttering or rubber-banding
- ❌ Ball "jumping" positions
- ❌ Score mismatches
- ❌ Jerky opponent paddle movement

---

## Stress Test

### Test 1: Rapid Rally
Play a long rally with fast paddle movements.

**Expected:**
- Ball syncs frequently (on each paddle hit)
- Client stays perfectly in sync
- No accumulating error

### Test 2: Score Test
Play until someone scores 5 points and wins.

**Expected:**
- Scores update instantly on both screens
- Both players see same winner
- Game over state synced

### Test 3: Pause-and-Resume
On the **CLIENT** computer, pause the game (Cmd+Z or minimize).  
Wait 2-3 seconds, then resume (fg).

**Expected:**
- Client catches up smoothly within 1 second
- No hard snaps or teleporting
- Ball might snap once (acceptable), then smooth again

---

## Debug Mode (Advanced)

Want to see what's happening under the hood?

### Add Debug Logging

Edit `src/main.rs` around line 225:

```rust
NetworkEvent::ReceivedBallState(ball_state) => {
    if matches!(player_role, PlayerRole::Client) {
        let dx = game_state.ball.x - ball_state.x;
        let dy = game_state.ball.y - ball_state.y;
        let error = (dx*dx + dy*dy).sqrt();
        
        // DEBUG: Print error
        eprintln!("Prediction error: {:.2} units", error);
        
        if error < ERROR_THRESHOLD {
            // ... rest of code
```

**Run and watch stderr:**
```bash
cargo run --release -- --connect ... 2> client_errors.log
tail -f client_errors.log
```

**What to expect:**
- Errors typically 0.5-2.0 units on LAN
- Spikes to 5-10 units after paddle hits (then corrects quickly)
- Should never exceed 15-20 units

---

## Performance Metrics

### Network Traffic (can monitor with Wireshark)

**Normal rally:**
- Input messages: ~60/sec (one per paddle tap)
- Ball syncs: ~3-5 per rally (paddle hits + backup timer)
- Score syncs: Only when points scored

**Total bandwidth:**
- Input: ~60 msgs/sec × ~50 bytes = ~3 KB/sec
- Ball sync: ~5 msgs/sec × ~100 bytes = ~0.5 KB/sec
- **Total: <4 KB/sec** (trivial for modern networks)

### CPU Usage

Should be negligible:
- Physics: Simple arithmetic (< 1% CPU)
- Network: Serialization overhead minimal
- Rendering: Already optimized (60 FPS lock)

---

## Tuning for Different Networks

### **LAN / Low Latency (<10ms)**

Default settings are perfect:
```rust
const ERROR_THRESHOLD: f32 = 10.0;
const CORRECTION_FACTOR: f32 = 0.25;
const BACKUP_SYNC_INTERVAL: u64 = 60;
```

### **Good Internet (20-50ms)**

No changes needed, should work great.

### **High Latency Internet (100-200ms)**

Edit `src/main.rs`:
```rust
const ERROR_THRESHOLD: f32 = 20.0;        // More tolerance
const CORRECTION_FACTOR: f32 = 0.15;      // Gentler corrections
const BACKUP_SYNC_INTERVAL: u64 = 30;     // More frequent syncs
```

### **Terrible Internet (>200ms or packet loss)**

```rust
const ERROR_THRESHOLD: f32 = 30.0;        // Very tolerant
const CORRECTION_FACTOR: f32 = 0.10;      // Very gentle
const BACKUP_SYNC_INTERVAL: u64 = 20;     // Frequent syncs
```

**Note:** At >200ms, game might not be playable anyway (reaction time issue).

---

## Known Issues & Expected Behavior

### ✅ **Expected (Normal)**

1. **Rare gentle "pull" of ball position**
   - Happens when client prediction drifts slightly
   - Correction is smooth over 3-4 frames
   - Barely noticeable

2. **Occasional snap after pause**
   - If you pause one client, it will snap once when resumed
   - Normal behavior for extreme desync

3. **Opponent paddle lag**
   - Opponent's paddle has network latency (your paddle is instant)
   - This is expected and unavoidable

### ❌ **Problems (Report These)**

1. **Constant stuttering**
   - Ball jitters every frame
   - Indicates sync issues or network problems

2. **Score desync**
   - Screens show different scores
   - Should never happen - indicates bug

3. **Ball teleporting frequently**
   - Ball jumps >20 units constantly
   - Indicates prediction totally failing

---

## Success Criteria

After testing, you should observe:

- ✅ Ball moves smoothly 95% of the time
- ✅ Scores always match
- ✅ No rubber-banding
- ✅ Feels nearly identical to local play (on LAN)
- ✅ Corrections are invisible or barely noticeable

**If you see these, the netcode is working perfectly!**

---

## Comparison Test

### **Before (Old Netcode)**
1. Run old version (git checkout before netcode changes)
2. Play a game
3. Notice: Choppy ball, rubber-banding, occasional score desync

### **After (Professional Netcode)**
1. Run new version (current)
2. Play a game
3. Notice: Smooth as butter, perfect sync, no jitter

**The difference should be night and day!**

---

## Next Steps

Once you've validated smoothness on LAN:
1. Test over real internet (different networks)
2. Try high-latency scenarios (mobile hotspot, VPN)
3. Tune parameters if needed
4. Add mDNS auto-discovery (Phase B)

Have fun! 🎮
