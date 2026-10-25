#!/bin/bash
# Genera `latest.json` para tauri-plugin-updater a partir de los artefactos
# firmados que deja `npm run tauri build`.
#
#   GITHUB_REPO="usuario/repo" ./create_latest_json.sh
#
# Variables opcionales:
#   GITHUB_REPO="usuario/repo"   Repositorio de los releases. Si no se indica,
#                                se intenta deducir del remoto `origin` de git.
#   NOTES="texto"                Notas que verá el usuario en el diálogo.
#   TAG_PREFIX="v"               Prefijo del tag (por defecto "v").
#
# Requiere que el build se haya hecho con TAURI_SIGNING_PRIVATE_KEY y
# `createUpdaterArtifacts: true` (ya configurado), para que existan los .sig.
set -euo pipefail

BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$BASE_DIR"

VERSION=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -1)
PUB_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
BUNDLE="$BASE_DIR/src-tauri/target/release/bundle"
NOTES="${NOTES:-Actualización de Nebula Launcher}"
TAG_PREFIX="${TAG_PREFIX:-v}"

if [ -z "${GITHUB_REPO:-}" ]; then
  origin=$(git config --get remote.origin.url 2>/dev/null || true)
  GITHUB_REPO=$(printf '%s' "$origin" \
    | sed -nE 's#.*github\.com[:/]([^/]+/[^/]+?)(\.git)?$#\1#p')
fi
if [ -z "$GITHUB_REPO" ]; then
  echo "ERROR: define GITHUB_REPO=\"usuario/repo\" (o configura el remoto origin de git)." >&2
  exit 1
fi

BASE_URL="https://github.com/$GITHUB_REPO/releases/download/${TAG_PREFIX}${VERSION}"

# Devuelve "nombre_archivo|firma" del primer artefacto que case con el patrón.
artifact() {
  local pattern="$1"
  local file
  file=$(find "$BUNDLE" -name "$pattern" -type f 2>/dev/null | head -1)
  if [ -z "$file" ]; then
    return 1
  fi
  local sig="${file}.sig"
  if [ ! -f "$sig" ]; then
    echo "ERROR: falta la firma $sig (¿compilaste sin TAURI_SIGNING_PRIVATE_KEY?)" >&2
    return 2
  fi
  printf '%s|%s' "$(basename "$file")" "$(tr -d '\n' < "$sig")"
}

PLATFORMS_FILE=$(mktemp)
trap 'rm -f "$PLATFORMS_FILE"' EXIT

add_platform() {
  local key="$1" data="$2"
  local name="${data%%|*}" sig="${data#*|}"
  printf '%s\t%s\t%s\n' "$key" "$name" "$sig" >> "$PLATFORMS_FILE"
}

if LINUX=$(artifact "*.AppImage.tar.gz"); then
  add_platform "linux-x86_64" "$LINUX"
elif LINUX=$(artifact "*.AppImage"); then
  # Tauri firma el AppImage tal cual cuando no genera el .tar.gz.
  add_platform "linux-x86_64" "$LINUX"
fi
# En Windows, Tauri 2 firma el propio instalador NSIS (*-setup.exe). También se
# aceptan los .zip de Tauri 1 / v1Compatible para no romper builds antiguos.
if WIN=$(artifact "*.nsis.zip"); then
  add_platform "windows-x86_64" "$WIN"
elif WIN=$(artifact "*.msi.zip"); then
  add_platform "windows-x86_64" "$WIN"
elif WIN=$(artifact "*-setup.exe"); then
  add_platform "windows-x86_64" "$WIN"
elif WIN=$(artifact "*.msi"); then
  add_platform "windows-x86_64" "$WIN"
fi

if [ ! -s "$PLATFORMS_FILE" ]; then
  echo "ERROR: no se encontraron artefactos firmados en $BUNDLE" >&2
  echo "Compila con: TAURI_SIGNING_PRIVATE_KEY=... npm run tauri build" >&2
  exit 1
fi

# Python construye el JSON: así las notas con saltos de línea, comillas o
# barras invertidas quedan escapadas correctamente.
VERSION="$VERSION" PUB_DATE="$PUB_DATE" NOTES="$NOTES" \
BASE_URL="$BASE_URL" PLATFORMS_FILE="$PLATFORMS_FILE" python3 - <<'PY'
import json
import os

platforms = {}
with open(os.environ["PLATFORMS_FILE"], encoding="utf-8") as f:
    for line in f:
        line = line.rstrip("\n")
        if not line:
            continue
        key, name, sig = line.split("\t")
        # GitHub sustituye los espacios del asset por puntos.
        url_name = name.replace(" ", ".")
        platforms[key] = {
            "signature": sig,
            "url": f'{os.environ["BASE_URL"]}/{url_name}',
        }

doc = {
    "version": os.environ["VERSION"],
    "notes": os.environ["NOTES"],
    "pub_date": os.environ["PUB_DATE"],
    "platforms": platforms,
}
with open("latest.json", "w", encoding="utf-8") as f:
    json.dump(doc, f, indent=2, ensure_ascii=False)
    f.write("\n")
PY

echo "✓ latest.json generado (versión $VERSION)"
echo "  repo: $GITHUB_REPO"
echo "  tag:  ${TAG_PREFIX}${VERSION}"
echo ""
cat "$BASE_DIR/latest.json"
