#!/bin/bash
set -e

#######################################################
# Install Rust compiler
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
sudo yum groupinstall "Development Tools"
sudo yum install openssl-devel