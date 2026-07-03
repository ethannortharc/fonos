#!/bin/bash
# Build & run the desktop panda sprite demo. Quit: right-click the panda.
set -e
cd "$(dirname "$0")"
swiftc -O -o panda-sprite main.swift
exec ./panda-sprite "$(pwd)/panda.html"
