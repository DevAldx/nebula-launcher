//! Lógica real del instalador: busca el AppImage de Nebula Launcher junto a sí
//! mismo (o recibe uno elegido a mano), lo copia a una ubicación estable,
//! lo marca ejecutable y crea un acceso en el menú de aplicaciones.
//!
//! Nada de esto es simulado: son operaciones de archivo reales sobre el disco
//! del usuario. Lo único que no cubre este instalador es `.deb`/`.rpm`/paquete
//! de Arch (la Fase 5 solo pide AppImage primero) y una actualización
//! automática (ver el README del launcher).

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use minisign_verify::{PublicKey, Signature};
use serde::Serialize;

/// Icono embebido en el propio binario: así el instalador no depende de que
/// el AppImage del launcher ya esté extraído para tener algo que mostrar.
const ICON_BYTES: &[u8] = include_bytes!("../icons/nebula.png");

/// Clave pública minisign con la que se firman los AppImage publicados.
/// La privada vive fuera del repositorio y la usa `scripts/build-appimage.sh`.
const LAUNCHER_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDMxMUM1NTdGMURDNDk4RDIKUldUU21NUWRmMVVjTWViSFMwSUwyNDRaazBZczM4dVMwR3dNMmdPankxckRDN0phTXBZNkl3eE8K";

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
struct SourceInfo {
    path: String,
    size_bytes: u64,
}

#[derive(Serialize)]
struct InstallResult {
    installed_path: String,
    desktop_file: String,
}

fn home_dir() -> CmdResult<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| "no se pudo determinar la carpeta personal (HOME)".to_owned())
}

fn is_appimage_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".appimage")
}

/// El nombre llega desde el webview: no debe contener separadores ni rutas.
fn validate_appimage_name(name: &str) -> CmdResult<()> {
    if name.is_empty() || name.contains(['/', '\\', '\0']) || !is_appimage_name(name) {
        return Err("solo se admiten archivos .AppImage".into());
    }
    Ok(())
}

fn signature_path(source: &Path) -> PathBuf {
    let mut path = source.as_os_str().to_os_string();
    path.push(".sig");
    PathBuf::from(path)
}

/// Tauri guarda las firmas como el texto minisign codificado en base64.
/// Aceptamos también el texto minisign directo para poder verificar a mano.
fn decode_signature(raw: &str) -> CmdResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("la firma del AppImage está vacía".into());
    }
    if trimmed.contains("untrusted comment:") {
        return Ok(trimmed.to_owned());
    }
    let decoded = BASE64
        .decode(trimmed.as_bytes())
        .map_err(|e| format!("firma del AppImage no válida: {e}"))?;
    String::from_utf8(decoded).map_err(|_| "la firma del AppImage no es texto UTF-8".into())
}

fn public_key() -> CmdResult<PublicKey> {
    // Tauri publica la clave como base64 del archivo `minisign.pub` completo
    // (comentario + línea de clave), no como la clave suelta.
    let decoded = BASE64
        .decode(LAUNCHER_PUBLIC_KEY.as_bytes())
        .map_err(|e| format!("la clave pública del launcher no es válida: {e}"))?;
    let text = String::from_utf8(decoded)
        .map_err(|_| "la clave pública del launcher no es texto UTF-8".to_owned())?;
    PublicKey::decode(&text).map_err(|e| format!("la clave pública del launcher no es válida: {e}"))
}

fn verify_bytes(bytes: &[u8], raw_signature: &str) -> CmdResult<()> {
    let signature = Signature::decode(&decode_signature(raw_signature)?)
        .map_err(|e| format!("firma del AppImage no válida: {e}"))?;
    public_key()?
        .verify(bytes, &signature, true)
        .map_err(|e| format!("la firma del AppImage no es válida: {e}"))
}

fn verify_file(path: &Path, raw_signature: &str) -> CmdResult<()> {
    let signature = Signature::decode(&decode_signature(raw_signature)?)
        .map_err(|e| format!("firma del AppImage no válida: {e}"))?;
    let key = public_key()?;
    let mut verifier = match key.verify_stream(&signature) {
        Ok(verifier) => verifier,
        // Las firmas antiguas no usan el modo pre-hasheado; en ese caso se
        // verifica el archivo completo en memoria.
        Err(minisign_verify::Error::UnsupportedLegacyMode) => {
            let bytes = fs::read(path).map_err(|e| format!("no se pudo leer el AppImage: {e}"))?;
            return key
                .verify(&bytes, &signature, true)
                .map_err(|e| format!("la firma del AppImage no es válida: {e}"));
        }
        Err(e) => return Err(format!("firma del AppImage no válida: {e}")),
    };

    let mut file = fs::File::open(path).map_err(|e| format!("no se pudo leer el AppImage: {e}"))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("no se pudo leer el AppImage: {e}"))?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
    }
    verifier
        .finalize()
        .map_err(|e| format!("la firma del AppImage no es válida: {e}"))
}

/// Carpeta donde vive el instalador en ejecución: si corre como AppImage, la
/// variable `APPIMAGE` que pone el propio runtime de AppImage apunta al
/// archivo `.AppImage` real (no al punto de montaje FUSE), así que de ahí se
/// saca el directorio. En `tauri dev` esa variable no existe: se usa el
/// directorio del ejecutable, útil solo para probar el flujo en desarrollo.
fn own_directory() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Busca un `Nebula-Launcher*.AppImage` firmado junto al instalador. `None` si
/// no está ahí (el frontend ofrece entonces elegirlo a mano).
#[tauri::command]
fn find_launcher_appimage() -> Option<SourceInfo> {
    let dir = own_directory()?;
    let entries = fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        let is_regular_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
        if lower.contains("nebula")
            && lower.contains("launcher")
            && is_appimage_name(&name)
            && is_regular_file
            && signature_path(&path).is_file()
        {
            let size_bytes = entry.metadata().ok()?.len();
            return Some(SourceInfo {
                path: path.to_string_lossy().into_owned(),
                size_bytes,
            });
        }
    }
    None
}

fn install_destination(home: &Path) -> PathBuf {
    home.join(".local/share/nebula-launcher")
}

/// Hace ejecutable el archivo para su propietario y grupo (equivalente a
/// `chmod u+x,g+x`), imprescindible para poder lanzar el AppImage después.
/// Se aplica sobre el descriptor abierto, no sobre una ruta que podría haberse
/// sustituido por un symlink.
fn make_executable(file: &fs::File) -> CmdResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = file
        .metadata()
        .map_err(|e| format!("no se pudo leer el archivo instalado: {e}"))?
        .permissions();
    perms.set_mode(perms.mode() | 0o110);
    file.set_permissions(perms)
        .map_err(|e| format!("no se pudo marcarlo ejecutable: {e}"))
}

/// Escribe un archivo pequeño mediante un temporal + `rename` atómico. El
/// `rename` sustituye el enlace en sí, no su destino, así que un symlink
/// preexistente no puede redirigir la escritura fuera del directorio.
fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> CmdResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "ruta de instalación no válida".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("no se pudo crear {}: {e}", parent.display()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("no se pudo preparar un archivo temporal: {e}"))?;
    tmp.write_all(bytes)
        .map_err(|e| format!("no se pudo escribir {}: {e}", path.display()))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| format!("no se pudo sincronizar {}: {e}", path.display()))?;
    tmp.persist(path)
        .map_err(|e| format!("no se pudo escribir {}: {}", path.display(), e.error))?;
    Ok(())
}

fn write_icon(dest_dir: &Path) -> CmdResult<PathBuf> {
    let icon_path = dest_dir.join("icon.png");
    write_atomic_bytes(&icon_path, ICON_BYTES)?;
    Ok(icon_path)
}

/// Escribe el `.desktop` que hace aparecer a Nebula en el menú de aplicaciones
/// (siguiendo la especificación freedesktop.org que usan KDE, GNOME, etc.).
fn write_desktop_entry(home: &Path, exec_path: &Path, icon_path: &Path) -> CmdResult<PathBuf> {
    let apps_dir = home.join(".local/share/applications");
    fs::create_dir_all(&apps_dir)
        .map_err(|e| format!("no se pudo preparar el menú de aplicaciones: {e}"))?;
    let desktop_path = apps_dir.join("nebula-launcher.desktop");
    let contents = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Nebula Launcher\n\
         Comment=Launcher de Minecraft\n\
         Exec=\"{}\" %U\n\
         Icon={}\n\
         Categories=Game;\n\
         Terminal=false\n",
        exec_path.display(),
        icon_path.display()
    );
    write_atomic_bytes(&desktop_path, contents.as_bytes())?;
    Ok(desktop_path)
}

/// Copia el AppImage a un temporal privado, lo verifica ya copiado y solo
/// entonces lo renombra al destino. Así un cambio del origen entre la
/// verificación y el copiado, o un symlink en el destino, no permite instalar
/// otro binario.
fn copy_verified_appimage(source: &Path, dest: &Path, signature: Option<&str>) -> CmdResult<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| "ruta de instalación no válida".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("no se pudo crear {}: {e}", parent.display()))?;

    let mut src =
        fs::File::open(source).map_err(|e| format!("no se pudo leer el AppImage: {e}"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("no se pudo preparar un archivo temporal: {e}"))?;
    std::io::copy(&mut src, tmp.as_file_mut())
        .map_err(|e| format!("no se pudo copiar el archivo: {e}"))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| format!("no se pudo sincronizar la copia: {e}"))?;

    if let Some(signature) = signature {
        verify_file(tmp.path(), signature)?;
    }
    // Se conservan los permisos del origen y se añade ejecución para el
    // propietario y el grupo, sin volver a tocar la ruta ya publicada.
    let perms = src
        .metadata()
        .map_err(|e| format!("no se pudo leer el AppImage: {e}"))?
        .permissions();
    tmp.as_file()
        .set_permissions(perms)
        .map_err(|e| format!("no se pudieron aplicar los permisos: {e}"))?;
    make_executable(tmp.as_file())?;

    tmp.persist(dest)
        .map_err(|e| format!("no se pudo copiar el archivo: {}", e.error))?;
    Ok(())
}

fn finish_install(source: &Path, signature: Option<&str>) -> CmdResult<InstallResult> {
    let home = home_dir()?;
    let dest_dir = install_destination(&home);
    let dest = dest_dir.join("Nebula-Launcher.AppImage");
    copy_verified_appimage(source, &dest, signature)?;

    let icon = write_icon(&dest_dir)?;
    let desktop_file = write_desktop_entry(&home, &dest, &icon)?;
    Ok(InstallResult {
        installed_path: dest.to_string_lossy().into_owned(),
        desktop_file: desktop_file.to_string_lossy().into_owned(),
    })
}

/// Instala a partir de un archivo que ya existe en el disco (el caso normal:
/// el AppImage encontrado junto al instalador). Exige su `.sig` al lado.
#[tauri::command]
fn install_from_path(source_path: String) -> CmdResult<InstallResult> {
    let source = PathBuf::from(&source_path);
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !is_appimage_name(&name) || !source.is_file() {
        return Err("el archivo elegido no es un .AppImage válido".into());
    }

    let sig_path = signature_path(&source);
    let sig_meta = fs::metadata(&sig_path).map_err(|_| {
        format!(
            "no se encontró la firma {}; descarga también el archivo .sig",
            sig_path.display()
        )
    })?;
    if !sig_meta.is_file() {
        return Err("la firma del AppImage no es un archivo válido".into());
    }
    if sig_meta.len() > MAX_SIGNATURE_BYTES as u64 {
        return Err("la firma del AppImage es demasiado grande".into());
    }
    let signature = fs::read_to_string(&sig_path).map_err(|_| {
        format!(
            "no se pudo leer la firma {}; descarga también el archivo .sig",
            sig_path.display()
        )
    })?;
    verify_file(&source, &signature)?;
    finish_install(&source, Some(&signature))
}

const MAX_UPLOAD_BYTES: usize = 500 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: usize = 16 * 1024;

/// Comprueba el tamaño codificado antes de decodificar: una cadena enorme no
/// debe llegar a reservarse en memoria. Separado para poder probarlo sin
/// asignar cientos de MB.
fn check_upload_size(encoded_len: usize) -> CmdResult<()> {
    let max_b64 = MAX_UPLOAD_BYTES.saturating_mul(4) / 3 + 4;
    if encoded_len > max_b64 {
        return Err("el archivo supera el tamaño máximo admitido".into());
    }
    Ok(())
}

/// Instala a partir de bytes recibidos del selector de archivos del navegador
/// (cuando no se encontró el AppImage automáticamente): el webview no expone
/// la ruta real de un archivo elegido a mano, así que llegan ya en memoria.
#[tauri::command]
fn install_from_bytes(
    file_name: String,
    data_base64: String,
    signature: String,
) -> CmdResult<InstallResult> {
    validate_appimage_name(&file_name)?;
    if signature.len() > MAX_SIGNATURE_BYTES {
        return Err("la firma del AppImage es demasiado grande".into());
    }

    check_upload_size(data_base64.len())?;
    let bytes = BASE64
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("archivo no válido: {e}"))?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("el archivo supera el tamaño máximo admitido".into());
    }
    verify_bytes(&bytes, &signature)?;

    // Directorio privado y aleatorio: no se reutiliza el nombre del cliente ni
    // una ruta fija en /tmp, así que no hay symlinks ni carreras entre usuarios.
    let tmp_dir = tempfile::Builder::new()
        .prefix("nebula-installer-")
        .tempdir()
        .map_err(|e| format!("no se pudo preparar una carpeta temporal: {e}"))?;
    let tmp_path = tmp_dir.path().join("payload.AppImage");
    fs::write(&tmp_path, &bytes)
        .map_err(|e| format!("no se pudo guardar el archivo elegido: {e}"))?;
    finish_install(&tmp_path, Some(&signature))
}

/// Lanza el Nebula Launcher ya instalado.
#[tauri::command]
fn open_installed(path: String) -> CmdResult<()> {
    let requested = PathBuf::from(path);
    let home = home_dir()?;

    let metadata = fs::symlink_metadata(&requested)
        .map_err(|_| "no se encontró el Nebula Launcher instalado".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("ruta no permitida".into());
    }

    let dest_dir = fs::canonicalize(install_destination(&home))
        .map_err(|_| "no se encontró el directorio de instalación".to_owned())?;
    let canonical = fs::canonicalize(&requested).map_err(|_| "ruta no permitida".to_owned())?;
    let expected = dest_dir.join("Nebula-Launcher.AppImage");
    if canonical != expected {
        return Err("ruta no permitida".into());
    }

    std::process::Command::new(&canonical)
        .spawn()
        .map_err(|e| format!("no se pudo abrir Nebula: {e}"))?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            find_launcher_appimage,
            install_from_path,
            install_from_bytes,
            open_installed,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("error al iniciar el instalador: {e}");
            std::process::exit(1);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `HOME` es global al proceso: serializa los tests que lo modifican.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nebula-installer-test-{}-{}",
            std::process::id(),
            fastrand_u64()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    // Generador mínimo sin dependencias nuevas: no necesita ser criptográfico,
    // solo distinguir directorios de distintos tests que corren en paralelo.
    fn fastrand_u64() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    #[test]
    fn full_install_flow_copies_marks_executable_and_writes_desktop_entry() {
        let home = temp_dir();
        let source_dir = temp_dir();
        let source = source_dir.join("Nebula-Launcher-0.1.0.AppImage");
        fs::write(&source, b"fake-appimage-bytes").expect("write source");

        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("HOME", &home);
        let result = finish_install(&source, None).expect("install");

        let dest = PathBuf::from(&result.installed_path);
        assert!(dest.is_file());
        assert_eq!(fs::read(&dest).unwrap_or_default(), b"fake-appimage-bytes");

        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&dest).expect("meta").permissions().mode();
        assert!(mode & 0o100 != 0, "debe ser ejecutable");

        let desktop = fs::read_to_string(&result.desktop_file).expect("desktop file");
        assert!(desktop.contains("Name=Nebula Launcher"));
        assert!(desktop.contains(&dest.display().to_string()));
        assert!(PathBuf::from(&result.desktop_file)
            .parent()
            .expect("parent")
            .join("..")
            .exists());
    }

    #[test]
    fn install_from_path_rejects_non_appimage_and_missing_files() {
        let dir = temp_dir();
        let not_appimage = dir.join("evil.sh");
        fs::write(&not_appimage, b"x").expect("write");
        assert!(install_from_path(not_appimage.to_string_lossy().into_owned()).is_err());
        assert!(
            install_from_path(dir.join("missing.AppImage").to_string_lossy().into_owned()).is_err()
        );
    }

    #[test]
    fn install_from_bytes_rejects_wrong_extension_and_oversize() {
        let err = install_from_bytes("script.sh".into(), "AAAA".into(), String::new());
        assert!(err.is_err());
        // El tope se prueba sobre la longitud, sin asignar cientos de MB.
        let max_b64 = MAX_UPLOAD_BYTES.saturating_mul(4) / 3 + 4;
        assert!(check_upload_size(max_b64).is_ok());
        assert!(check_upload_size(max_b64 + 1).is_err());
        // Un nombre con separadores tampoco debe servir para escapar de /tmp.
        let err = install_from_bytes("../evil.AppImage".into(), "AAAA".into(), String::new());
        assert!(err.is_err());
    }

    #[test]
    fn open_installed_rejects_paths_outside_the_install_directory() {
        let home = temp_dir();
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("HOME", &home);
        let outside = home.join(".local/share/nebula-launcher/../../../../bin/sh");
        assert!(open_installed(outside.to_string_lossy().into_owned()).is_err());
        assert!(open_installed("/bin/sh".into()).is_err());
    }

    #[test]
    fn signature_rejects_tampered_data() {
        let err = verify_bytes(b"no es un appimage", "no es una firma");
        assert!(err.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_writes_replace_symlinks_instead_of_following_them() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir();
        let target = dir.join("target.txt");
        fs::write(&target, b"original").expect("target");
        let link = dir.join("link.txt");
        symlink(&target, &link).expect("symlink");

        write_atomic_bytes(&link, b"nuevo").expect("write");
        assert_eq!(fs::read(&target).expect("target"), b"original");
        assert_eq!(fs::read(&link).expect("link"), b"nuevo");
        assert!(!fs::symlink_metadata(&link)
            .expect("metadata")
            .file_type()
            .is_symlink());
    }

    /// Prueba manual contra un AppImage real ya firmado:
    /// `NEBULA_TEST_APPIMAGE=/ruta/Nebula-Launcher.AppImage cargo test`.
    #[test]
    fn verifies_a_real_signed_appimage_when_requested() {
        let Some(path) = std::env::var_os("NEBULA_TEST_APPIMAGE") else {
            return;
        };
        let path = PathBuf::from(path);
        let signature = fs::read_to_string(signature_path(&path)).expect("leer .sig");
        verify_file(&path, &signature).expect("la firma real debe ser válida");
    }
}
