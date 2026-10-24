#!/bin/bash
# Выполняется внутри sherpa-pkg-builder: /src — репозиторий, /out — dist, CARGO_TARGET_DIR/BUILD — кэши.
set -euo pipefail
export LD_LIBRARY_PATH="$QT_ROOT/lib:$QT_ROOT"
VERSION="${VERSION:-0.1.0}"
BUILD=/work/build; APPDIR=/work/AppDir
rm -rf "$APPDIR"; mkdir -p "$BUILD" "$APPDIR/usr/bin" /out

cargo build --release --locked --manifest-path /src/core/Cargo.toml -p sherpa-core
cmake -S /src/frontend -B "$BUILD" -G Ninja -DCMAKE_BUILD_TYPE=Release -DSHERPA_BUILD_TESTS=OFF \
    -DCMAKE_PREFIX_PATH="$QT_ROOT" -DCMAKE_INSTALL_PREFIX=/usr
cmake --build "$BUILD"
install -m755 "$BUILD/sherpa" "$APPDIR/usr/bin/sherpa"
install -m755 "$CARGO_TARGET_DIR/release/sherpa-core" "$APPDIR/usr/bin/sherpa-core"

# Рабочий стол + иконка
cp /src/packaging/sherpa.desktop /work/sherpa.desktop
cp /src/packaging/sherpa.svg /work/sherpa.svg

export QMAKE="$QT_ROOT/bin/qmake" VERSION
export EXTRA_PLATFORM_PLUGINS="libqoffscreen.so"
export QML_SOURCES_PATHS=/src/frontend/qml
# Драйверы Qt SQL, кроме sqlite, не нужны: linuxdeploy-plugin-qt тянул бы клиентские библиотеки
# MySQL/PostgreSQL/ODBC/Mimer, которых нет в образе. Убираем их из локальной копии Qt.
for f in "$QT_ROOT"/plugins/sqldrivers/*.so; do
    case "$f" in *sqlite*) ;; *) rm -f "$f" ;; esac
done
linuxdeploy --appdir "$APPDIR" --plugin qt \
    -e "$APPDIR/usr/bin/sherpa" -d /work/sherpa.desktop -i /work/sherpa.svg
# libOpenGL/libGLdispatch (glvnd) linuxdeploy не бандлит (в excludelist), а в чистой Ubuntu их может не быть
for l in libOpenGL.so.0 libGLdispatch.so.0; do
    cp -L "$(ldconfig -p | awk -v l="$l" '$1==l && /x86-64/ {print $NF; exit}')" "$APPDIR/usr/lib/"
done
# sherpa-core — не Qt-программа; рядом с sherpa (CoreClient ищет его в каталоге приложения)
test -x "$APPDIR/usr/bin/sherpa-core"

# ---- AppImage
export LINUXDEPLOY_OUTPUT_VERSION="$VERSION"
( cd /work && linuxdeploy --appdir "$APPDIR" --output appimage )
cp /work/Sherpa*-x86_64.AppImage "/out/Sherpa-${VERSION}-x86_64.AppImage"

# ---- .deb: тот же AppDir кладём в /opt/sherpa, Qt 6.8 бандлится (системный в 22.04/24.04 слишком старый)
D=/work/deb; rm -rf "$D"; mkdir -p "$D/opt/sherpa" "$D/usr/bin" "$D/usr/share/applications" \
    "$D/usr/share/icons/hicolor/scalable/apps" "$D/DEBIAN"
cp -a "$APPDIR/usr/." "$D/opt/sherpa/"
rm -rf "$D/opt/sherpa/share/applications" 
ln -s ../../opt/sherpa/bin/sherpa "$D/usr/bin/sherpa"
ln -s ../../opt/sherpa/bin/sherpa-core "$D/usr/bin/sherpa-core"
sed 's|^Exec=.*|Exec=sherpa|' /src/packaging/sherpa.desktop > "$D/usr/share/applications/sherpa.desktop"
cp /src/packaging/sherpa.svg "$D/usr/share/icons/hicolor/scalable/apps/sherpa.svg"
find "$D/opt" -type f \( -perm -u+x -o -name '*.so*' \) -exec strip --strip-unneeded {} + 2>/dev/null || true
SIZE=$(du -sk "$D" | cut -f1)
cat > "$D/DEBIAN/control" <<CTL
Package: sherpa
Version: $VERSION
Architecture: amd64
Maintainer: Sherpa <noreply@sherpa.invalid>
Installed-Size: $SIZE
Section: admin
Priority: optional
Depends: libc6 (>= 2.35), libgl1, libopengl0, libegl1, libfontconfig1, libfreetype6, libdbus-1-3, libxkbcommon0, libxkbcommon-x11-0, libxcb-cursor0, libxcb-icccm4, libxcb-image0, libxcb-keysyms1, libxcb-randr0, libxcb-render-util0, libxcb-shape0, libxcb-xkb1, libxcb-xinerama0, openssh-client
Description: Sherpa — менеджер серверов (SSH)
 Qt6/QML-приложение и sherpa-core. Qt 6.8 поставляется внутри пакета (/opt/sherpa).
CTL
fakeroot dpkg-deb -Zxz --build "$D" "/out/sherpa_${VERSION}_amd64.deb"
chown -R "${HOST_UID:-0}:${HOST_GID:-0}" /out
