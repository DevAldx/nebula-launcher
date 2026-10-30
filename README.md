# Nebula Launcher

Launcher de Minecraft ligero para Linux y Windows. Rust + Tauri 2 + Svelte 5 +
TypeScript. No incluye Minecraft: descarga los archivos oficiales desde Mojang
con tu propia cuenta.

## Estado

- **Cuenta local**: nombre de jugador, UUID determinista
  (`MD5("OfflinePlayer:<nombre>")`) y aviso de `online-mode=false`.
- **Cuenta Microsoft**: flujo oficial con código de dispositivo y tu propia app
  de Azure, o flujo heredado sin Azure. La contraseña siempre se escribe en una
  página de Microsoft; Nebula solo recibe tokens.
- **Java automático**: si falta la versión exacta, descarga un JRE de Eclipse
  Temurin desde Adoptium, verifica su SHA-256 y lo extrae en
  `data/java/temurin-<mayor>`.
- **Instancias**: carpetas independientes con `instance.json`, mods, configs,
  resourcepacks, shaderpacks, saves y logs. Minecraft, librerías, assets y Java
  son compartidos entre instancias.
- **Importar por código**: vista previa con nombre, versión, loader, archivos y
  Java mínimo; descarga verificada de mods, resourcepacks, shaders, configs y
  fondo remoto.
- **Exportar**: genera la plantilla del manifiesto con los datos de la instancia
  y sus mods activos. Las URLs de descarga las pones tú.
- **Personalización**: icono propio, tema, acento, transparencia, animaciones y
  fondo PNG/JPG/WEBP/GIF animado.
- **Ventana de bienvenida**: animación con progreso real, comprobación de
  versión, detección de Java y lectura de instancias.
- **Windows**: rutas, `java.exe`, classpath, iconos `.ico` e instalador NSIS.
- **Pendiente**: cancelar una importación a mitad.

## Requisitos

- **Linux**: Rust ≥ 1.77, Node ≥ 20 y dependencias de Tauri 2. En Arch:
  `sudo pacman -S webkit2gtk-4.1 base-devel librsvg`.
- **Windows**: Rust con toolchain MSVC, Node ≥ 20, Visual Studio Build Tools con
  «Desarrollo para el escritorio con C++» y WebView2. Windows 10/11; Windows
  8/8.1 no es compatible con Tauri 2/WebView2 moderno.

## Desarrollo

```bash
npm install
cargo test -p nebula-core      # 106 tests del núcleo
npm run check                  # svelte-check + TypeScript
npm run tauri dev              # ejecutar la app
./scripts/build-appimage.sh    # AppImage Linux firmado
npm run tauri build -- --bundles nsis  # instalador Windows
```

Estructura principal:

```
crates/nebula-core/  Lógica pura sin Tauri (config, instancias, descargas,
                     Minecraft, Java, auth, mods, share, updater)
installer/           Instalador del AppImage para Linux
scripts/             bump-version.sh, build-appimage.sh, build-windows.ps1
src-tauri/           Comandos IPC (workspace propio exigido por Tauri)
src/                 Frontend Svelte
```

## Ejecutar el AppImage

Descarga `Nebula.Launcher_0.3.1_amd64.AppImage` desde
<https://github.com/DevAldx/nebula-launcher/releases/tag/v0.3.1>:

```bash
chmod +x "Nebula.Launcher_0.3.1_amd64.AppImage"
./"Nebula.Launcher_0.3.1_amd64.AppImage"
```

No necesita instalación. Para una entrada en el menú, compila el instalador de
`installer/`; ese instalador necesita el AppImage **y** su `.AppImage.sig`, y
verifica la firma minisign antes de copiar nada.

El AppImage usa WebKitGTK 4.1 y GTK 3 del sistema si están disponibles; si no,
usa la copia empaquetada. Para forzarla:
`NEBULA_BUNDLED_WEBKIT=1 ./"Nebula Launcher_0.3.1_amd64.AppImage"`.

## Compilar y publicar

```bash
npm install
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/nebula-launcher.key)"
./scripts/build-appimage.sh

cd installer && npm install
npm run tauri build -- --bundles appimage
# deja el instalador junto al AppImage y su .sig
```

El AppImage queda en `src-tauri/target/release/bundle/appimage/` y el instalador
NSIS en `src-tauri/target/release/bundle/nsis/`.

Para publicar: sube la versión con `scripts/bump-version.sh` (`patch`, `minor`,
`major` o `set X.Y.Z`), compila en cada plataforma, genera `latest.json` con
`create_latest_json.sh` y sube los artefactos firmados y `latest.json` al mismo
release. Los detalles están en `INSTRUCCIONES_ACTUALIZACION.txt` y
`INSTRUCCIONES_WINDOWS.txt`.

## Actualizaciones y versiones

Nebula usa `tauri-plugin-updater` con firmas minisign. Al arrancar consulta
`https://github.com/DevAldx/nebula-launcher/releases/latest/download/latest.json`;
si hay una versión nueva, muestra las novedades, descarga el artefacto, verifica
la firma e instala. También se puede comprobar a mano en **Ajustes →
Actualizaciones**.

El formato es **MAYOR.MENOR.PARCHE**: parche para arreglos y cambios pequeños,
menor para funciones nuevas y mayor para rediseños o cambios incompatibles. Los
cambios pequeños se agrupan en un solo parche. Guía completa: `VERSIONADO.md`.

## Cuentas

### Local

Escribe un nombre de 3-16 caracteres (letras, números y `_`). Se guarda en
`account.json` sin contraseña ni token. Funciona en servidores con
`online-mode=false`; un servidor oficial rechazará la cuenta local.

### Microsoft

Hay dos vías; en ambas la contraseña se escribe en una página real de Microsoft:

- **Oficial (Azure)**: registra una app en <https://portal.azure.com> con
  «Personal Microsoft accounts only», activa «Allow public client flows», pega
  el Client ID en **Ajustes → Cuenta** y envía la app a revisión en
  <https://aka.ms/mce-reviewappid>. Sin esa aprobación aparece *Invalid app
  registration*.
- **Heredada (sin Azure, no oficial)**: usa el Client ID público
  `00000000402b5328`. No requiere Azure ni tarjeta, pero Microsoft podría
  cambiarlo o bloquearlo. Sirve para uso personal o grupos pequeños, no como
  base de un servicio público.

Sin cuenta configurada, Nebula funciona en **modo demo**.

## Java

Cada versión de Minecraft declara su Java. En modo automático Nebula busca un
runtime instalado con la versión mayor exacta; si no existe, descarga Temurin
desde Adoptium, verifica el SHA-256 y lo extrae en `data/java/temurin-<mayor>`.
Para versiones anteriores a 1.13 que piden Java 8 se exige exactamente Java 8.
También puedes elegir un Java concreto en los ajustes de la instancia.

## Instancias compartibles

Un código `NBL-XXXXXX` se busca en `<URL configurada>/NBL-XXXXXX.json`. Sirve
cualquier sitio que publique archivos estáticos por HTTPS (GitHub raw, S3, tu
servidor). Solo se permiten los hosts que el manifiesto declara y cada archivo
se descarga con el mismo `Downloader` verificado que usa Mojang.

```json
{
  "code": "NBL-K7P2X9",
  "name": "SkyFactory liviano",
  "description": "Pack de supervivencia con Fabric",
  "background_url": "https://.../fondo.jpg",
  "minecraft_version": "1.21.1",
  "loader": "fabric",
  "loader_version": "0.15.11",
  "java": { "min_major": 21 },
  "memory": { "min_mb": 2048, "max_mb": 4096 },
  "mods": [
    { "file_name": "sodium.jar", "url": "https://.../sodium.jar", "sha1": "...", "size": 123456 }
  ],
  "resourcepacks": [], "shaderpacks": [], "config_files": []
}
```

Cada instancia vive en `instances/NBL-XXXXXX/`. Al copiarla sola a otro PC,
Nebula conserva mods, configs y saves y descarga al jugar lo que falte de
Minecraft, librerías, assets y Java. Si copias también `versions/`,
`libraries/`, `assets/` y `java/`, evitas esa descarga.

## Seguridad

- Solo HTTPS y hosts conocidos; los redirects no pueden salir de la lista.
- Descargas verificadas por tamaño y SHA-1 (los JRE además por SHA-256) antes de
  renombrarse; nada queda a medias.
- Natives y JRE extraídos con protección contra zip-slip y entradas gigantes.
- Escritura atómica de JSON, `join_safe` contra path traversal y rechazo de
  symlinks en rutas críticas.
- El refresh token vive en el almacén seguro del sistema (Secret Service,
  KWallet o Credential Manager) y los tokens se censuran en los logs.
- El frontend solo puede elegir un Java de la lista detectada y abrir el enlace
  de inicio de sesión de Microsoft; no tiene fs, shell ni http.

## Windows

El instalador NSIS se puede compilar en Windows con
`scripts/build-windows.ps1`, desde Linux con `cargo-xwin` + NSIS 3.11, o probar
un `.exe` ya publicado con `.github/workflows/test-windows-installer.yml`. Se
probó en Windows Server 2025 (GitHub Actions): instalación, versión, ventana
principal y app viva. Falta un Windows 10/11 de escritorio y el flujo completo
de juego.

**Wine/Bottles no es compatible**: el instalador intenta instalar WebView2 y
Wine no lo soporta. En Linux hay que usar el AppImage nativo.

## Aviso legal

Nebula Launcher es un proyecto independiente. **No está afiliado, asociado,
autorizado, respaldado ni patrocinado por Mojang Studios, Microsoft Corporation
ni sus filiales.** Minecraft es una marca registrada de Mojang Studios. Nebula
no incluye ni distribuye Minecraft.

El copyright, las marcas y los créditos completos están en **Acerca de**. La
barra lateral muestra «© 2026 Nebula» y «Minecraft © Mojang · No afiliado».

## Documentación

- `VERSIONADO.md`: formato y proceso de versiones.
- `INSTRUCCIONES_ACTUALIZACION.txt`: compilar, firmar y publicar.
- `INSTRUCCIONES_WINDOWS.txt`: instalador y pruebas en Windows.
- `PATCH_NOTES_0.3.2.md`: cambios de la versión actual del repositorio.
