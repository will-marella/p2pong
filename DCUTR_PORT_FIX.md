# DCUTR Port Correction Fix

## Problem Diagnosed

DCUTR was failing even in ideal scenarios (public IP host + NAT client) due to a **port mismatch issue**.

### Root Cause

When the host (with public IP) connects to the relay server:

1. **Host listens on**: `0.0.0.0:4001` (the actual listening port)
2. **Relay observes**: `64.23.198.155:38456` (ephemeral source port from NAT'd outbound connection)
3. **DCUTR tries to connect to**: `64.23.198.155:38456` (wrong!)
4. **Result**: Connection refused - nothing listening on port 38456

### Why It Happened

The `observed_addr` from the identify protocol contains the **source port** that the relay server saw, NOT the listening port. DCUTR didn't know the correct port to dial.

## Solution Implemented

Added **port correction logic** that constructs the proper external address for DCUTR:

### Changes Made

1. **Track listen port** (`runtime.rs:51-65`)
   - Added `listen_port: Option<u16>` to `ConnectionState`
   - Store the port when host starts listening

2. **IP extraction helper** (`runtime.rs:28-40`)
   - Added `extract_ip_from_multiaddr()` function
   - Extracts just the IP address from a multiaddr

3. **External address construction** (`runtime.rs:524-555`)
   - When relay observes our IP, extract just the IP part
   - Combine with our known listening port
   - Add corrected address via `swarm.add_external_address()`
   - This triggers `NewExternalAddrCandidate` with the correct port for DCUTR

### How It Works

**Before (broken):**
```
Relay observes: /ip4/64.23.198.155/tcp/38456 (ephemeral port)
DCUTR tries:    /ip4/64.23.198.155/tcp/38456
Result:         Connection refused ❌
```

**After (fixed):**
```
Relay observes: /ip4/64.23.198.155/tcp/38456 (ephemeral port)
We extract IP:  64.23.198.155
We know port:   4001 (from listen_port)
We construct:   /ip4/64.23.198.155/tcp/4001
DCUTR tries:    /ip4/64.23.198.155/tcp/4001
Result:         Connection succeeds! ✅
```

## Testing

To test the fix:

### Host (VM with public IP):
```bash
./target/release/p2pong --listen
```

You should now see:
```
🔧 PORT CORRECTION for DCUTR:
   Observed address: /ip4/64.23.198.155/tcp/38456 (ephemeral port)
   Corrected address: /ip4/64.23.198.155/tcp/4001 (listen port)
   → Adding corrected address for DCUTR hole punching
```

### Client (NAT'd computer):
```bash
./target/release/p2pong --connect 12D3Koo...
```

Expected result: **DCUTR should succeed** and establish direct P2P connection!

## Debug Output

The fix adds clear diagnostic output:
- Shows the observed address (with wrong port)
- Shows the corrected address (with listen port)
- Confirms when corrected address is added

## What This Doesn't Fix

This fix addresses the **port mismatch** issue specifically. DCUTR may still fail in these cases:

1. **Symmetric NAT** - NAT changes source port for each destination
2. **Firewall blocking inbound** - Even on correct port, firewall may block
3. **Both peers behind NAT** - More complex scenarios (but should work with proper port info)

## Philosophy

We're **NOT adding relay fallback** by design. This is p2pong - it's about P2P! When DCUTR fails, we fail explicitly rather than silently degrading to relay mode. This makes diagnosis easier.

## Files Modified

- `src/network/runtime.rs`:
  - Added `extract_ip_from_multiaddr()` helper
  - Added `listen_port` field to `ConnectionState`
  - Implemented port correction logic in identify handler
  - Added diagnostic logging

## Credits

Fix developed through careful log analysis showing:
- DCUTR was initiating correctly
- Connection was failing at TCP level (connection refused)
- Port mismatch between observed and listening addresses
