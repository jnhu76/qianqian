#!/usr/bin/env bash
# Copy the WSL-side logs into the evidence dir (durable) — no product tree writes.
set -u
EV=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d1-out-handle
cp "$HOME/d1-out-handle-build/build.log" "$EV/build-wsl.log"
cp "$HOME/d1-out-handle-build/win-build.log" "$EV/win-build-wsl.log"
echo "copied:"
ls -la "$EV"
