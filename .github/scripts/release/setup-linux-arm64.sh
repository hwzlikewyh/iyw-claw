#!/usr/bin/env bash

set -euo pipefail

mode="${1:?usage: setup-linux-arm64.sh <desktop|server>}"
cat > /tmp/iyw-claw-sources.list <<'EOF'
deb [arch=amd64] https://archive.ubuntu.com/ubuntu jammy main restricted universe multiverse
deb [arch=amd64] https://archive.ubuntu.com/ubuntu jammy-updates main restricted universe multiverse
deb [arch=amd64] https://archive.ubuntu.com/ubuntu jammy-backports main restricted universe multiverse
deb [arch=amd64] https://security.ubuntu.com/ubuntu jammy-security main restricted universe multiverse

deb [arch=arm64] https://ports.ubuntu.com/ubuntu-ports jammy main restricted universe multiverse
deb [arch=arm64] https://ports.ubuntu.com/ubuntu-ports jammy-updates main restricted universe multiverse
deb [arch=arm64] https://ports.ubuntu.com/ubuntu-ports jammy-backports main restricted universe multiverse
deb [arch=arm64] https://ports.ubuntu.com/ubuntu-ports jammy-security main restricted universe multiverse
EOF
sudo mv /etc/apt/sources.list /etc/apt/sources.list.default
sudo mv /tmp/iyw-claw-sources.list /etc/apt/sources.list
sudo tee /etc/apt/apt.conf.d/99iyw-claw-network >/dev/null <<'EOF'
Acquire::ForceIPv4 "true";
Acquire::Retries "5";
Acquire::http::Timeout "30";
Acquire::https::Timeout "30";
EOF
sudo dpkg --add-architecture arm64
sudo apt-get update

if [[ "$mode" == "desktop" ]]; then
  sudo apt-get install -y --only-upgrade \
    libgraphite2-3 libharfbuzz0b libfreetype6 libssl3
  sudo apt-get install -y \
    gcc-aarch64-linux-gnu \
    g++-aarch64-linux-gnu \
    libwebkit2gtk-4.1-dev:arm64 \
    libayatana-appindicator3-dev:arm64 \
    librsvg2-dev:arm64 \
    libssl-dev:arm64 \
    patchelf
elif [[ "$mode" == "server" ]]; then
  sudo apt-get install -y --only-upgrade libssl3
  sudo apt-get install -y \
    gcc-aarch64-linux-gnu \
    g++-aarch64-linux-gnu \
    libssl-dev:arm64
else
  echo "::error::Unknown Linux arm64 setup mode: $mode"
  exit 1
fi

cat >> "$GITHUB_ENV" <<'EOF'
PKG_CONFIG_ALLOW_CROSS=1
PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig
PKG_CONFIG_LIBDIR=/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig
PKG_CONFIG_SYSROOT_DIR=/usr/aarch64-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_AR=aarch64-linux-gnu-gcc-ar
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS=-Clinker=aarch64-linux-gnu-gcc
CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc-ar
EOF
