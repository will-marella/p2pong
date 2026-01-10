# QUIC-Only Relay Connection Implementation

**Date:** January 9, 2026  
**Status:** ✅ Complete - Ready for Testing

## Summary

Successfully converted p2pong to use **QUIC-only** for relay server connections, eliminating TCP ephemeral port issues and enabling proper QUIC address discovery for DCUTR hole punching.

## What Changed

### 1. Relay Address Changed from TCP to QUIC
**File:** `src/network/runtime.rs` (line ~26)

**Before:**
```rust
const RELAY_ADDRESS: &str =
    "/ip4/143.198.15.158/tcp/4001/p2p/12D3Koo...";
```

**After:**
```rust
const RELAY_ADDRESS: &str =
    "/ip4/143.198.15.158/udp/4001/quic-v1/p2p/12D3Koo...";
```

### 2. Relay Circuit Address Updated
**File:** `src/network/runtime.rs` (line ~267)

**Before:**
```rust
format!("/ip4/143.198.15.158/tcp/4001/p2p/{}/p2p-circuit", peer)
```

**After:**
```rust
format!("/ip4/143.198.15.158/udp/4001/quic-v1/p2p/{}/p2p-circuit", peer)
```

### 3. Console Messages Updated
**File:** `src/network/runtime.rs` (line ~130)

**Before:**
```rust
println!("🔗 Connecting to NYC relay server (143.198.15.158:4001)...");
```

**After:**
```rust
println!("🔗 Connecting to relay server via QUIC (143.198.15.158:4001/udp)...");
```

### 4. Removed TCP Port Correction Logic
**Deleted ~30 lines** of code that tried to fix TCP ephemeral port issues:
- Removed `extract_ip_from_multiaddr()` helper function
- Removed `listen_port` field from `ConnectionState`
- Removed port correction logic in Identify event handler

**Why?** QUIC doesn't have TCP's ephemeral port problems. The observed UDP port is stable and correct.

## Why This Works

### The TCP Ephemeral Port Problem (Now Fixed!)
**Old behavior with TCP:**
1. Host listens on TCP port 4001
2. Host connects to relay (NAT assigns ephemeral source port 43102)
3. Relay observes: `64.23.198.155:43102`
4. DCUTR tries to connect to: `64.23.198.155:43102` ❌ (nothing listening)
5. Connection refused

**New behavior with QUIC:**
1. Host listens on UDP port 4001
2. Host connects to relay (UDP port stays 4001)
3. Relay observes: `64.23.198.155:4001/quic-v1`
4. DCUTR tries to connect to: `64.23.198.155:4001/quic-v1` ✅ (correct!)
5. Hole punch succeeds

### Relay Server Already Supports QUIC
Verified on `143.198.15.158`:
```bash
# Server is already listening on both transports:
tcp   LISTEN 0      1024         0.0.0.0:4001
udp   UNCONN 0      0            0.0.0.0:4001

# Firewall allows both:
4001/tcp                   ALLOW       Anywhere
4001/udp                   ALLOW       Anywhere
```

**No relay server changes needed!** The rust-libp2p relay-server-example already had QUIC enabled.

## Expected Behavior

### Successful Connection Flow:
```
🔗 Connecting to relay server via QUIC (143.198.15.158:4001/udp)...
✅ Connection established with 12D3Koo... via direct QUIC/UDP
🎉 Connected to relay server
🔍 Observed external address (QUIC) from relay: /ip4/64.23.198.155/udp/4001/quic-v1
🌐 External address candidate (QUIC) for DCUTR: /ip4/64.23.198.155/udp/4001/quic-v1
✨ Relay reservation ready
🚀 Connecting to peer via relay...
✅ Connection established with peer via relay circuit
📞 Dialing peer: 12D3Koo...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🎯 DCUTR SUCCESS! Direct P2P connection established
   Peer: 12D3Koo...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ Connection established with peer via direct QUIC/UDP
```

### What to Look For:
✅ "via QUIC" in relay connection message  
✅ "External address candidate (QUIC)"  
✅ No TCP addresses or ephemeral ports (43102, etc.)  
✅ DCUTR success with "direct QUIC/UDP"  

### What Would Indicate Failure:
❌ "Failed to dial relay via QUIC"  
❌ Still seeing TCP addresses  
❌ DCUTR timeout (no event fires)  
❌ Connection refused errors  

## Testing Instructions

### 1. Test on Host (VM with public IP)
```bash
ssh root@143.198.15.158
cd p2pong
./target/release/p2pong --listen --port 4001
```

**Expected output:**
- Should see QUIC relay connection
- Should see QUIC external address candidate
- No TCP ephemeral ports (43102, etc.)

### 2. Test on Client (behind NAT)
```bash
./target/release/p2pong --connect 12D3KooW...
```

**Expected output:**
- QUIC relay connection
- QUIC external address discovered
- DCUTR SUCCESS message
- Connection type: "direct QUIC/UDP"

### 3. With Debug Logging (if needed)
```bash
RUST_LOG=libp2p_dcutr=debug,libp2p_swarm=debug ./target/release/p2pong --listen
```

## Code Cleanup Achieved

### Lines Removed: ~45 lines
- ❌ `extract_ip_from_multiaddr()` function (14 lines)
- ❌ `listen_port` field from `ConnectionState` (1 line)
- ❌ Port correction logic in Identify handler (~20 lines)
- ❌ Various related assignments and checks (~10 lines)

### Lines Changed: ~4 lines
- ✅ RELAY_ADDRESS constant (TCP → QUIC)
- ✅ Relay circuit address format (TCP → QUIC)
- ✅ Console message updated

### Result: Simpler, Cleaner Code
- No hacky port correction workarounds
- No special-case logic for TCP ephemeral ports
- Straightforward QUIC-only path
- Easier to understand and maintain

## Potential Issues & Solutions

### Issue 1: ISP Blocks UDP
**Symptom:** "Failed to dial relay via QUIC"  
**Solution:** User needs to check firewall or try different network  
**Note:** If UDP doesn't work, P2P won't work anyway (DCUTR needs it)

### Issue 2: QUIC Connection Slower Than TCP
**Symptom:** Longer connection time  
**Solution:** This is expected on first connection (TLS handshake)  
**Note:** Subsequent connections use 0-RTT and are faster

### Issue 3: Relay Circuit Fails Over QUIC
**Symptom:** "Relay reservation failed"  
**Likelihood:** Very low (relay protocol is transport-agnostic)  
**Solution:** Check relay server logs, verify QUIC listener is active

## Rollback Plan

If QUIC-only doesn't work:

1. **Quick rollback** (1 line change):
   ```rust
   const RELAY_ADDRESS: &str =
       "/ip4/143.198.15.158/tcp/4001/p2p/12D3Koo...";
   ```

2. **Dual transport approach** (dial both TCP and QUIC):
   ```rust
   swarm.dial(tcp_relay_addr)?;
   swarm.dial(quic_relay_addr)?;
   ```

3. **No data loss** - All changes are in connection logic only

## Success Criteria

### Must Have (for success):
- [x] Build succeeds
- [ ] Relay connection via QUIC works
- [ ] QUIC external address discovered (not TCP)
- [ ] DCUTR receives QUIC candidates
- [ ] DCUTR hole punch succeeds
- [ ] Direct P2P connection established

### Nice to Have:
- [ ] Lower latency than TCP path
- [ ] No connection refused errors
- [ ] Clean logs (no TCP addresses)
- [ ] Works across different NAT types

## Performance Expectations

### QUIC Advantages:
- ✅ **Better NAT traversal** - UDP simultaneous open
- ✅ **No head-of-line blocking** - Multiple streams independent
- ✅ **Connection migration** - Survives NAT rebinding
- ✅ **0-RTT after first connection** - Faster reconnects
- ✅ **Built-in multiplexing** - No TCP port exhaustion

### Potential Downsides:
- ⚠️ **UDP blocking** - Some networks/ISPs block/throttle UDP
- ⚠️ **First connection slower** - TLS 1.3 handshake overhead
- ⚠️ **More CPU usage** - Encryption/decryption in userspace

## Next Steps After Testing

### If Successful:
1. Document QUIC requirement for users
2. Add UDP port to firewall instructions
3. Consider removing TCP listeners entirely (QUIC-only)
4. Update README with QUIC information

### If Partially Successful:
1. Keep QUIC for relay, add TCP fallback
2. Prefer QUIC but allow TCP as backup
3. Add transport preference configuration

### If Failed:
1. Investigate specific failure mode
2. Check relay server QUIC implementation
3. Consider WebRTC as alternative
4. Document NAT traversal limitations

## Related Files

- `src/network/runtime.rs` - Main changes here
- `Cargo.toml` - QUIC feature already enabled
- Relay server: `143.198.15.158:/root/rust-libp2p/examples/relay-server/`

## References

- libp2p QUIC spec: https://github.com/libp2p/specs/tree/master/quic
- DCUTR spec: https://github.com/libp2p/specs/blob/master/relay/DCUtR.md
- QUIC hole punching: https://github.com/libp2p/rust-libp2p/pull/3964

---

**Ready to test!** 🚀

Run the host and client and check for:
1. QUIC relay connection (not TCP)
2. QUIC external addresses (not ephemeral TCP ports)
3. DCUTR SUCCESS with direct QUIC/UDP connection

If you see "🎯 DCUTR SUCCESS!" with "direct QUIC/UDP", we've solved the NAT traversal problem! 🎉
