# Nebula Launcher 0.1.10 — notas de la versión

Fecha: 2026-10-25

## Correcciones

- **La cara de la skin vuelve a aparecer.** El recorte de la textura usaba
  posiciones negativas de `background-position`, que dejaban visible una zona
  vacía de la skin en vez de la cara. Ahora la textura se escala 8× y se
  desplaza en píxeles hasta la región correcta: cara en (8,8) y sombrero en
  (40,8). Funciona con cualquier tamaño del avatar.
- **El icono de Halloween es el icono de la app.** El instalador opcional
  todavía escribía el icono de la bola morada en el menú de aplicaciones y lo
  usaba como icono de su propia ventana. Ahora usa el mismo icono de Halloween
  que el launcher, tanto en el instalador como en el acceso del menú.
  En este PC también se reemplazaron los iconos ya instalados; si el menú o la
  barra de tareas siguen mostrando el anterior, cierra y vuelve a abrir Nebula
  (o reinicia la sesión) para que KDE refresque la caché.

La cabeza solo se muestra cuando hay una **cuenta de Microsoft con skin**. Las
cuentas locales muestran la inicial del nombre, y una tarjeta «Sin cuenta» no
tiene skin que mostrar. Si vienes de 0.1.8, recuerda que 0.1.9 borró la sesión
fantasma: hay que iniciar sesión una vez con Microsoft.

## Windows y Linux

- **Linux 0.1.10**: AppImage x86_64 firmado, con actualización automática.
- **Windows 0.1.10**: instalador NSIS x64 compilado desde Linux con `cargo-xwin`.
  Requiere Windows 10/11 y WebView2; Windows 8/8.1 no es compatible. El `.exe`
  no tiene firma Authenticode, así que SmartScreen puede avisar; la firma del
  actualizador sí está incluida.

## Verificación

- `svelte-check`: 0 errores y 0 warnings.
- 93 tests de `nebula-core` correctos (1 test live ignorado).
- `cargo check` de Tauri correcto.
- Cara y sombrero verificados visualmente en el binario 0.1.10 con una cuenta
  de prueba y una skin real de Mojang.
- AppImage y NSIS generados y firmados con la clave de actualizaciones.
- El instalador de Windows no se ejecutó en un Windows 10/11 real, así que esa
  prueba sigue pendiente.
