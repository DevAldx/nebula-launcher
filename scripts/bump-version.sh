#!/bin/bash
# Incrementa la versión de Nebula Launcher en todos los sitios donde vive.
#
#   ./scripts/bump-version.sh          # 0.1.5 -> 0.1.6 (patch)
#   ./scripts/bump-version.sh minor    # 0.1.3 -> 0.2.0
#   ./scripts/bump-version.sh major    # 0.1.3 -> 1.0.0
#
# Actualiza: package.json, src-tauri/tauri.conf.json,
# src-tauri/Cargo.toml y la versión del workspace (Cargo.toml raíz).
set -euo pipefail

BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_JSON="$BASE_DIR/package.json"
PACKAGE_LOCK="$BASE_DIR/package-lock.json"
TAURI_CONF="$BASE_DIR/src-tauri/tauri.conf.json"
CARGO_TOML="$BASE_DIR/src-tauri/Cargo.toml"
ROOT_CARGO="$BASE_DIR/Cargo.toml"

CURRENT=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' "$PACKAGE_JSON" | head -1)
MAJOR=${CURRENT%%.*}
REST=${CURRENT#*.}
MINOR=${REST%%.*}
PATCH=${REST##*.}

case "${1:-patch}" in
  major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
  minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
  patch) PATCH=$((PATCH + 1)) ;;
  *) echo "uso: $0 [patch|minor|major]" >&2; exit 1 ;;
esac

NEW_VERSION="$MAJOR.$MINOR.$PATCH"
echo "Versión actual: $CURRENT"
echo "Nueva versión:  $NEW_VERSION"

sed -i "s/\"version\": \"$CURRENT\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON" "$TAURI_CONF"
sed -i "s/^version = \"$CURRENT\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"

# package-lock.json lleva la versión en la raíz y en el paquete raíz.
if [ -f "$PACKAGE_LOCK" ]; then
  sed -i "0,/\"version\": \"$CURRENT\"/s//\"version\": \"$NEW_VERSION\"/" "$PACKAGE_LOCK"
  sed -i "0,/\"version\": \"$CURRENT\"/s//\"version\": \"$NEW_VERSION\"/" "$PACKAGE_LOCK"
fi

# La versión del workspace la hereda nebula-core (`version.workspace = true`).
# Se reescribe solo dentro de la sección [workspace.package].
if [ -f "$ROOT_CARGO" ]; then
  sed -i '/^\[workspace\.package\]/,/^\[/ s/^version = ".*"/version = "'"$NEW_VERSION"'"/' "$ROOT_CARGO"
fi

echo "✓ Actualizado a $NEW_VERSION"
echo "  - package.json"
echo "  - package-lock.json"
echo "  - src-tauri/tauri.conf.json"
echo "  - src-tauri/Cargo.toml"
[ -f "$ROOT_CARGO" ] && echo "  - Cargo.toml (workspace)"
