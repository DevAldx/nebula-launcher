# Nebula Launcher 0.1.7 — notas de la versión

Fecha: 2026-10-24

## Lo nuevo

- **Cuenta de Microsoft sin Azure (flujo heredado, opcional y no oficial).**
  En **Ajustes → Cuenta → Microsoft heredado** puedes iniciar sesión con una
  cuenta premium de Minecraft usando el flujo antiguo de `login.live.com` y el
  Client ID público del launcher oficial (`00000000402b5328`). No necesitas
  registrar una app, tarjeta ni tenant de Azure.
  - Funciona con código de dispositivo y navegador, igual que el flujo oficial.
  - La sesión se guarda en el almacén seguro del sistema y se renueva sola.
  - Validado con una cuenta real: código de dispositivo, Xbox Live, XSTS,
    Minecraft, perfil y renovación con el refresh token.
  - **No es una integración oficial de Mojang.** Microsoft podría cambiarla o
    bloquearla sin aviso. Para uso personal y grupos pequeños; si falla, usa la
    vía oficial con tu propia app de Azure o la cuenta local.
- **Icono de Halloween** en la aplicación, la ventana y los instaladores.
- **Windows 0.1.7**: instalador NSIS x64 compilado con `cargo-xwin` desde Linux.
  Requiere Windows 10/11 y WebView2. Windows 8/8.1 no es compatible.
- **Linux 0.1.7**: AppImage firmado y actualización automática.

## Cuenta local

Sigue disponible sin Microsoft ni Azure. Solo funciona en servidores con
`online-mode=false`; Nebula lo avisa en la interfaz.

## Vía oficial con Azure

La opción **Microsoft (avanzado, requiere Azure)** sigue igual: configura tu
Client ID, activa «Allow public client flows» y envía la app a revisión de
Mojang. Es la única vía aprobada oficialmente.

## Actualización

El feed `latest.json` incluye Linux y Windows. La actualización se verifica con
la firma minisign incluida en la app.

## Verificación

- 92 tests de `nebula-core` correctos (1 test live ignorado), `svelte-check` con
  0 errores y 0 warnings, y `cargo check` de Tauri correcto.
- Flujo heredado probado contra Microsoft con una cuenta real: código de
  dispositivo, Xbox Live, XSTS, Minecraft, perfil y refresh token.
- AppImage y NSIS generados y firmados. El instalador de Windows no se ejecutó
  en un Windows real.

## Notas técnicas

- Hosts de autenticación añadidos a la lista blanca: `login.live.com`.
- El flujo heredado usa `RpsTicket: t=<token>`; el de Azure sigue usando `d=`.
- Los tests de `nebula-core` cubren el código de dispositivo heredado, el
  refresco y la selección de flujo al reanudar la sesión.
- El instalador de Windows no se ha probado en un Windows 10/11 real en este
  entorno; si algo falla, guarda el log y el mensaje exacto.
