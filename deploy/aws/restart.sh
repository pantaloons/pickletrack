#!/bin/bash
set -e
set -o xtrace

sudo pkill "^pickletrack\$" || true
RUST_LOG=info nohup ~/bin/pickletrack &
