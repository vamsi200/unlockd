#!/usr/bin/env bash

cargo build --release
sudo cp target/release/libunlockd.so /usr/lib/security/pam_unlockd.so
