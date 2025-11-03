#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [ ! -f disk.img ]; then
  echo "creating 16MB disk.img"
  qemu-img create -f raw disk.img 16M >/dev/null
fi
cargo bootimage
qemu-system-x86_64 -drive format=raw,file=target/x86_64-blog_os/debug/bootimage-blog_os.bin -device isa-debug-exit,iobase=0xf4,iosize=0x04 -serial stdio -drive file=disk.img,format=raw,if=ide,index=1,media=disk

