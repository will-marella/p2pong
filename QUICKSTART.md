# P2Pong WebRTC - Quick Start

## Build

```bash
cargo build --release
cargo build --release --bin signaling-server
```

## Deploy Signaling Server (VM)

```bash
# Copy to VM
scp target/release/signaling-server root@143.198.15.158:~/

# SSH and run
ssh root@143.198.15.158
nohup ./signaling-server > signaling.log 2>&1 &
```

## Play

**Host (on VM):**
```bash
./p2pong --listen
# Copy the peer ID shown
```

**Client (local):**
```bash
./target/release/p2pong --connect peer-<id-from-host>
```

## Test Locally

```bash
# Terminal 1
./target/release/p2pong --listen

# Terminal 2 (use peer ID from terminal 1)
./target/release/p2pong --connect peer-abc123
```

## Debug

```bash
RUST_LOG=info ./p2pong --listen
```

## More Info

See `WEBRTC_MIGRATION.md` for full deployment guide and troubleshooting.
