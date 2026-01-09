#!/bin/bash
# Quick test script for DCUTR fix verification

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "P2Pong DCUTR Fix - Quick Test Script"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Binary: ./target/release/p2pong"
echo "Build date: $(stat -f "%Sm" -t "%Y-%m-%d %H:%M" ./target/release/p2pong 2>/dev/null || echo 'Unknown')"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Choose test mode:"
echo ""
echo "  1) Host mode (listen for connections)"
echo "  2) Client mode (connect to host)"
echo "  3) Show logs from last run"
echo "  4) Exit"
echo ""
read -p "Enter choice [1-4]: " choice

case $choice in
    1)
        echo ""
        echo "Starting in HOST mode..."
        echo ""
        echo "Look for this output:"
        echo "  🔧 PORT CORRECTION for DCUTR:"
        echo "     Observed address: /ip4/X.X.X.X/tcp/XXXXX (ephemeral port)"
        echo "     Corrected address: /ip4/X.X.X.X/tcp/4001 (listen port)"
        echo ""
        echo "Share the Peer ID with your opponent!"
        echo ""
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo ""
        RUST_LOG=libp2p_dcutr=debug ./target/release/p2pong --listen 2>&1 | tee /tmp/p2pong_host.log
        ;;
    2)
        echo ""
        read -p "Enter Peer ID to connect to: " peer_id
        echo ""
        echo "Starting in CLIENT mode..."
        echo ""
        echo "Look for this output:"
        echo "  🚀 DIRECT P2P CONNECTION ESTABLISHED!"
        echo "  ✅ Connected! Starting game..."
        echo ""
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo ""
        RUST_LOG=libp2p_dcutr=debug ./target/release/p2pong --connect "$peer_id" 2>&1 | tee /tmp/p2pong_client.log
        ;;
    3)
        echo ""
        echo "=== Host logs (if exists) ==="
        if [ -f /tmp/p2pong_host.log ]; then
            cat /tmp/p2pong_host.log | grep -E "(PORT CORRECTION|DCUTR|DIRECT|Connection|External)" | tail -20
        else
            echo "No host log found"
        fi
        echo ""
        echo "=== Client logs (if exists) ==="
        if [ -f /tmp/p2pong_client.log ]; then
            cat /tmp/p2pong_client.log | grep -E "(DCUTR|DIRECT|Connection|refused)" | tail -20
        else
            echo "No client log found"
        fi
        ;;
    4)
        echo "Exiting..."
        exit 0
        ;;
    *)
        echo "Invalid choice"
        exit 1
        ;;
esac
