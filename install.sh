#!/bin/sh
# shellcheck disable=SC2086  # $RETRY намеренно без кавычек: набор флагов curl
# Установщик Sherpa для Linux (x86_64).
#   curl -fsSL https://raw.githubusercontent.com/DrTiget/sherpa/main/install.sh | sh
#   ... | sh -s -- --deb | --uninstall | --version vX.Y.Z
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
Установка Sherpa (Linux x86_64).
  --deb            поставить .deb (нужен sudo) вместо AppImage
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
  Linux) ;;
  Darwin) die "macOS пока не поддерживается — готовые сборки есть только для Linux" ;;
  *) die "система $OS пока не поддерживается — готовые сборки есть только для Linux" ;;
esac

as_root() {
  if [ "$(id -u)" -eq 0 ]; then "$@"
  elif command -v sudo >/dev/null 2>&1; then sudo "$@"
  else die "нужны права root, а sudo не найден"; fi
}

refresh_desktop_db() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESK_DIR" >/dev/null 2>&1 || true
  fi
}

if [ "$MODE" = uninstall ]; then
  rm -f "$BIN_DIR/$APP" "$ICON_DIR/$APP.png" "$DESK_DIR/$APP.desktop"
  refresh_desktop_db
  if command -v dpkg >/dev/null 2>&1 && dpkg -s "$APP" >/dev/null 2>&1; then
    info "Удаляю пакет $APP"
    as_root dpkg -r "$APP"
  fi
  info "Sherpa удалена (данные приложения и пароли в хранилище ключей не затронуты)"
  exit 0
fi

need curl
case "$ARCH" in
  x86_64|amd64) ;;
  *) die "архитектура $ARCH пока не поддерживается: готовые сборки только для x86_64" ;;
esac

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t sherpa)
trap 'rm -rf "$TMP"' EXIT INT TERM

# Параметры повторов; --retry-all-errors есть не во всех версиях curl (>= 7.71)
RETRY="--retry 5 --retry-delay 2"
if curl --help all 2>/dev/null | grep -q -- '--retry-all-errors'; then RETRY="$RETRY --retry-all-errors"; fi

# fetch <url> <файл> — докачка (-C -), повторы; при сбое ещё раз по HTTP/1.1 (лечит обрывы HTTP/2 PROTOCOL_ERROR)
fetch() {
  curl -fL $RETRY -C - --progress-bar -o "$2" "$1" && return 0
  info "Загрузка прервалась — повторяю по HTTP/1.1 с докачкой"
  curl -fL --http1.1 $RETRY -C - --progress-bar -o "$2" "$1" && return 0
  rm -f "$2"
  return 1
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  else return 1; fi
}

if [ -n "$VERSION" ]; then API="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
else API="https://api.github.com/repos/$REPO/releases/latest"; fi
info "Запрашиваю релиз: $API"
curl -fsSL $RETRY -H 'Accept: application/vnd.github+json' "$API" -o "$TMP/release.json" \
  || die "не удалось получить информацию о релизе (нет релиза/сети или лимит GitHub API)"

TAG=$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$TMP/release.json" | head -n 1)
[ -n "$TAG" ] || die "в ответе GitHub нет tag_name"
sed -n 's/.*"browser_download_url": *"\([^"]*\)".*/\1/p' "$TMP/release.json" > "$TMP/urls"

# pick <regex> — первый URL ассета, подходящий под регулярное выражение
pick() { grep -E "$1" "$TMP/urls" | head -n 1 || true; }

# download <url> <итоговый файл>: во временный файл, затем проверка SHA256 из SHA256SUMS релиза
download() {
  name=${1##*/}
  info "Скачиваю $name"
  fetch "$1" "$TMP/$name" || die "не удалось скачать $1"
  SUMS_URL=$(pick '/SHA256SUMS$')
  if [ -n "$SUMS_URL" ] && sha256_of "$TMP/$name" >/dev/null 2>&1; then
    curl -fsSL $RETRY "$SUMS_URL" -o "$TMP/SHA256SUMS" || die "не удалось скачать SHA256SUMS"
    want=$(awk -v n="$name" '$2 == n || $2 == "*" n { print $1 }' "$TMP/SHA256SUMS" | head -n 1)
    [ -n "$want" ] || die "в SHA256SUMS нет записи для $name"
    got=$(sha256_of "$TMP/$name")
    if [ "$got" != "$want" ]; then
      rm -f "$TMP/$name"
      die "контрольная сумма $name не совпала (ожидалась $want, получена $got) — файл повреждён, повторите установку"
    fi
    info "SHA256 совпала"
  else
    info "Пропускаю проверку SHA256 (нет SHA256SUMS в релизе или нет sha256sum/shasum)"
  fi
  mv -f "$TMP/$name" "$2"
}

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

if curl -fsSL $RETRY "https://raw.githubusercontent.com/$REPO/$TAG/src-tauri/icons/128x128.png" -o "$TMP/icon.png"; then
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
refresh_desktop_db

info "Sherpa $TAG установлена: $BIN_DIR/$APP"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) info "Добавьте $BIN_DIR в PATH, чтобы запускать командой 'sherpa'" ;;
esac
if ! { ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; }; then
  info "Для запуска AppImage нужен libfuse2 (Debian/Ubuntu: sudo apt install libfuse2; Fedora: sudo dnf install fuse-libs) — или запускайте: $APP --appimage-extract-and-run"
fi
