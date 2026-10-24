#!/bin/bash
# Скачивает Qt 6.8.3 (gcc_64) с download.qt.io на хосте и распаковывает в $1.
# aqtinstall внутри контейнера не используем: download.qt.io оттуда бывает недоступен.
set -euo pipefail
DEST="$1"; V=6.8.3; STAMP=202503201345
B=https://download.qt.io/online/qtsdkrepository/linux_x64/desktop/qt6_683/qt6_683
mkdir -p "$DEST" "$DEST/dl"
get() { curl -4 -fsSL --retry 3 -o "$DEST/dl/$2" "$B/$1/$V-0-$STAMP$2"; }
S=Linux-RHEL_8_10-GCC-Linux-RHEL_8_10-X86_64.7z
for a in qtbase qtdeclarative qtsvg qttools qttranslations; do get qt.qt6.683.linux_gcc_64 "$a-$S"; done
get qt.qt6.683.linux_gcc_64 icu-linux-Rhel8.6-x86_64.7z
get qt.qt6.683.addons.qtshadertools.linux_gcc_64 "qtshadertools-$S"
docker run --rm -v "$DEST:/qt" python:3.10-slim sh -c \
  'pip -q install py7zr >/dev/null 2>&1; mkdir -p /qt/6.8.3/gcc_64 && for f in /qt/dl/*.7z; do python -c "import py7zr,sys;py7zr.SevenZipFile(sys.argv[1]).extractall(\"/qt/6.8.3/gcc_64\")" $f; done; rm -rf /qt/dl; chown -R 1001:1001 /qt'
true
