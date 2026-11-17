#!/usr/bin/env bash
# box/setup.sh SOLAR_SHA HARNESS_SHA LEAN_SHA: prepares a fresh box for run.sh from public sources
# only: the system packages, rustup's stable toolchain, elan with solidity-lean's Lean toolchain,
# Python 3.14 (generic.py's), solc 0.8.35 checked against soliditylang.org's sha256, the driver
# checkout (~/solar at SOLAR_SHA), the harness (~/harness at HARNESS_SHA), solidity-lean (~/sl at
# LEAN_SHA) and a package cache (~/pk: the same commit with its packages fetched by lake, for
# --packages-from). Every version is written to ~/t5-versions.txt. Any failure stops it.
set -euxo pipefail
solar_sha=$1 harness_sha=$2 lean_sha=$3
solc_url=https://binaries.soliditylang.org/linux-amd64/solc-linux-amd64-v0.8.35+commit.47b9dedd
solc_sha256=fa8ac9a32d301ad023a36ee5a29f8e291fe3200c60244e43c142539e82a617f4
export DEBIAN_FRONTEND=noninteractive
sudo -E apt-get update -q
sudo -E apt-get install -yq git build-essential time zstd curl ca-certificates
# The sandbox (tools/certify/sandbox.sh, lean_iso.sh) needs unprivileged user namespaces.
sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 || true
unshare -r -m -p -f --mount-proc true
mkdir -p ~/.local/bin ~/.cache/solc
curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
curl -fsSL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh | sh -s -- -y --default-toolchain none
curl -fsSL https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.elan/bin:$PATH"
uv python install 3.14
py=$(uv python find 3.14)
ln -sf "$py" ~/.local/bin/python3.14
ln -sf "$py" ~/.local/bin/python3
curl -fsSL "$solc_url" -o ~/.cache/solc/solc-0.8.35
echo "$solc_sha256  $HOME/.cache/solc/solc-0.8.35" | sha256sum -c
chmod +x ~/.cache/solc/solc-0.8.35
clone() { # clone URL DIR SHA
    git clone -q "$1" "$2"
    git -C "$2" fetch -q origin "$3"
    git -C "$2" checkout -q --detach "$3"
    [[ "$(git -C "$2" rev-parse HEAD)" == "$3" ]]
}
clone https://github.com/paradigmxyz/solar ~/solar "$solar_sha"
git clone -q --shared ~/solar ~/harness
git -C ~/harness fetch -q https://github.com/paradigmxyz/solar "$harness_sha"
git -C ~/harness checkout -q --detach "$harness_sha"
clone https://github.com/paradigmxyz/solidity-lean ~/sl "$lean_sha"
elan toolchain install "$(cat ~/sl/lean-toolchain)"
git clone -q ~/sl ~/pk
git -C ~/pk checkout -q --detach "$lean_sha"
(cd ~/pk && lake exe cache get)
{
    echo "solar $(git -C ~/solar rev-parse HEAD)"
    echo "harness $(git -C ~/harness rev-parse HEAD)"
    echo "solidity-lean $(git -C ~/sl rev-parse HEAD)"
    echo "package-cache $(git -C ~/pk rev-parse HEAD)"
    echo "solc $(sha256sum < ~/.cache/solc/solc-0.8.35 | cut -c1-64) $(~/.cache/solc/solc-0.8.35 --version | tail -1)"
    echo "python3 $(python3 --version)"
    echo "rustc $(rustc --version)"
    echo "cargo $(cargo --version)"
    echo "lean $(cd ~/sl && lean --version)"
    echo "lake $(cd ~/sl && lake --version)"
    echo "gcc $(gcc --version | head -1)"
    echo "time $(dpkg-query -W -f='${Version}' time)"
    echo "kernel $(uname -r)"
    echo "nproc $(nproc)"
    echo "mem_total_bytes $(awk '/MemTotal/ {print $2 * 1024}' /proc/meminfo)"
    echo "swap_total_bytes $(awk '/SwapTotal/ {print $2 * 1024}' /proc/meminfo)"
    df -B1 --output=size,avail "$HOME" | tail -1 | awk '{print "disk_bytes " $1 " avail " $2}'
} > ~/t5-versions.txt
cat ~/t5-versions.txt
