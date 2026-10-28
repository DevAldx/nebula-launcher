# Nebula Launcher 0.3.0 — notas de la versión

Fecha: 2026-10-27

## Cambios

- **Ventana de bienvenida animada.** Al abrir Nebula aparece una ventana
  `splash` de 620×380, sin bordes, centrada, siempre encima y oculta de la
  barra de tareas. Muestra el logo, anillos, partículas, una barra de progreso,
  los pasos del arranque, la versión y el copyright.
- **Arranque real antes de la ventana principal.** La ventana principal nace
  oculta. El splash ejecuta de verdad la carga de configuración, la
  comprobación de la versión más reciente (timeout de 4 s), la detección de
  Java (timeout de 2 s) y la lectura de instancias. La animación dura al menos
  2,6 s y hay un límite de 12 s para que la ventana principal se abra aunque
  alguna comprobación se atasque.
- **Progreso compartido entre Rust y Svelte.** El backend guarda el estado en
  `StartupProgress`, lo emite a la ventana `splash` y permite recuperarlo con
  `get_startup_progress`, de modo que la animación no se pierde si el frontend
  llega tarde al evento.
- **Página «Acerca de» con copyright.** Nueva sección accesible desde la barra
  lateral y desde Ajustes. Incluye el copyright de Nebula, la licencia MIT, las
  marcas de Mojang Studios y Microsoft, el aviso de no afiliación, los créditos
  y un enlace al repositorio.
- **Aviso legal visible en la barra lateral.** El pie muestra «© 2026 Nebula» y
  «Minecraft © Mojang · No afiliado», con un botón que lleva a «Acerca de». La
  tarjeta legal de Ajustes se amplió con el copyright, los derechos de Mojang y
  un acceso directo a la página completa.
- **Versión 0.3.0.** Cambio menor por la nueva ventana de bienvenida y la
  sección de copyright.

## Compatibilidad

- **Linux**: AppImage x86_64 firmado, con actualización automática.
- **Windows**: instalador NSIS x64. Requiere Windows 10/11 y WebView2; Windows
  8/8.1 no es compatible con Tauri 2/WebView2 moderno. El `.exe` no tiene firma
  Authenticode, así que SmartScreen puede avisar; la firma del actualizador sí
  está incluida.
- **Wine/Bottles**: no es compatible. En Linux hay que usar el AppImage nativo.

## Artefactos

- AppImage: `Nebula Launcher_0.3.0_amd64.AppImage`, 114 485 752 bytes
  (SHA-256 `41384b353068e33dc01f3c3a8bed429007ae96349dd59a54eb4921337bc624ec`).
- Instalador Windows: `Nebula Launcher_0.3.0_x64-setup.exe`, 2 525 378 bytes
  (SHA-256 `39001ee0b66f5a6f0e4813efa9d0b7cf2b15bb86d93a2f7481804d81e59b32a3`).
- Ambos incluyen su `.sig` del actualizador y están enlazados desde
  `latest.json`.

## Verificación

- `cargo test -p nebula-core`: 100 tests correctos.
- `npm run check`: 0 errores y 0 warnings.
- `npm run build`: correcto.
- `cargo check` en `src-tauri`: correcto.
- Arranque final con recursos embebidos y XDG aislados: splash visible desde
  ~0,5 s, ventana principal a los ~4,6 s, sin depender del servidor de Vite.
- Página «Acerca de» y pie de la barra lateral revisados visualmente con X11.
- AppImage 0.3.0 probado con XDG aislados: splash visible a los 1,5 s,
  ventana principal con `V0.3.0`, «Acerca de» y pie lateral correctos.
- Instalador Windows 0.3.0 compilado con cargo-xwin + NSIS 3.11 y firmado.
  La prueba de instalación y arranque se ejecuta en GitHub Actions
  (`Windows installer test`) después de publicar la release.

## Limitaciones conocidas

- En modo dev (`cargo build` + Vite) el splash puede quedar negro por un error
  interno de WebKit al cargar dos webviews de Vite a la vez
  (`WebLoaderStrategy::internallyFailedLoadTimerFired`). El build con recursos
  embebidos (`tauri://localhost`), que es el que se publica, funciona.
- La comprobación de actualizaciones del splash y la del diálogo principal
  pueden duplicarse. El diálogo principal sigue siendo el respaldo.
- La prueba de Windows es en Windows Server 2025, no en un Windows 10/11 de
  escritorio. No se probaron en Windows el flujo completo de juego ni la
  creación de una instancia.
