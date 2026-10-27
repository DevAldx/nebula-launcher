#!/bin/bash
# Incrementa la versión de Nebula Launcher en todos los sitios donde vive.
#
# Formato: MAYOR.MENOR.PARCHE (SemVer)
#
#   patch  Cambios pequeños: arreglos, textos, iconos, ajustes visuales.
#   minor  Cambios relevantes: funciones nuevas compatibles.
#   major  Cambios grandes o incompatibles.
#   set    Fija una versión exacta, siempre mayor que la actual.
#
# Ejemplos:
#   ./scripts/bump-version.sh              # 0.1.11 -> 0.1.12
#   ./scripts/bump-version.sh patch        # 0.1.11 -> 0.1.12
#   ./scripts/bump-version.sh minor        # 0.1.11 -> 0.2.0
#   ./scripts/bump-version.sh major        # 0.1.11 -> 1.0.0
#   ./scripts/bump-version.sh set 2.1.0    # fija 2.1.0
#
# Actualiza: package.json, package-lock.json, src-tauri/tauri.conf.json,
# src-tauri/Cargo.toml y la versión del workspace (Cargo.toml raíz).
# Cargo.lock se actualiza solo al compilar.
set -euo pipefail

BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_JSON="$BASE_DIR/package.json"
PACKAGE_LOCK="$BASE_DIR/package-lock.json"
TAURI_CONF="$BASE_DIR/src-tauri/tauri.conf.json"
CARGO_TOML="$BASE_DIR/src-tauri/Cargo.toml"
ROOT_CARGO="$BASE_DIR/Cargo.toml"

usage() {
  cat <<'EOF'
Uso: ./scripts/bump-version.sh [patch|minor|major|set X.Y.Z]

  patch        Sube el parche. Ejemplo: 0.1.11 -> 0.1.12
  minor        Sube el menor y reinicia el parche. Ejemplo: 0.1.11 -> 0.2.0
  major        Sube el mayor y reinicia menor/parche. Ejemplo: 0.1.11 -> 1.0.0
  set X.Y.Z    Fija una versión exacta mayor que la actual. Ejemplo: 2.1.0
  X.Y.Z        Atajo de set. Ejemplo: 1.10.1

Sin argumentos equivale a patch.

Regla rápida:
  - Arreglo, texto, icono o detalle visual  -> patch
  - Función nueva o cambio relevante       -> minor
  - Rediseño o cambio incompatible         -> major
  - Varios cambios diminutos               -> un solo patch, no uno por commit
EOF
}

CURRENT=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' "$PACKAGE_JSON" | head -1)
if [[ ! "$CURRENT" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "ERROR: la versión actual no tiene el formato MAYOR.MENOR.PARCHE: '$CURRENT'" >&2
  exit 1
fi
MAJOR="${BASH_REMATCH[1]}"
MINOR="${BASH_REMATCH[2]}"
PATCH="${BASH_REMATCH[3]}"

NEW_VERSION=""
case "${1:-patch}" in
  patch)
    NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
    ;;
  minor)
    NEW_VERSION="$MAJOR.$((MINOR + 1)).0"
    ;;
  major)
    NEW_VERSION="$((MAJOR + 1)).0.0"
    ;;
  set)
    NEW_VERSION="${2:-}"
    if [[ ! "$NEW_VERSION" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
      echo "ERROR: 'set' necesita una versión MAYOR.MENOR.PARCHE." >&2
      usage >&2
      exit 1
    fi
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    if [[ "$1" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
      NEW_VERSION="$1"
    else
      echo "ERROR: opción desconocida: $1" >&2
      usage >&2
      exit 1
    fi
    ;;
esac

# Nunca reutilizar ni bajar una versión ya publicada.
if [[ "$NEW_VERSION" == "$CURRENT" ]] || \
   [[ "$(printf '%s\n%s\n' "$CURRENT" "$NEW_VERSION" | sort -V | tail -1)" != "$NEW_VERSION" ]]; then
  echo "ERROR: la nueva versión ($NEW_VERSION) debe ser mayor que la actual ($CURRENT)." >&2
  exit 1
fi

echo "Versión actual: $CURRENT"
echo "Nueva versión:  $NEW_VERSION"

# Escapamos los puntos para que sed busque la versión exacta y no cualquier
# carácter en su lugar.
CURRENT_RE="${CURRENT//./\\.}"

sed -i "s/\"version\": \"$CURRENT_RE\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON" "$TAURI_CONF"
sed -i "s/^version = \"$CURRENT_RE\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"

# package-lock.json lleva la versión en la raíz y en el paquete raíz.
if [ -f "$PACKAGE_LOCK" ]; then
  sed -i "0,/\"version\": \"$CURRENT_RE\"/s//\"version\": \"$NEW_VERSION\"/" "$PACKAGE_LOCK"
  sed -i "0,/\"version\": \"$CURRENT_RE\"/s//\"version\": \"$NEW_VERSION\"/" "$PACKAGE_LOCK"
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
echo
echo "Siguiente: compila, genera latest.json y publica el release v$NEW_VERSION."
echo "No reutilices una versión ya publicada; si algo falla, sube otro parche."
