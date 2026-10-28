# Nebula Launcher 0.2.0 — notas de la versión

Fecha: 2026-10-27

## Cambios

- **Personalización del launcher en «Personalizar → Launcher».** La página
  tiene una vista previa real y dos pestañas: **Launcher** e **Instancias**.
  El fondo del launcher es general; el de cada instancia sigue siendo
  independiente.
- **Icono propio.** Se puede elegir un PNG cuadrado de hasta 4 MB y
  2048×2048. Se aplica al instante en la barra lateral, en la vista previa y
  como icono de la ventana. «Restablecer» vuelve al icono de Halloween.
- **Colores e interfaz.** Tema oscuro, negro o púrpura; color de acento
  `#RRGGBB` con muestras predefinidas; transparencia de paneles y animaciones.
  Los cambios de color y efectos se aplican con **Guardar apariencia**.
- **Fondo general con imágenes y GIF animados.** Admite PNG, JPG, WEBP y GIF de
  hasta 15 MB. Los GIF y WEBP animados se reproducen tal cual, sin conversión.
  Se puede ajustar como cubrir, contener o mosaico, con desenfoque de 0 a 40 px
  y oscurecimiento de 0 a 90 %.
- **Guardado inmediato de la identidad.** El icono y el fondo se guardan en
  cuanto se eligen y se recuperan al volver a abrir Nebula. El fondo y el icono
  viven en el directorio de configuración, no dentro de una instancia.
- **Aviso legal de no afiliación.** La barra lateral, Personalizar, Ajustes →
  Acerca de y la ventana de inicio de sesión indican que Nebula Launcher no está
  afiliado, asociado, autorizado, respaldado ni patrocinado por Mojang Studios,
  Microsoft Corporation ni sus filiales, y que Minecraft es una marca de Mojang
  Studios.

## Compatibilidad

- **Linux**: AppImage x86_64 firmado, con actualización automática.
- **Windows**: instalador NSIS x64. Requiere Windows 10/11 y WebView2; Windows
  8/8.1 no es compatible con Tauri 2/WebView2 moderno. El `.exe` no tiene firma
  Authenticode, así que SmartScreen puede avisar; la firma del actualizador sí
  está incluida.
- **Wine/Bottles**: no es compatible. En Linux hay que usar el AppImage nativo.

## Artefactos

- AppImage: `Nebula Launcher_0.2.0_amd64.AppImage`
  (SHA-256 `fa59a7d12cb19a2666ed7296ef54f752685796dd386d7732fd81617203d36317`).
- Instalador Windows: `Nebula Launcher_0.2.0_x64-setup.exe`
  (SHA-256 `5484422eec59fe2b284c17e20de5d99cbc70d1efdba592fc762e2fa0f7b57443`).

## Verificación

- `cargo test -p nebula-core`: 100 tests correctos.
- `npm run check`: 0 errores y 0 warnings.
- `npm run build`: correcto.
- `cargo check` en `src-tauri`: correcto.
- Prueba visual completa en Linux con AppImage de desarrollo y XDG aislados:
  cambio y guardado de color de acento y tema; subida de icono PNG y
  restablecimiento; subida de fondo GIF animado con desenfoque de 21 px y
  oscurecimiento del 67 %; persistencia tras reiniciar; «Quitar fondo»;
  «Restablecer colores»; pestaña Instancias; aviso legal en la barra lateral,
  Personalizar, Ajustes y la ventana de inicio de sesión.
- El AppImage 0.2.0 de release se probó también con XDG aislados: arranque,
  versión 0.2.0, página Personalizar, icono PNG, fondo GIF animado, guardado
  del tema púrpura, persistencia tras reiniciar y aviso legal visible.
- Prueba del instalador Windows en GitHub Actions (Windows Server 2025,
  WebView2 155.0.4283.62, ejecución 40628354950): SHA-256 correcto,
  instalación silenciosa con código 0, versión 0.2.0, ventana «Nebula
  Launcher», botón Microsoft, ventana «Nebula — Iniciar sesión» y Cancelar.
  La personalización se probó con UI Automation: icono PNG, fondo GIF, tema
  púrpura, acento `#22c55e`, desenfoque de 15 px y oscurecimiento del 60 %;
  los archivos quedaron en `%APPDATA%\nebula\nebula\config` y persistieron
  tras reiniciar. El aviso legal también se comprobó en Personalizar.

## Limitaciones conocidas

- El icono del menú del sistema no cambia en caliente; solo el de la ventana y
  la barra lateral. El del menú sigue siendo el del paquete instalado.
- La prueba de Windows es en Windows Server 2025, no en un Windows 10/11 de
  escritorio. No se probaron en Windows el flujo completo de juego ni la
  creación de una instancia.
