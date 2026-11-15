#!/bin/bash
# Compile xapp (xapp/ at the top of the repository) and put it into the
# extension. Runs inside tools/mkext.sh's Arch container, as root:
#   $1 the extension's tree, $2 the Arch snapshot (YYYY/MM/DD).
# Rust comes from the same snapshot (the container's pacman.conf points to
# it); --locked: exactly the crates pinned in xapp/Cargo.lock.
set -euo pipefail
root=${1:?tree missing}
pacman --config /tmp/pacman.conf -Sy --noconfirm --needed rust >/dev/null
export CARGO_HOME=/work/cargo-home
cargo build --release --locked --quiet \
    --manifest-path /src/xapp/Cargo.toml --target-dir /work/xapp-target
install -Dm0755 /work/xapp-target/release/xapp "$root/usr/bin/xapp"
