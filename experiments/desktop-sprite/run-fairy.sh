#!/bin/bash
# Build & run the 3D flower fairy overlay. Quit: pkill -f fairy-sprite
set -e
cd "$(dirname "$0")"
swiftc -O -o fairy-sprite fairy.swift
exec ./fairy-sprite "$(pwd)/fairy.html"
