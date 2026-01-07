#!/bin/bash
# Test script to demonstrate virtual coordinate scaling

echo "=== P2Pong Virtual Coordinate System Test ==="
echo ""
echo "The game now uses a virtual 200×100 coordinate system."
echo "It will scale to ANY terminal size!"
echo ""
echo "Current terminal: $(tput cols)×$(tput lines)"
echo ""
echo "Try resizing your terminal while playing to see adaptive rendering!"
echo ""
echo "Suggested test sizes:"
echo "  - Small:  60×20  (quarter screen)"
echo "  - Medium: 100×30 (half screen)"
echo "  - Large:  160×40 (full screen)"
echo ""
echo "Press Enter to start the game..."
read

cargo run --release
