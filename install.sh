#!/bin/sh
# Установщик Sherpa для Linux и macOS.
#   curl -fsSL https://raw.githubusercontent.com/DrTiget/sherpa/main/install.sh | sh
#   ... | sh -s -- --deb | --uninstall | --version v0.1.0
set -eu

REPO="DrTiget/sherpa"
APP="sherpa"
BIN_DIR="${HOME}/.local/bin"
ICON_DIR="${HOME}/.local/share/icons/hicolor/128x128/apps"
DESK_DIR="${HOME}/.local/share/applications"

MODE=install
USE_DEB=0
VERSION=""

die() { printf 'Ошибка: %s\n' "$*" >&2; exit 1; }
info() { printf '==> %s\n' "$*"; }
need() { command -v "$1" >/dev/null 2>&1 || die "не найдена команда '$1' — установите её и повторите"; }

usage() {
  cat <<EOF
Установка Sherpa.
  --deb            Linux: поставить .deb (нужен sudo) вместо AppImage
  --uninstall      удалить Sherpa
  --version vX.Y.Z поставить конкретную версию (по умолчанию — последняя)
  -h, --help       эта справка
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --deb) USE_DEB=1 ;;
    --uninstall) MODE=uninstall ;;
    --version) [ $# -ge 2 ] || die "--version требует значение"; VERSION=$2; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "неизвестный параметр: $1" ;;
  esac
  shift
done

OS=$(uname -s)
ARCH=$(uname -m)
case "$OS" in
  Linux|Darwin) ;;
  *) die "система $OS не поддерживается этим скриптом (для Windows используйте install.ps1)" ;;
esac

as_root() {
  if [ "$(id -u)" -eq 0 ]; then "$@"
  elif command -v sudo >/dev/null 2>&1; then sudo "$@"
  else die "нужны права root, а sudo не найден"; fi
}

uninstall() {
  if [ "$OS" = Darwin ]; then
    found=0
    for d in /Applications "$HOME/Applications"; do
      if [ -d "$d/Sherpa.app" ]; then
        info "Удаляю $d/Sherpa.app"
        rm -rf "$d/Sherpa.app" 2>/dev/null || as_root rm -rf "$d/Sherpa.app"
        found=1
      fi
    done
    [ "$found" -eq 1 ] || info "Sherpa.app не найдено"
  else
    rm -f "$BIN_DIR/$APP" "$ICON_DIR/$APP.png" "$DESK_DIR/$APP.desktop"
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DESK_DIR" >/dev/null 2>&1 || true
    if command -v dpkg >/dev/null 2>&1 && dpkg -s "$APP" >/dev/null 2>&1; then
      info "Удаляю пакет $APP"
      as_root dpkg -r "$APP"
    fi
  fi
  info "Sherpa удалена (данные приложения и пароли в хранилище ключей не затронуты)"
}

if [ "$MODE" = uninstall ]; then uninstall; exit 0; fi

need curl
[ "$OS" = Darwin ] && need tar

if [ "$OS" = Linux ]; then
  case "$ARCH" in
    x86_64|amd64) ;;
    *) die "архитектура $ARCH пока не поддерживается: готовые сборки только для x86_64" ;;
  esac
fi

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t sherpa)
trap 'rm -rf "$TMP"' EXIT INT TERM

if [ -n "$VERSION" ]; then API="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
else API="https://api.github.com/repos/$REPO/releases/latest"; fi
info "Запрашиваю релиз: $API"
curl -fsSL -H 'Accept: application/vnd.github+json' "$API" -o "$TMP/release.json" \
  || die "не удалось получить информацию о релизе (нет релиза/сети или лимит GitHub API)"

TAG=$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$TMP/release.json" | head -n 1)
[ -n "$TAG" ] || die "в ответе GitHub нет tag_name"
sed -n 's/.*"browser_download_url": *"\([^"]*\)".*/\1/p' "$TMP/release.json" > "$TMP/urls"

# pick <regex> — первый URL ассета, подходящий под регулярное выражение
pick() { grep -E "$1" "$TMP/urls" | head -n 1 || true; }

download() { # url dest
  info "Скачиваю ${1##*/}"
  curl -fL --progress-bar "$1" -o "$2" || die "не удалось скачать $1"
}

if [ "$OS" = Darwin ]; then
  URL=$(pick '\.app\.tar\.gz$')
  [ -n "$URL" ] || die "в релизе $TAG нет .app.tar.gz для macOS"
  download "$URL" "$TMP/app.tar.gz"
  mkdir "$TMP/x"
  tar -xzf "$TMP/app.tar.gz" -C "$TMP/x" || die "не удалось распаковать архив"
  [ -d "$TMP/x/Sherpa.app" ] || die "в архиве нет Sherpa.app"
  DEST=/Applications
  if [ ! -w "$DEST" ]; then DEST="$HOME/Applications"; mkdir -p "$DEST"; fi
  rm -rf "$DEST/Sherpa.app"
  cp -R "$TMP/x/Sherpa.app" "$DEST/Sherpa.app"
  xattr -dr com.apple.quarantine "$DEST/Sherpa.app" 2>/dev/null || true
  info "Sherpa $TAG установлена: $DEST/Sherpa.app"
  info "Сборка не подписана; карантин снят, приложение можно открыть из Launchpad/Spotlight"
  exit 0
fi

# ---- Linux ----
if [ "$USE_DEB" -eq 1 ]; then
  command -v dpkg >/dev/null 2>&1 || die "--deb: dpkg не найден (это не Debian/Ubuntu); установите без --deb — будет AppImage"
  URL=$(pick '\.deb$')
  [ -n "$URL" ] || die "в релизе $TAG нет .deb"
  download "$URL" "$TMP/sherpa.deb"
  info "Устанавливаю .deb (нужен root)"
  if command -v apt-get >/dev/null 2>&1; then as_root apt-get install -y "$TMP/sherpa.deb"
  else as_root dpkg -i "$TMP/sherpa.deb"; fi
  info "Sherpa $TAG установлена пакетом (команда: sherpa)"
  exit 0
fi

URL=$(pick '\.AppImage$')
[ -n "$URL" ] || die "в релизе $TAG нет .AppImage"
mkdir -p "$BIN_DIR" "$ICON_DIR" "$DESK_DIR"
download "$URL" "$TMP/sherpa.AppImage"
chmod +x "$TMP/sherpa.AppImage"
mv -f "$TMP/sherpa.AppImage" "$BIN_DIR/$APP"

if curl -fsSL "https://raw.githubusercontent.com/$REPO/$TAG/src-tauri/icons/128x128.png" -o "$TMP/icon.png"; then
  mv -f "$TMP/icon.png" "$ICON_DIR/$APP.png"
else
  info "Иконку скачать не удалось — ярлык будет без неё"
fi

cat > "$DESK_DIR/$APP.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Sherpa
Comment=Графическая оболочка для серверов по SSH
Exec=$BIN_DIR/$APP
Icon=$APP
Terminal=false
Categories=Development;Network;
EOF
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DESK_DIR" >/dev/null 2>&1 || true

info "Sherpa $TAG установлена: $BIN_DIR/$APP"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) info "Добавьте $BIN_DIR в PATH, чтобы запускать командой 'sherpa'" ;;
esac
if ! { ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; }; then
  info "Для запуска AppImage нужен libfuse2 (Debian/Ubuntu: sudo apt install libfuse2; Fedora: sudo dnf install fuse-libs) — или запускайте: $APP --appimage-extract-and-run"
fi
