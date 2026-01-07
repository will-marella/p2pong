#!/bin/bash
# Simple test to verify paddle can reach boundaries
# Run the game and move paddles to top/bottom
echo "Testing P2Pong Physics..."
echo ""
echo "Terminal size: $(tput cols)x$(tput lines)"
echo "Playable height: $(($(tput lines) - 3))"
echo "Paddle height should be: $(( ($(tput lines) - 3) / 5 ))"
echo ""
echo "When you run the game:"
echo "1. Press W repeatedly - left paddle should reach y=3 (top of playable area)"
echo "2. Press S repeatedly - left paddle should reach y=$(( $(tput lines) - ($(tput lines) - 3) / 5 )) (bottom)"
echo "3. Same for right paddle with Up/Down arrows"
echo ""
echo "If paddle gets stuck BEFORE reaching these positions, there's a bug."
echo ""
echo "Press Enter to launch the game (Q to quit game when done)..."
read
cargo run --release
