#!/bin/bash
# Локальная сборка .deb и AppImage (Ubuntu 22.04/24.04, x86_64) в dist/.
# Использование: packaging/build-linux.sh   (нужен docker)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE="${SHERPA_PKG_CACHE:-$HOME/.cache/sherpa-pkg}"
mkdir -p "$ROOT/dist" "$CACHE/cargo-target" "$CACHE/build" "$CACHE/cargo-registry"
QT_VERSION=6.8.3
if [ ! -x "$CACHE/qt/$QT_VERSION/gcc_64/bin/qmake" ]; then
    mkdir -p "$CACHE/qt"
    "$ROOT/packaging/fetch-qt.sh" "$CACHE/qt"
fi
docker build -t sherpa-pkg-builder "$ROOT/packaging/docker"
docker run --rm \
    -v "$ROOT:/src:ro" -v "$ROOT/dist:/out" \
    -v "$CACHE/qt/$QT_VERSION/gcc_64:/opt/qt/current:ro" -v "$CACHE/cargo-target:/cargo-target" -v "$CACHE/build:/work" -v "$CACHE/cargo-registry:/root/.cargo/registry" \
    -e CARGO_TARGET_DIR=/cargo-target -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" -e VERSION="${VERSION:-0.1.0}" \
    sherpa-pkg-builder bash /src/packaging/inner-build.sh
ls -lh "$ROOT/dist"
