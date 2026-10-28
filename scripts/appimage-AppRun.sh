#!/bin/sh
# Arranque del AppImage de Nebula Launcher.
#
# Si el sistema ya tiene WebKitGTK 4.1 y GTK 3, se usan esas librerías.
# Cargar la copia empaquetada (unos 300 MB sin comprimir) desde el AppImage
# añade casi un segundo al arranque; con las del sistema el splash aparece
# en una fracción de segundo. Si no están, se usa la copia incluida.
#
# Para forzar la copia empaquetada:
#   NEBULA_BUNDLED_WEBKIT=1 ./Nebula\ Launcher_*.AppImage
set -e

APPDIR="${APPDIR:-$(cd "$(dirname "$0")" && pwd)}"
export APPDIR
export PATH="$APPDIR/usr/bin:$APPDIR/usr/sbin:$APPDIR/usr/games:$APPDIR/bin:$APPDIR/sbin:$PATH"

find_lib() {
  for dir in /usr/lib /usr/lib64 /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu /lib64 /usr/local/lib; do
    if [ -e "$dir/$1" ]; then
      printf '%s\n' "$dir"
      return 0
    fi
  done
  return 1
}

system_dirs=""
if [ "${NEBULA_BUNDLED_WEBKIT:-0}" != "1" ]; then
  webkit_dir="$(find_lib libwebkit2gtk-4.1.so.0 || true)"
  gtk_dir="$(find_lib libgtk-3.so.0 || true)"
  if [ -n "$webkit_dir" ] && [ -n "$gtk_dir" ]; then
    system_dirs="$webkit_dir:$gtk_dir"
    for dir in /usr/lib /usr/lib64 /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu /lib64; do
      [ -d "$dir" ] && system_dirs="$system_dirs:$dir"
    done
  fi
fi

if [ -n "$system_dirs" ]; then
  export LD_LIBRARY_PATH="$system_dirs${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  cd "$APPDIR/usr"
  exec "$APPDIR/usr/bin/nebula-launcher" "$@"
fi

# Respaldo: todo desde el AppImage. WebKitGTK empaquetado busca sus procesos
# hijos con una ruta relativa a AppDir/usr, igual que hacía AppRun.wrapped.
. "$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh"
export LD_LIBRARY_PATH="$APPDIR/usr/lib/:$APPDIR/usr/lib/i386-linux-gnu/:$APPDIR/usr/lib/x86_64-linux-gnu/:$APPDIR/usr/lib32/:$APPDIR/usr/lib64/:$APPDIR/lib/:$APPDIR/lib/i386-linux-gnu/:$APPDIR/lib/x86_64-linux-gnu/:$APPDIR/lib32/:$APPDIR/lib64/${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cd "$APPDIR/usr"
exec "$APPDIR/usr/bin/nebula-launcher" "$@"
