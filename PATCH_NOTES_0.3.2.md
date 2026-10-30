# Nebula Launcher 0.3.2 — notas de la versión

Fecha: 2026-10-29

## Cambios

Este parche corrige fallos de seguridad y de funcionamiento detectados en la
revisión de la versión 0.3.1. No añade funciones nuevas ni cambia el flujo
normal de uso.

### Instalador

- **La firma del AppImage es obligatoria.** El instalador exige el archivo
  `.sig`, verifica la firma minisign antes de copiar nada y vuelve a verificar
  la copia final instalada. Acepta tanto el `.sig` base64 que genera Tauri como
  el texto minisign directo.
- **Selección conjunta de AppImage y `.sig`.** El diálogo pide ambos archivos;
  si falta la firma o no coincide, no se instala.
- **Instalación atómica y sin symlinks.** El temporal se crea en un directorio
  privado, los nombres y rutas se validan y las escrituras no siguen enlaces
  simbólicos. `open_installed` también rechaza rutas que no sean el AppImage
  esperado.

### Núcleo

- **Escrituras atómicas más seguras.** `write_atomic` usa temporales aleatorios
  en el mismo directorio y no sigue symlinks; `join_safe` rechaza componentes
  que sean enlaces simbólicos.
- **Descargas y mods.** Los archivos `.part` se crean de forma segura y el
  reemplazo final funciona también en Windows. La importación de mods usa
  `import_one` y valida cada entrada antes de moverla.
- **Manifiestos compartidos.** Se exige SHA-1 completo y tamaño de cada
  archivo, con topes de tamaño y número de entradas. Los manifiestos antiguos
  sin hash se rechazan en lugar de aceptarse a ciegas.
- **Cachés verificadas.** Los manifiestos y datos de versión de Minecraft se
  guardan con un `.sha1` lateral y se vuelven a descargar si no coinciden.
- **Sesión y autenticación.** `AuthExpired` distingue credenciales inválidas de
  errores transitorios; la sesión conserva las credenciales ante fallos de red
  y solo las borra cuando el servidor confirma que caducaron. Se valida el
  nombre de usuario de Minecraft antes de usarlo.
- **TOCTOU en la importación por código.** La vista previa entrega el
  manifiesto completo a `import_from_code`, que lo importa sin volver a leer el
  archivo original.
- **Splash y apertura de enlaces.** Se corrigieron la secuencia de arranque del
  splash y la validación de `open_url`.

### Documentación

- El README indica que el instalador necesita el AppImage **y** su
  `.AppImage.sig`; sin la firma no copia nada.

## Compatibilidad

- **Linux**: AppImage x86_64 firmado, con actualización automática. Requiere
  WebKitGTK 4.1 y GTK 3 para el arranque rápido; si no están, usa la copia
  empaquetada. Necesita una sesión gráfica X11 o Wayland.
- **Windows**: instalador NSIS x64. Requiere Windows 10/11 y WebView2. El `.exe`
  no tiene firma Authenticode, así que SmartScreen puede avisar; la firma del
  actualizador sí está incluida.
- **Wine/Bottles**: no es compatible. En Linux hay que usar el AppImage nativo.

## Verificación

- `cargo test -p nebula-core`: 106 tests correctos.
- `cargo check --all-targets` en `src-tauri`: correcto.
- `cargo test` en `installer/src-tauri`: 7 tests correctos.
- `svelte-check` en la raíz y en el instalador: 0 errores.
- Firma real del AppImage 0.3.1 verificada por el instalador, tanto con el
  `.sig` base64 de Tauri como con el texto minisign.
- `git diff --check`: limpio.

## Limitaciones conocidas

- El token de Minecraft todavía se pasa en `argv` al proceso del juego; es una
  limitación heredada y se corregirá en una versión posterior.
- `extract_zip` conserva una carrera teórica entre la comprobación y la
  escritura de una entrada; el resto de rutas ya se validan.
- No se ejecutó `cargo-audit` en esta revisión.
- `npm audit` reporta vulnerabilidades bajas y moderadas en dependencias de
  desarrollo; no afectan al binario publicado.
- Las cachés antiguas sin `.sha1` se vuelven a descargar una vez.

## Publicación

La versión es **0.3.2** (parche). Los artefactos firmados y `latest.json` se
generan al compilar esta versión; no se reutiliza el tag ni los artefactos de
0.3.1.
