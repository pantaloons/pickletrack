#!/bin/bash
set -e
set -o xtrace

#######################################################
# Build Pickletrack web server
cd ~/release
source $HOME/.cargo/env
cargo build --release

#######################################################
# Set up static program layout
rm -f ~/bin/pickletrack
rm -f ~/static/data/current.json
ln -s ~/release/target/release/server ~/bin/pickletrack
ln -s ~/static/data/20170901.json ~/static/data/current.json 
