# Nebula Launcher 0.1.8 — notas de la versión

Fecha: 2026-10-25

## Correcciones

- **La cara de la skin en Inicio.** Antes se cargaba la textura completa de 64×64 y,
  además, Mojang la devuelve por `http://`, que la CSP bloqueaba. Ahora la URL se
  normaliza a HTTPS y se recorta la cara de 8×8 con la capa del sombrero. Si la
  skin no está disponible, se muestra la inicial del nombre.
- **El icono de Halloween sí se ve.** Se usa en la barra lateral (antes había una
  «N»). En Linux, Tauri incrusta el PNG de 512×512 como icono de la ventana; en
  Windows usa el `.ico` del ejecutable. `bundle.icon` incluye `icons/icon.png`
  para que el AppImage use el mismo icono de 512×512.
- **Microsoft abre el flujo heredado por defecto.** El botón **Microsoft** de
  Inicio y el de **Ajustes → Cuenta** llaman al flujo de `login.live.com` sin
  Azure. La vía oficial con Client ID propio queda dentro de
  **Usar mi propia app de Azure (avanzado)**.
- **Instancias en otro PC.** El estado «se descargará al jugar» ya no parece un
  error de configuración: se explica que Minecraft, librerías, assets y Java son
  compartidos y se descargan otra vez en cada PC. La instancia usada más
  recientemente se selecciona sola si `settings.json` no viaja con la carpeta.
  El icono de carpeta abre la raíz de la instancia (con `instance.json`, mods y
  saves) para copiarla, y el modal de ajustes avisa de que el engranaje es
  opcional.

## Windows y Linux

- **Linux 0.1.8**: AppImage x86_64 firmado, con actualización automática.
- **Windows 0.1.8**: instalador NSIS x64 compilado desde Linux con `cargo-xwin`.
  Requiere Windows 10/11 y WebView2; Windows 8/8.1 no es compatible. El `.exe`
  no tiene firma Authenticode, así que SmartScreen puede avisar; la firma del
  actualizador sí está incluida.

## Cuenta local y modo demo

Siguen disponibles, sin Microsoft ni Azure. La cuenta local solo funciona en
servidores con `online-mode=false`; Nebula lo avisa en la interfaz.

## Verificación

- 93 tests de `nebula-core` correctos (1 test live ignorado).
- `svelte-check`: 0 errores y 0 warnings.
- `cargo check` de Tauri correcto.
- AppImage y NSIS generados y firmados con la clave de actualizaciones.
- El instalador de Windows no se ejecutó en un Windows 10/11 real, así que esa
  prueba sigue pendiente.
