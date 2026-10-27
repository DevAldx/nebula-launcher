# Nebula Launcher

Launcher de Minecraft ligero para Linux y Windows. Rust + Tauri 2 + Svelte 5 + TypeScript.

## Estructura

```
crates/nebula-core/   Lógica pura (sin Tauri), 88 tests
  config/             settings.json (validación, guardado atómico, backup si está corrupto)
  instances/          IDs NBL-XXXXXX, modelo, store en disco
  filesystem/         write_atomic multiplataforma, join_safe (anti path traversal)
  downloads/          HTTPS con lista blanca de hosts, SHA-1/SHA-256, tamaño, .part + rename
  minecraft/          metadatos Mojang, reglas, libraries, assets, natives (install.rs)
  java/               detección de runtimes, elección por versión y descarga de Temurin (Adoptium)
  launcher/           argumentos JVM/juego multiplataforma, ejecución y logs con redacción de tokens
  auth/               Microsoft (Azure o flujo heredado) → Xbox Live → XSTS → Minecraft, keyring; cuenta local sin Azure
  mods.rs             Mods locales: listar, activar/desactivar, borrar, importar
  updater.rs          Comparación de versiones + lectura de un manifiesto JSON (sin feed propio)
  background.rs       Imagen de fondo por instancia (PNG/JPG/WEBP), con comprobación de contenido
  share.rs            Instancias compartibles por código: manifiesto, descarga verificada, export
  paths.rs            XDG en Linux; AppData/Local en Windows
installer/             Segunda app de Tauri: instala el AppImage, crea el acceso del menú (Linux)
scripts/               bump-version.sh, build-windows.ps1
.github/workflows/     CI opcional para compilar el instalador NSIS en Windows

`minecraft/fabric.rs` resuelve la versión de Fabric Loader y fusiona su perfil de
lanzamiento con el JSON vanilla (reglas de `inheritsFrom`: `mainClass` propio,
`libraries`/`arguments` añadidas después de las de vanilla; `assetIndex`,
`downloads` y `javaVersion` se quedan como en vanilla).
src-tauri/            Capa fina de comandos IPC (workspace propio, exigido por Tauri)
src/                  Frontend Svelte: Inicio, Instancias, Configuración
```

Módulos del spec que se añaden en sus fases (como módulos de `nebula-core` o crates nuevos):
`minecraft`, `downloads`, `java`, `launcher` (F2), `auth` (F3), `mods` (F4), `updater` (F5).

## Desarrollo

Requisitos Linux: Rust ≥ 1.77, Node ≥ 20 y las dependencias de Tauri 2
(Arch: `sudo pacman -S webkit2gtk-4.1 base-devel librsvg`).

Requisitos Windows: Rust con toolchain MSVC, Node ≥ 20, Microsoft Visual Studio
Build Tools con «Desarrollo para el escritorio con C++» y WebView2 (Windows 10/11
lo trae; el instalador puede descargarlo). Los pasos completos están en
`INSTRUCCIONES_WINDOWS.txt`.

```
npm install
cargo test -p nebula-core      # tests del núcleo
npm run check                  # svelte-check + TypeScript
npm run tauri dev              # ejecutar la app
npm run tauri build            # AppImage (Linux) o NSIS (Windows)
```

## Seguridad (Fase 1)

* Capability de Tauri solo con `core:default`: el frontend no tiene fs, shell ni http.
* Todo el disco pasa por Rust; el ID de instancia se valida con `^NBL-[A-Z0-9]{6}$` antes de formar rutas.
* CSP restrictiva; escritura atómica de JSON.

## Estado (Fases 1 a 7 + instancias compartibles)

| Función | Estado |
|---|---|
| Todo lo de las Fases 1 a 6 | Real, sin cambios |
| **Cuenta local (sin Azure)**: nombre de jugador guardado en disco, UUID determinista (`MD5("OfflinePlayer:<nombre>")`) y aviso de `online-mode=false` | Real |
| **Cuenta Microsoft**: flujo oficial con código de dispositivo y tu propia app de Azure, o flujo heredado sin Azure con el Client ID público del launcher oficial | Real; el heredado es opcional y no oficial |
| **Inicio de sesión en ventana dedicada**: el código de Microsoft se muestra en una ventana aparte, con copiar, abrir en el navegador y Cancelar; la interfaz principal no se cubre con un modal | Real en Linux; ventana y Cancelar probados en Windows Server 2025 |
| **Java automático**: al jugar o importar, si falta la versión mayor exacta que pide Minecraft, descarga el JRE de Eclipse Temurin desde Adoptium, verifica SHA-256 y lo extrae en `data/java/temurin-<mayor>` | Real |
| **Importar por código**: busca `NBL-XXXXXX`, muestra una vista previa (nombre, descripción, versión, loader, nº de archivos, si tu Java cumple el mínimo que pide) y, si confirmas, crea la instancia con su **nombre** y descarga mods/resourcepacks/shaders/config **y el fondo remoto** verificando SHA-1 y tamaño, sin repetir lo que ya esté completo, con progreso real | Real |
| **Exportar**: genera una plantilla de manifiesto con los datos de tu instancia y la lista de tus mods activos, guardada en su propia carpeta | Real — pero sin URLs de descarga, porque no hay dónde subir los archivos (ver abajo) |
| **Windows**: rutas, `java.exe`, detección en Program Files, classpath con `;`, configuración NSIS, iconos `.ico` e instalador compilado desde Linux con `cargo-xwin` | Instalador probado en Windows Server 2025 (GitHub Actions); **falta un Windows 10/11 de escritorio y el flujo completo de juego**. Windows 8/8.1 no es compatible con Tauri 2/WebView2 moderno |
| Icono de Halloween en la app, ventana e instaladores | Real |
| Cancelar una importación a mitad | No incluido todavía — sí hereda los reintentos y la verificación del resto del sistema de descargas |

## Instancias compartibles: qué es real y qué falta

Igual que con las actualizaciones, lo que falta no lo puedo inventar: un servicio real donde
alojar los manifiestos. Un código como `NBL-K7P2X9` se busca en
`<URL que configures>/NBL-K7P2X9.json`. El formato de ese JSON:

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

Cualquier sitio que pueda servir archivos estáticos por https sirve (un repositorio de GitHub vía
`raw.githubusercontent.com`, un bucket S3, tu propio servidor). Solo se permiten los hosts https
que el propio manifiesto declara en sus URLs — ni uno más — y cada archivo se descarga con el
mismo `Downloader` verificado que usan Mojang y Fabric. Al importar se usa el
`name` del manifiesto como nombre de la instancia y, si existe `background_url`,
se descarga el fondo validando que sea PNG, JPG o WEBP. «Exportar» en una instancia te da la
plantilla con los datos correctos y la lista de tus mods; te falta solo subir cada archivo a tu
sitio y pegar su URL (y, si quieres verificación de integridad, su SHA-1) en el JSON.

## Instancias: qué se guarda y cómo moverlas a otro PC

Cada instancia es una carpeta independiente dentro del directorio de datos. La ruta exacta
aparece en **Ajustes → General** y en el aviso de **Ajustes de la instancia**; por ejemplo, en
Linux `~/.local/share/nebula/instances/NBL-XXXXXX/` y en Windows, bajo
`%APPDATA%\dev.nebula.nebula\data\instances\NBL-XXXXXX\`.

Dentro están `instance.json` (nombre, versión, loader, RAM y Java), `mods/`, `config/`,
`resourcepacks/`, `shaderpacks/`, `saves/` y `logs/`. La versión y el loader quedan fijados al
crear la instancia; el engranaje de la tarjeta es **opcional** y solo cambia nombre, RAM, Java y
argumentos. Si la instancia guardó un Java manual de otro PC y esa ruta ya no existe, elige
**Automático** en el engranaje: Nebula descargará el JRE que pida la versión.

Minecraft, las librerías, los assets y Java **no** están dentro de la instancia: son compartidos
y viven en `versions/`, `libraries/`, `assets/` y `java/` del directorio de datos.

Por eso, al copiar solo la carpeta de la instancia a otro PC:

1. Nebula la lista y conserva mods, configs y saves.
2. La marca como «se descargará al jugar» porque en ese PC faltan los archivos compartidos.
   **No falta configuración**: no hay que tocar el engranaje.
3. Al pulsar **JUGAR** se descargan Minecraft, librerías, assets y el Java que pida la versión.

Si quieres evitar esa descarga, copia también `versions/`, `libraries/`, `assets/` y `java/`.
`settings.json` (en el directorio de configuración) guarda la instancia seleccionada; si no lo
copias, Nebula selecciona automáticamente la instancia usada más recientemente. El icono de
carpeta de cada tarjeta abre la raíz de la instancia para copiarla.

## Cómo ejecutar el AppImage ya construido

Descarga `Nebula.Launcher_0.1.11_amd64.AppImage` desde la página de releases:
<https://github.com/DevAldx/nebula-launcher/releases/tag/v0.1.11>

```
chmod +x "Nebula.Launcher_0.1.11_amd64.AppImage"
./"Nebula.Launcher_0.1.11_amd64.AppImage"
```

El AppImage no necesita instalación. Si quieres una entrada en el menú de
aplicaciones, compila el instalador de `installer/` (ver abajo); ese instalador
opcional no se publica como asset en esta release.

## Compilar el AppImage tú mismo

```
npm install
npm run tauri build -- --bundles appimage
# el .AppImage queda en src-tauri/target/release/bundle/appimage/

cd installer && npm install
npm run tauri build -- --bundles appimage
# deja el instalador junto al AppImage del launcher antes de ejecutarlo
```

## Actualizaciones

Nebula usa **`tauri-plugin-updater`** con actualizaciones firmadas (minisign):

- Al arrancar consulta el feed configurado en `src-tauri/tauri.conf.json`
  (`plugins.updater.endpoints`): `https://github.com/DevAldx/nebula-launcher/releases/latest/download/latest.json`.
- Si hay una versión más nueva, muestra un diálogo con las novedades.
- «Actualizar ahora» descarga el artefacto firmado, **verifica la firma** contra la clave
  pública embebida, instala y reinicia la aplicación.
- También se puede comprobar a mano en Configuración → Actualizaciones.

### Cómo se numeran las versiones

Nebula usa **MAYOR.MENOR.PARCHE** (`0.1.11`, `0.1.12`, `0.2.0`, `1.0.0`):

- **Parche** (`0.1.11` → `0.1.12`): arreglos, textos, iconos y cambios pequeños.
- **Menor** (`0.1.12` → `0.2.0`): funciones nuevas o cambios relevantes.
- **Mayor** (`0.2.0` → `1.0.0`): rediseños o cambios incompatibles.

Los cambios muy pequeños se agrupan en un solo parche; no se publica una
versión por cada commit. La guía completa está en `VERSIONADO.md`.

Para publicar una versión: subir la versión con `scripts/bump-version.sh`
(`patch`, `minor`, `major` o `set X.Y.Z`), compilar con
`TAURI_SIGNING_PRIVATE_KEY` en cada plataforma, generar `latest.json` con
`create_latest_json.sh` y subir todos los artefactos firmados (`.AppImage`, `.AppImage.sig`,
`.exe`/`.nsis.zip` de Windows y `latest.json`) al mismo release.
Los detalles están en `INSTRUCCIONES_ACTUALIZACION.txt` y `INSTRUCCIONES_WINDOWS.txt`.

`nebula-core::updater` se conserva como **comprobación manual opcional** contra un manifiesto
JSON propio (`{"version","url","notes"}`) configurable en Configuración → Actualizaciones →
«Manifiesto manual (avanzado)». No descarga ni reemplaza nada: solo compara y abre la descarga
en el navegador.

## Windows

El código ya contempla Windows (rutas, `java.exe`, detección de Java en
`Program Files`, classpath con `;`, escritura atómica compatible, iconos `.ico`)
y hay dos formas de compilar el instalador NSIS:

1. **En un PC con Windows**: `scripts/build-windows.ps1` (requiere Rust MSVC,
   Node, Build Tools C++ y la clave de firma).
2. **Con GitHub Actions**: `.github/workflows/build-windows.yml` compila y
   firma al crear un tag `v*` o al lanzarlo a mano. Necesita los secretos
   `TAURI_SIGNING_PRIVATE_KEY` y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

El instalador se genera en `src-tauri/target/release/bundle/nsis/`. La versión
0.1.11 se compiló desde Linux con `cargo-xwin` y NSIS. Su instalación y la
ventana dedicada de inicio de sesión se probaron en un runner `windows-latest`
de GitHub Actions (Windows Server 2025): instalación correcta, ventana
«Nebula Launcher», botón Microsoft, ventana «Nebula — Iniciar sesión» y
Cancelar. **Falta probarlo en un Windows 10/11 de escritorio** y el flujo
completo de juego. Windows 8/8.1 no es viable: Tauri 2 y el WebView2 actual ya
no lo soportan. La guía está en `INSTRUCCIONES_WINDOWS.txt`.

**Wine/Bottles no es compatible.** El instalador NSIS intenta descargar e
instalar WebView2 y falla (código 2) porque Wine no lo soporta; aunque se
forzara la instalación, la app Tauri no podría renderizar sin WebView2. En
Linux hay que usar el AppImage nativo. Para la versión de Windows hace falta un
Windows 10/11 real o una máquina virtual.

## Cuenta local: jugar sin Microsoft ni Azure

La opción más sencilla es una **cuenta local**:

* Escribes un nombre de jugador (3-16 caracteres: letras, números y `_`).
* Se guarda en `account.json` (sin contraseña ni token).
* El UUID se calcula igual que el servidor de Minecraft:
  `MD5("OfflinePlayer:<nombre>")` con bits de versión 3 y variante RFC 4122.
  Es determinista: el mismo nombre conserva inventario y permisos entre sesiones.
* Funciona en servidores con `online-mode=false`.

**Aviso importante:** en un servidor oficial (`online-mode=true`) el servidor
comprueba la sesión con Mojang y rechazará una cuenta local. Nebula lo avisa en
el formulario y en Ajustes; para esos servidores hay que usar Microsoft.

## Cuenta de Microsoft: dos vías

Minecraft usa el inicio de sesión de Microsoft (Xbox Live + XSTS + Minecraft
Services). Nebula ofrece dos formas de entrar; en ambas la contraseña se escribe
en una página real de Microsoft y Nebula solo recibe tokens. El código de
dispositivo se muestra en una **ventana aparte**, no encima de la interfaz
principal.

El botón **Microsoft** de Inicio y de **Ajustes → Cuenta** usa directamente la
vía heredada, sin Azure. La vía oficial con tu propia app queda en un apartado
avanzado de Ajustes.

### Vía oficial: tu propia app de Azure

Es la única aprobada por Mojang. Requiere registrar una aplicación y esperar su
aprobación para la API de Minecraft. En Nebula está en
**Ajustes → Cuenta → Usar mi propia app de Azure (avanzado)**.

1. Entra en <https://portal.azure.com> → **Azure Active Directory** → **App registrations** → **New registration**.
2. Nombre: lo que quieras (p. ej. "Nebula Launcher"). En **Supported account types** elige
   **Personal Microsoft accounts only**. Sin URI de redirección (el flujo de código de dispositivo no la necesita).
3. Una vez creada, copia el **Application (client) ID** (un GUID) y pégalo en Nebula, en
   **Configuración → Cuenta → Client ID de Microsoft**.
4. En **Authentication**, activa **"Allow public client flows"** → Sí, y guarda.
5. **Paso obligatorio y fácil de pasar por alto**: una aplicación nueva NO tiene permiso para hablar
   con la API de Minecraft hasta que Microsoft la aprueba. Envíala en
   <https://aka.ms/mce-reviewappid>. Sin este paso, Nebula mostrará el error
   *"Invalid app registration"* al intentar iniciar sesión. La aprobación no es instantánea ni
   está garantizada; es un proceso de Microsoft fuera del control de Nebula.

### Vía heredada: sin Azure (no oficial)

Si no puedes registrar una app (por ejemplo, porque tu centro educativo bloquea
el registro de aplicaciones), el botón **Microsoft** de Inicio y de
**Ajustes → Cuenta** usa el flujo antiguo de `login.live.com` con el Client ID
público del launcher oficial de Minecraft (`00000000402b5328`).

* No necesitas Azure ni tarjeta.
* Se abre una página de Microsoft en tu navegador y escribes un código, igual
  que en la vía oficial.
* **No es una integración oficial**: el Client ID es de Mojang/Microsoft, no
  tuyo, y Microsoft podría cambiarlo o bloquearlo sin aviso. Funciona para uso
  personal y para un grupo pequeño de amigos, no como base de un servicio público.
* Si algún día deja de funcionar, la cuenta local y el modo demo siguen
  disponibles.

La sesión heredada se guarda en el mismo almacén seguro que la oficial y se
renueva contra `login.live.com`; no se vuelve a pedir la contraseña en cada
partida.

Sin ninguna cuenta configurada, Nebula funciona en **modo demo**; no es
necesario iniciar sesión para probar las Fases 1 y 2.

## Cómo se decide e instala el Java

Cada versión de Minecraft declara el Java que necesita. En modo automático
Nebula busca primero un runtime instalado con la versión mayor exacta; si no
existe, descarga un **JRE de Eclipse Temurin** desde la API oficial de Adoptium,
verifica el **SHA-256** que publica esa misma API y lo extrae en
`data/java/temurin-<mayor>`. Al jugar y al importar una instancia se hace esta
comprobación, así que una instalación recién importada queda lista para jugar.

Para versiones anteriores a 1.13 que piden Java 8 se exige exactamente Java 8.
Si prefieres un Java concreto, puedes elegirlo en los ajustes de la instancia;
en ese caso Nebula valida la ruta y no descarga nada. La detección incluye
`JAVA_HOME`, `PATH`, `/usr/lib/jvm`, Adoptium, Microsoft, Zulu, Corretto y
`data/java` en Windows y Linux.

## Seguridad (Fases 2, 3 y 7)

* Solo HTTPS y solo hosts conocidos (Mojang, Adoptium/GitHub para Java y, para la cuenta,
  Microsoft/Xbox/Minecraft Services, incluido `login.live.com` para el flujo heredado); los
  redirects no pueden salir de la lista.
* Cada descarga se verifica (tamaño + SHA-1; los JRE además con SHA-256) antes de renombrarse;
  nada queda a medias.
* Los natives y los JRE se extraen con protección contra zip-slip y entradas gigantes.
* La cuenta local no guarda secretos: solo nombre, UUID, tipo y flujo en `account.json`.
* Se aplica el config de log4j de Mojang (mitigación de Log4Shell en versiones afectadas).
* Nebula nunca pide ni ve tu contraseña de Microsoft: el login ocurre en tu navegador, en una
  página real de Microsoft, tanto en el flujo de Azure como en el heredado.
* El refresh token se guarda en el almacén seguro del sistema (Secret Service / KWallet /
  Credential Manager vía la crate `keyring`), nunca en un archivo plano; los tokens se censuran
  también en los logs del juego.
* El frontend solo puede elegir un Java de la lista detectada, nunca una ruta arbitraria, y solo
  puede abrir en el navegador el enlace de inicio de sesión de Microsoft (host fijo en el backend).
