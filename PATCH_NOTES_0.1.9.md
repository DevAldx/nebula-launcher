# Nebula Launcher 0.1.9 — notas de la versión

Fecha: 2026-10-25

## Corrección principal: la cuenta de Microsoft ya no es «fantasma»

En 0.1.8 (y anteriores) la app podía mostrar **«conectado»** con tu cuenta de
Microsoft, pero al pulsar **JUGAR** arrancaba como si no hubiera ninguna cuenta.
La causa era el almacén de credenciales: `keyring` se compilaba sin su backend
nativo, así que usaba un almacén `mock` **en memoria**. El perfil (nombre, UUID
y skin) sí se guardaba en `account.json`, pero el refresh token se perdía al
cerrar la app. Al volver a abrirla, la tarjeta mostraba la sesión cacheada y el
juego no podía renovarla, por lo que caía en modo demo sin explicarlo.

Ahora:

- **Linux** usa Secret Service (KWallet o GNOME Keyring) mediante
  `sync-secret-service` + `crypto-rust`.
- **Windows** usa el Administrador de credenciales de Windows
  (`windows-native`).
- **macOS** usa el Llavero (`apple-native`).
- El refresh token sobrevive al cierre y la cuenta se renueva al jugar.
- `get_account` solo devuelve una cuenta de Microsoft si el token existe de
  verdad. Si el perfil quedó huérfano, se borra y la interfaz deja de mostrar
  una sesión inexistente.
- Si al jugar hay un perfil cacheado pero falta el token, Nebula muestra un
  error claro («inicia sesión de nuevo») en vez de arrancar en modo demo.
- Si el juego falla por la sesión, la interfaz vuelve a leer la cuenta para no
  seguir mostrando «conectado».

**Corrección de las notas de 0.1.8:** allí se afirmaba que la sesión se guardaba
en el almacén seguro. El perfil sí se guardaba, pero el token no persistía por
el backend `mock`. Esta versión corrige esa afirmación y el fallo real.

Las cuentas guardadas por 0.1.8 no tienen token utilizable, así que al abrir
0.1.9 esa tarjeta desaparecerá y habrá que **iniciar sesión una vez más**. A
partir de entonces la sesión sí se conserva. La cuenta local y el modo demo no
cambian; la cuenta local sigue sirviendo solo con `online-mode=false`.

## Windows y Linux

- **Linux 0.1.9**: AppImage x86_64 firmado, con actualización automática.
- **Windows 0.1.9**: instalador NSIS x64 compilado desde Linux con `cargo-xwin`.
  Requiere Windows 10/11 y WebView2; Windows 8/8.1 no es compatible. El `.exe`
  no tiene firma Authenticode, así que SmartScreen puede avisar; la firma del
  actualizador sí está incluida.

## Verificación

- 93 tests de `nebula-core` correctos (1 test live ignorado).
- `svelte-check`: 0 errores y 0 warnings.
- `cargo check` de Tauri correcto.
- Persistencia del backend nativo comprobada con dos procesos separados:
  uno guardó el secreto y otro lo leyó después (servicio de prueba, sin tocar
  la sesión real).
- AppImage y NSIS generados y firmados con la clave de actualizaciones.
- El instalador de Windows no se ejecutó en un Windows 10/11 real, así que esa
  prueba sigue pendiente.
