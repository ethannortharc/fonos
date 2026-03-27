#!/bin/bash
# Compile the Apple Speech STT helper tool.
# Output: ../resources/fonos-stt-apple
set -e
cd "$(dirname "$0")"
echo "Building fonos-stt-apple..."
swiftc -O -o ../resources/fonos-stt-apple apple_stt.swift \
    -framework Speech -framework Foundation
echo "Built: $(ls -lh ../resources/fonos-stt-apple | awk '{print $5}')"
