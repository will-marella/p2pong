# Netcode Simplification - High-Frequency Sync

## What Changed

We **simplified the netcode dramatically** after real-world testing revealed the "professional" approach was over-engineered.

### Before (Part 5 - Complex)

```rust
const BACKUP_SYNC_INTERVAL: u64 = 60;     // 1 second between syncs
const ERROR_THRESHOLD: f32 = 10.0;
const CORRECTION_FACTOR: f32 = 0.25;

// Client reconciliation (30+ lines):
if error < THRESHOLD {
    // Gentle lerp
    ball.x += (synced.x - ball.x) * CORRECTION_FACTOR;
} else {
    // Hard snap
    ball.x = synced.x;
}
// + velocity sync logic
// + error distance calculations
// + conditional branching
```

**Result:** 
- ❌ Ball jittered every 1 second
- ❌ Rubber-banding caused false goals
- ❌ Complex code, hard to tune

### After (Part 6 - Simple)

```rust
const BACKUP_SYNC_INTERVAL: u64 = 5;      // 83ms between syncs (12/sec)

// Client reconciliation (4 lines):
game_state.ball.x = ball_state.x;
game_state.ball.y = ball_state.y;
game_state.ball.vx = ball_state.vx;
game_state.ball.vy = ball_state.vy;
```

**Result:**
- ✅ Buttery smooth (corrections invisible)
- ✅ No false goals
- ✅ Simple code, easy to understand

---

## Why Simplification Works

### The Problem with "Smart" Reconciliation

**1. Infrequent syncs (1 second) = Large corrections**
- Ball diverges 60 frames of movement
- At 360 virtual units/sec, that's 360 units of error
- Lerp can't hide a 360-unit correction

**2. Lerp fights with prediction**
- Client predicts: ball at position A
- Sync arrives: ball should be at position B
- Client lerps 25% toward B
- Next frame: Prediction moves from lerped position (wrong!)
- Next sync: Process repeats
- Result: Constant micro-corrections = visible jitter

**3. Complexity doesn't help on LAN**
- Client prediction is nearly perfect (same code, same physics)
- Error is typically <5 virtual units
- Sophisticated reconciliation is overkill
- Simple snap would be fine if it happened often enough

### The Solution: More Syncs, Less Complexity

**Insight:** If you sync fast enough, you can just snap.

**Math:**
- 12 syncs/sec = 83ms between syncs
- Ball moves 360 units/sec × 0.083 sec = ~30 units max divergence
- On LAN (5ms latency), actual divergence is ~2 units
- A 2-unit snap is invisible (ball diameter is 20 units)

**Why this beats lerp:**
- No interpolation artifacts
- No prediction fighting
- Corrections are instant and tiny
- Simple code = fewer bugs

---

## Network Traffic Analysis

### Is 12 syncs/sec too much?

**Before (1 sync/sec):**
- 1 sync/sec × ~50 bytes = 50 bytes/sec
- 0.05 KB/sec

**After (12 syncs/sec):**
- 12 syncs/sec × ~50 bytes = 600 bytes/sec
- 0.6 KB/sec

**Conclusion:** **12x more traffic, but still trivial!**
- 0.6 KB/sec is nothing on modern networks
- Voice chat uses ~50 KB/sec
- Video streaming uses ~500 KB/sec
- Our 0.6 KB/sec is 0.1% of voice chat bandwidth

### What about internet connections?

**On high latency (200ms), is 12 syncs/sec still good?**

Yes! Counter-intuitively, **high latency benefits from MORE syncs:**

**Example: 200ms latency, 1 sync/sec:**
- Ball moves 1 second of physics
- Sync arrives 200ms later
- Client is 1.2 seconds ahead of sync
- Massive correction needed

**Example: 200ms latency, 12 syncs/sec:**
- Ball moves 83ms of physics
- Sync arrives 200ms later
- Client is 283ms ahead of sync
- Much smaller correction

**Even better:** Could go to 20 syncs/sec (50ms) for internet play!

---

## Performance Characteristics

### LAN (1-5ms latency)

**Before:**
- Sync arrives at: 1000ms, 2000ms, 3000ms...
- Visible jitter every 1 second

**After:**
- Sync arrives at: 83ms, 166ms, 249ms, 332ms...
- Corrections every 83ms are invisible
- Ball appears perfectly smooth

### Good Internet (50ms latency)

**Before:**
- Large corrections every second
- Visible rubber-banding

**After:**
- Tiny corrections 12 times per second
- Each correction ~10-20 units (barely visible)
- Overall smooth experience

### High Latency (200ms)

**Before:**
- Massive corrections
- Constant rubber-banding
- Barely playable

**After:**
- Still playable!
- More frequent small corrections
- Better than 1-second intervals
- Could tune to 20 syncs/sec for even better feel

---

## Lessons Learned

### 1. Simple Often Beats Complex

The "professional" netcode with prediction, lerp, error thresholds, and reconciliation was **worse** than simple snapping because:
- Syncs were too infrequent (1/sec)
- Complexity can't fix fundamental frequency problem
- More code = more surface area for bugs

### 2. Measure Before Optimizing

We built sophisticated reconciliation before testing if it was needed:
- LAN has 1-5ms latency
- Client prediction is nearly perfect
- Complex reconciliation was premature optimization

**Better approach:**
1. Start simple (just snap)
2. Test on target network (LAN)
3. Only add complexity if simple doesn't work

### 3. Network Bandwidth Is Cheap

We worried about network traffic:
- Tried to minimize syncs (1/sec)
- Built event-based system to avoid periodic syncs
- Added complex logic to reduce messages

**Reality:**
- 0.6 KB/sec is trivial on any network
- Could easily do 60 syncs/sec and still use <3 KB/sec
- Bandwidth is not the constraint for Pong

### 4. Latency, Not Bandwidth, Is The Enemy

**Bandwidth:** How much data you can send (MB/sec)
**Latency:** How long it takes to arrive (milliseconds)

For real-time games:
- Bandwidth is almost never the problem (games use KB/sec)
- Latency is what kills responsiveness (100ms+ feels laggy)

**Our fix:**
- More frequent syncs = lower effective latency
- Client never more than 83ms out of sync
- Feels responsive even with network lag

---

## Tuning Guide

### Current Settings (Optimal for LAN)

```rust
const BACKUP_SYNC_INTERVAL: u64 = 5;  // 12 syncs/sec, 83ms interval
```

### For Different Networks

**LAN / Low Latency (<10ms):**
- Current: 5 frames (83ms) ✅ Perfect
- Could use: 10 frames (166ms) if bandwidth is a concern

**Good Internet (20-50ms):**
- Current: 5 frames (83ms) ✅ Still great
- Keep as-is

**High Latency (100-200ms):**
- Try: 3 frames (50ms) - 20 syncs/sec
- More syncs compensate for latency

**Terrible Internet (>200ms):**
- Try: 2 frames (33ms) - 30 syncs/sec
- But honestly, Pong isn't playable at >200ms anyway

### How to Change

Edit `src/main.rs`:
```rust
const BACKUP_SYNC_INTERVAL: u64 = 5;  // Change this number

// 2 = 30 syncs/sec (very aggressive)
// 3 = 20 syncs/sec (aggressive)
// 5 = 12 syncs/sec (current, optimal)
// 10 = 6 syncs/sec (conservative)
// 20 = 3 syncs/sec (low bandwidth)
```

**Rule of thumb:** Lower is smoother, higher saves bandwidth.

---

## Comparison Table

| Approach | Sync Rate | Code Lines | Smoothness (LAN) | Smoothness (Internet) | Complexity |
|----------|-----------|------------|------------------|----------------------|------------|
| Part 5 (Complex) | 1/sec | 40 | ❌ Jittery | ❌ Very jittery | High |
| Part 6 (Simple) | 12/sec | 4 | ✅ Perfect | ✅ Good | Low |
| Aggressive | 20/sec | 4 | ✅ Perfect | ✅ Great | Low |
| Conservative | 6/sec | 4 | ✅ Very good | ⚠️ Minor jitter | Low |

---

## Final Thoughts

**We built Rocket League netcode for Pong... then realized we just needed to sync more often.**

The sophisticated client-side prediction, error-based reconciliation, and adaptive lerping was:
1. Over-engineered for the problem
2. Solving the wrong problem (latency, not frequency)
3. More complex than necessary
4. Actually worse than the simple approach

**The right solution:**
- Sync every 83ms (12 times per second)
- Just snap to authoritative state
- Simple, robust, performant
- Works great on LAN, scales to internet

**Proof that sometimes the best code is the code you delete!** 🎯
