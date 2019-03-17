#!/bin/bash
set -e
set -o xtrace

SCRIPT_DIR=`dirname "$0"`

#######################################################
# Copy Pickletrack to server
ssh -x ec2-user@34.229.210.131 "mkdir -p ~/release && mkdir -p ~/static"
ssh -x ec2-user@34.229.210.131 "mkdir -p ~/bin && rm -rf ~/bin/*"
scp -r ${SCRIPT_DIR}/../Cargo.toml ec2-user@34.229.210.131:~/release
scp -r ${SCRIPT_DIR}/../Cargo.lock ec2-user@34.229.210.131:~/release
scp -r ${SCRIPT_DIR}/../src ec2-user@34.229.210.131:~/release
scp -r ${SCRIPT_DIR}/../static/* ec2-user@34.229.210.131:~/static
scp -r ${SCRIPT_DIR}/../deploy/aws/* ec2-user@34.229.210.131:~/bin

#######################################################
# Run script to build and release Pickletrack, then start it on the server.
ssh -x ec2-user@34.229.210.131 "~/bin/release.sh && ~/bin/restart.sh"
