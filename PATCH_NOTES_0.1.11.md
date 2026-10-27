# Nebula Launcher 0.1.11 — notas de la versión

Fecha: 2026-10-26

## Cambios

- **El inicio de sesión de Microsoft usa una ventana propia.** El código ya no
  se muestra como un modal encima de la interfaz principal. Al pulsar
  «Microsoft» se abre una ventana separada, centrada y de tamaño fijo, con el
  código, un botón para copiarlo, el enlace a Microsoft y un botón Cancelar.
- **La ventana principal se mantiene limpia.** Mientras espera, solo muestra el
  estado «Esperando a Microsoft…». Al completar el login, cancelar o cerrar la
  ventana de inicio de sesión, la ventana principal vuelve a su estado normal.
- **Cancelar detiene el flujo de verdad.** Cerrar la ventana con la X o pulsar
  Cancelar descarta el código pendiente y reinicia la espera. Abrir la cuenta
  local cancela un login de Microsoft en curso para que no compitan dos cuentas.
- **Respaldo si la ventana no se puede abrir.** En ese caso se usa el diálogo
  interno, ahora con fondo opaco y desenfoque para que no se mezcle con la UI.
- La nueva ventana usa el icono de Halloween de la aplicación.

## Windows y Linux

- **Linux 0.1.11**: AppImage x86_64 firmado, con actualización automática.
- **Windows 0.1.11**: instalador NSIS x64. Requiere Windows 10/11 y WebView2;
  Windows 8/8.1 no es compatible. El `.exe` no tiene firma Authenticode, así
  que SmartScreen puede avisar; la firma del actualizador sí está incluida.

## Artefactos

- AppImage: `Nebula Launcher_0.1.11_amd64.AppImage`
  (SHA-256 `b0d300593537b8d30c72658da2f91526fecceab729eec2422c72aff9feed3791`).
- Instalador Windows: `Nebula Launcher_0.1.11_x64-setup.exe`
  (SHA-256 `2473ceb697ff7fc945f76007c0c76dc9cc851047430081a2be84e23f2052ab2c`).

## Verificación

- `svelte-check`: 0 errores y 0 warnings.
- `cargo check --release` correcto.
- `npm run build` y `npm run tauri build -- --no-bundle` correctos.
- Prueba real del AppImage 0.1.11 en Linux con un HOME temporal: la ventana
  de login se abrió separada, mostró el código, «Abrir en el navegador» llamó a
  `xdg-open` con `https://www.microsoft.com/link`, y Cancelar y la X cerraron
  el flujo y reiniciaron la ventana principal.
- Login completo con una cuenta de prueba: la ventana principal mostró la
  cuenta y la skin después de terminar el flujo de Microsoft.
- El instalador de Windows se genera y se firma. La prueba automatizada se
  ejecuta en un runner `windows-latest` de GitHub Actions (Windows Server 2025,
  no un Windows 10/11 de escritorio). El flujo completo de cuenta local,
  creación de instancia y lanzamiento de Minecraft en Windows sigue pendiente.
