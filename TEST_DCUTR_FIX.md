# Testing the DCUTR Port Fix

## Quick Test Guide

### Scenario 1: VM Host (Public IP) + Local Client (NAT)

This is the scenario that was failing before.

**On VM (64.23.198.155):**
```bash
RUST_LOG=libp2p_dcutr=debug ./target/release/p2pong --listen
```

**Look for this NEW output:**
```
🔧 PORT CORRECTION for DCUTR:
   Observed address: /ip4/64.23.198.155/tcp/38456 (ephemeral port)
   Corrected address: /ip4/64.23.198.155/tcp/4001 (listen port)
   → Adding corrected address for DCUTR hole punching
```

Copy the Peer ID shown.

**On your local machine (behind NAT):**
```bash
RUST_LOG=libp2p_dcutr=debug ./target/release/p2pong --connect 12D3Koo...
```

**Expected result:**
```
🚀 DIRECT P2P CONNECTION ESTABLISHED!
✅ Connected! Starting game...
```

### Scenario 2: Both Behind NAT (Personal Computer to Personal Computer)

**Computer 1:**
```bash
RUST_LOG=libp2p_dcutr=debug ./target/release/p2pong --listen
```

**Computer 2:**
```bash
RUST_LOG=libp2p_dcutr=debug ./target/release/p2pong --connect 12D3Koo...
```

**Expected:**
- Both sides get proper external IPs from relay
- DCUTR initiates hole punch
- Should succeed unless both have symmetric NAT

### What Success Looks Like

**Host logs:**
```
✅ Connection established with 12D3Koo... (type: relay circuit)
⏳ Waiting for DCUTR hole punch (5 second timeout)...
🔍 DCUTR DEBUG: Dialing event
🚀 DIRECT P2P CONNECTION ESTABLISHED!
   Closing old relay connection
   Adding peer to Gossipsub mesh for game messages
✅ Connected! Starting game...
```

**Client logs:**
```
✅ Connection established with 12D3Koo... (type: relay circuit)
⏳ Waiting for DCUTR hole punch (5 second timeout)...
🔍 DCUTR DEBUG: Dialing event
🚀 DIRECT P2P CONNECTION ESTABLISHED!
   Closing old relay connection
   Adding peer to Gossipsub mesh for game messages
✅ Connected! Starting game...
```

### What Failure Looks Like

**If DCUTR still fails:**
```
⏰ DCUTR TIMEOUT after 5 seconds
   No direct connection established
   DCUTR event never fired - possible network issue
   ❌ DISCONNECTING - Direct connection required
```

**Common reasons:**
1. **Symmetric NAT** - Your router changes source port for each destination
2. **Firewall** - Inbound connections blocked even after hole punch
3. **No UPnP/NAT-PMP** - Router doesn't support port mapping protocols

### Debugging Tips

**Check which port DCUTR is trying:**

Look for this in the logs:
```
❌ Failed to connect to Some(PeerId("...")): 
   [(/ip4/64.23.198.155/tcp/4001/p2p/12D3Koo...: ...)]
```

The IP and port shown should match the **listen port** (4001), not an ephemeral port.

**Verify direct connectivity:**

Test if you can reach the host directly (bypassing DCUTR):
```bash
# Client side - try direct connection
./target/release/p2pong --connect /ip4/64.23.198.155/tcp/4001/p2p/12D3Koo...
```

If this works, DCUTR should work too (it's using the same address now).

### Measuring Success Rate

Try connecting 5 times and count successes:
```bash
# Run this on client 5 times
for i in {1..5}; do
    echo "Attempt $i"
    timeout 15 ./target/release/p2pong --connect 12D3Koo... 2>&1 | grep -E "(DIRECT|TIMEOUT|FAILED)"
    sleep 2
done
```

**Goal:** 80%+ success rate on repeated attempts.

## Expected Improvements

### Before Fix
- VM host + NAT client: **0% success** (always connection refused)
- Both behind NAT: **~30% success** (only when ports aligned by chance)

### After Fix
- VM host + NAT client: **~95% success** (should work unless firewall issues)
- Both behind NAT: **~70% success** (depends on NAT types)

## If It Still Fails

Check these:

1. **Port 4001 open on VM?**
   ```bash
   # On VM
   sudo netstat -tlnp | grep 4001
   ```

2. **Firewall blocking?**
   ```bash
   # On VM - allow inbound on port 4001
   sudo ufw allow 4001/tcp
   ```

3. **Client can actually listen?**
   ```bash
   # On client
   nc -l 12345  # If this fails, your NAT is too restrictive
   ```

4. **AutoNAT working?**
   Look for `🌐 AutoNAT: Reachable` in logs

## Success Criteria

✅ DCUTR completes within 5 seconds
✅ Game UI launches
✅ Paddles respond to input
✅ Ball syncs smoothly
✅ No "Connection refused" errors

## Next Steps After Success

Once DCUTR is working reliably:
1. Test on different network types (home, office, mobile hotspot)
2. Measure actual RTT during gameplay
3. Consider cleaning up debug logs for production
