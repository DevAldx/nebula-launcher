#!/bin/bash
# Compila el AppImage de Linux y sustituye el AppRun generado por Tauri
# por uno que prefiere WebKitGTK/GTK del sistema cuando están disponibles.
#
# Uso:
#   export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/nebula-launcher.key)"
#   ./scripts/build-appimage.sh
#
# El AppImage final queda en:
#   src-tauri/target/release/bundle/appimage/
#
# Si quieres conservar el AppDir (320 MB) para inspeccionarlo:
#   KEEP_APPDIR=1 ./scripts/build-appimage.sh
set -euo pipefail

BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$BASE_DIR"

VERSION=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -1)
if [ -z "$VERSION" ]; then
  echo "ERROR: no se pudo leer la versión de src-tauri/tauri.conf.json" >&2
  exit 1
fi

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && [ -z "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]; then
  echo "ERROR: define TAURI_SIGNING_PRIVATE_KEY o TAURI_SIGNING_PRIVATE_KEY_PATH." >&2
  echo "La clave privada no se guarda en el repositorio." >&2
  exit 1
fi

PLUGIN=$(find "${XDG_CACHE_HOME:-$HOME/.cache}/tauri" -maxdepth 1 -type f \
  -name 'linuxdeploy-plugin-appimage*.AppImage' 2>/dev/null | head -1)
if [ -z "$PLUGIN" ]; then
  echo "ERROR: no se encontró linuxdeploy-plugin-appimage en la caché de Tauri." >&2
  echo "Ejecuta primero 'npm run tauri build' para que Tauri lo descargue." >&2
  exit 1
fi

echo "== Compilando Nebula Launcher $VERSION =="
npm run tauri build -- --bundles appimage

BUNDLE_DIR="$BASE_DIR/src-tauri/target/release/bundle/appimage"
APPDIR="$BUNDLE_DIR/Nebula Launcher.AppDir"
APPIMAGE=$(find "$BUNDLE_DIR" -maxdepth 1 -type f -name '*.AppImage' -name "*${VERSION}*" | head -1)

if [ ! -d "$APPDIR" ]; then
  echo "ERROR: no se encontró el AppDir en $APPDIR" >&2
  exit 1
fi
if [ -z "$APPIMAGE" ]; then
  echo "ERROR: no se encontró el AppImage de la versión $VERSION en $BUNDLE_DIR" >&2
  exit 1
fi

echo "== Sustituyendo AppRun =="
install -m 755 "$BASE_DIR/scripts/appimage-AppRun.sh" "$APPDIR/AppRun"

echo "== Repackando AppImage =="
TMP_DIR=$(mktemp -d "$BASE_DIR/src-tauri/target/appimage-repack.XXXXXX")
trap 'rm -rf "$TMP_DIR"' EXIT
(
  cd "$TMP_DIR"
  ARCH=x86_64 "$PLUGIN" --appdir="$APPDIR" >/dev/null
)
NEW_APPIMAGE=$(find "$TMP_DIR" -maxdepth 1 -type f -name '*.AppImage' | head -1)
if [ -z "$NEW_APPIMAGE" ]; then
  echo "ERROR: el repack no generó ningún AppImage." >&2
  exit 1
fi
mv -f "$NEW_APPIMAGE" "$APPIMAGE"
chmod 755 "$APPIMAGE"

echo "== Firmando AppImage =="
rm -f "$APPIMAGE.sig"
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  ./node_modules/.bin/tauri signer sign --app-version "$VERSION" "$APPIMAGE" >/dev/null
else
  ./node_modules/.bin/tauri signer sign --app-version "$VERSION" \
    --private-key-path "$TAURI_SIGNING_PRIVATE_KEY_PATH" "$APPIMAGE" >/dev/null
fi
if [ ! -f "$APPIMAGE.sig" ]; then
  echo "ERROR: no se generó la firma $APPIMAGE.sig" >&2
  exit 1
fi

if [ "${KEEP_APPDIR:-0}" != "1" ]; then
  rm -rf "$APPDIR"
fi

echo
echo "✓ AppImage listo: $APPIMAGE"
echo "  SHA-256: $(sha256sum "$APPIMAGE" | cut -d' ' -f1)"
echo "  Firma:   $APPIMAGE.sig"
