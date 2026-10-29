# Nebula Launcher 0.3.1 — notas de la versión

Fecha: 2026-10-28

## Cambios

- **El splash aparece mucho antes.** La ventana principal ya no se crea al
  arrancar: se construye 120 ms después, cuando el splash ya está pintado. Así
  su webview no compite con el primer fotograma de la bienvenida.
- **El AppImage usa WebKitGTK/GTK del sistema cuando están disponibles.** Cargar
  la copia empaquetada de WebKit (unos 300 MB sin comprimir) desde el AppImage
  añadía casi un segundo al arranque. El nuevo `AppRun` detecta WebKitGTK 4.1 y
  GTK 3 en el sistema y los usa; si no están, sigue usando la copia incluida.
  Para forzar la copia empaquetada:
  `NEBULA_BUNDLED_WEBKIT=1 ./"Nebula Launcher_0.3.1_amd64.AppImage"`.
- **Limpieza de cachés y artefactos de compilación.** Se eliminaron los
  directorios `target/debug`, las dependencias release que ya no hacen falta y
  los AppDir temporales. Se conservan el código, la documentación y los
  artefactos firmados de la última versión.
- **Versión 0.3.1.** Cambio de parche: no hay funciones nuevas ni cambios
  incompatibles.

## Compatibilidad

- **Linux**: AppImage x86_64 firmado, con actualización automática. Requiere
  WebKitGTK 4.1 y GTK 3 para el arranque rápido; si no están, usa la copia
  empaquetada. En ambos casos necesita una sesión gráfica X11 o Wayland.
- **Windows**: instalador NSIS x64. Requiere Windows 10/11 y WebView2; Windows
  8/8.1 no es compatible con Tauri 2/WebView2 moderno. El `.exe` no tiene firma
  Authenticode, así que SmartScreen puede avisar; la firma del actualizador sí
  está incluida.
- **Wine/Bottles**: no es compatible. En Linux hay que usar el AppImage nativo.

## Artefactos

- AppImage: `Nebula Launcher_0.3.1_amd64.AppImage`, 114 489 848 bytes
  (SHA-256 `0a6ea403fdfac869ec1f1b1f0c2114d59acca8af0d0855d64237bdfaee77865f`).
- Instalador Windows: `Nebula Launcher_0.3.1_x64-setup.exe`, 2 393 781 bytes
  (SHA-256 `226b8d1615eb5c2cceacc00de4e243227709b390540840578c595e3998fae73c`).
- Ambos incluyen su `.sig` del actualizador y están enlazados desde
  `latest.json`.

## Verificación

- `cargo test -p nebula-core`: 100 tests correctos.
- `npm run check`: 0 errores y 0 warnings.
- `npm run build`: correcto.
- `cargo check` en `src-tauri`: correcto.
- AppImage 0.3.1 con XDG aislados en este equipo (CachyOS, X11):
  - primer arranque tras la compilación (frío): splash a los 1,18 s y
    contenido a los 1,35 s;
  - segundo arranque (caliente): splash y contenido a los 0,42 s;
  - binario release extraído con librerías del sistema: splash a los 0,15 s;
  - con `NEBULA_BUNDLED_WEBKIT=1` (WebKit empaquetada): splash a los 1,59 s.
  - La ventana principal aparece y la app sigue viva en todos los casos.
- Prueba Windows 0.3.1 en GitHub Actions (run `40721304886`): Windows Server
  2025 Datacenter 10.0.26100 con WebView2 155.0.4283.62. SHA-256 del instalador
  correcto, instalación silenciosa con código 0, `FileVersion` y
  `ProductVersion` 0.3.1, ventana principal encontrada y app viva. La captura
  muestra `V0.3.1` y el pie legal. El árbol UIA encontró 0 elementos con nombre
  y no encontró el botón de inicio de sesión.
- Actualización real 0.3.0 → 0.3.1 con XDG aislados: el diálogo mostró
  «0.3.0 → 0.3.1», se pulsó «Actualizar ahora», la app se cerró, el AppImage se
  reemplazó por el 0.3.1 (114 489 848 bytes, SHA-256 `0a6ea403...`), se
  reinició sola y la ventana nueva muestra `V0.3.1`. El registro no tuvo
  errores. La automatización se hizo con `GDK_BACKEND=x11`; la app probada era
  el AppImage 0.3.0 publicado.

## Limitaciones conocidas

- En modo dev (`cargo run` + Vite) el splash puede quedar negro por un error
  interno de WebKit al cargar dos webviews de Vite a la vez
  (`WebLoaderStrategy::internallyFailedLoadTimerFired`). El build con recursos
  embebidos (`tauri://localhost`), que es el que se publica, funciona.
- La comprobación de actualizaciones del splash y la del diálogo principal
  pueden duplicarse. El diálogo principal sigue siendo el respaldo.
- La prueba de Windows es en Windows Server 2025, no en un Windows 10/11 de
  escritorio. En 0.3.1 se verificaron instalación, arranque y ventana principal;
  el árbol UIA no encontró el botón de inicio de sesión, así que el inicio de
  sesión, la personalización y el flujo completo de juego no se revalidaron en
  Windows. La prueba de 0.2.0 sí cubrió inicio de sesión y personalización.
- La actualización real se probó en CachyOS con X11 y directorios XDG aislados.
  No se probó en Wayland puro ni en otras distribuciones; el actualizador de
  Tauri reemplaza el AppImage y lo reinicia, que es el flujo publicado.
