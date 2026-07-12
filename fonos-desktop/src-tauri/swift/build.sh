#!/bin/bash
# Compile the Apple helper tools.
# Outputs: ../resources/fonos-stt-apple
#          ../resources/fonos-audio-capture
#          ../resources/fonos-voice-capture
set -e
cd "$(dirname "$0")"

echo "Building fonos-stt-apple..."
swiftc -O -o ../resources/fonos-stt-apple apple_stt.swift \
    -framework Speech -framework Foundation
echo "Built: $(ls -lh ../resources/fonos-stt-apple | awk '{print $5}')"

echo "Building fonos-audio-capture..."
swiftc -O -o ../resources/fonos-audio-capture system_audio_capture.swift \
    -framework ScreenCaptureKit -framework AVFoundation \
    -framework CoreMedia -framework Foundation
echo "Built: $(ls -lh ../resources/fonos-audio-capture | awk '{print $5}')"

echo "Building fonos-voice-capture..."
swiftc -O -o ../resources/fonos-voice-capture voice_capture.swift \
    -framework AVFoundation -framework Foundation
echo "Built: $(ls -lh ../resources/fonos-voice-capture | awk '{print $5}')"

echo "Building fonos-diarize (SwiftPM + FluidAudio)..."
(cd fonos-diarize && swift build -c release)
cp fonos-diarize/.build/release/fonos-diarize ../resources/fonos-diarize
echo "Built: $(ls -lh ../resources/fonos-diarize | awk '{print $5}')"
