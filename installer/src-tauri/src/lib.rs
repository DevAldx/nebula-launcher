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
    io::Write,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;

/// Icono embebido en el propio binario: así el instalador no depende de que
/// el AppImage del launcher ya esté extraído para tener algo que mostrar.
const ICON_BYTES: &[u8] = include_bytes!("../icons/nebula.png");

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

/// Busca un `Nebula-Launcher*.AppImage` junto al instalador. `None` si no
/// está ahí (el frontend ofrece entonces elegirlo a mano).
#[tauri::command]
fn find_launcher_appimage() -> Option<SourceInfo> {
    let dir = own_directory()?;
    let entries = fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if lower.contains("nebula") && lower.contains("launcher") && is_appimage_name(&name) {
            let size_bytes = entry.metadata().ok()?.len();
            return Some(SourceInfo {
                path: entry.path().to_string_lossy().into_owned(),
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
fn make_executable(path: &Path) -> CmdResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("no se pudo leer el archivo instalado: {e}"))?
        .permissions();
    perms.set_mode(perms.mode() | 0o110);
    fs::set_permissions(path, perms).map_err(|e| format!("no se pudo marcarlo ejecutable: {e}"))
}

fn write_icon(dest_dir: &Path) -> CmdResult<PathBuf> {
    let icon_path = dest_dir.join("icon.png");
    fs::write(&icon_path, ICON_BYTES).map_err(|e| format!("no se pudo escribir el icono: {e}"))?;
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
    let mut file = fs::File::create(&desktop_path)
        .map_err(|e| format!("no se pudo crear el acceso del menú: {e}"))?;
    file.write_all(contents.as_bytes())
        .map_err(|e| format!("no se pudo escribir el acceso del menú: {e}"))?;
    Ok(desktop_path)
}

fn finish_install(source: &Path) -> CmdResult<InstallResult> {
    let home = home_dir()?;
    let dest_dir = install_destination(&home);
    fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("no se pudo crear {}: {e}", dest_dir.display()))?;
    let dest = dest_dir.join("Nebula-Launcher.AppImage");
    fs::copy(source, &dest).map_err(|e| format!("no se pudo copiar el archivo: {e}"))?;
    make_executable(&dest)?;
    let icon = write_icon(&dest_dir)?;
    let desktop_file = write_desktop_entry(&home, &dest, &icon)?;
    Ok(InstallResult {
        installed_path: dest.to_string_lossy().into_owned(),
        desktop_file: desktop_file.to_string_lossy().into_owned(),
    })
}

/// Instala a partir de un archivo que ya existe en el disco (el caso normal:
/// el AppImage encontrado junto al instalador).
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
    finish_install(&source)
}

const MAX_UPLOAD_BYTES: usize = 500 * 1024 * 1024;

/// Instala a partir de bytes recibidos del selector de archivos del navegador
/// (cuando no se encontró el AppImage automáticamente): el webview no expone
/// la ruta real de un archivo elegido a mano, así que llegan ya en memoria.
#[tauri::command]
fn install_from_bytes(file_name: String, data_base64: String) -> CmdResult<InstallResult> {
    if !is_appimage_name(&file_name) {
        return Err("solo se admiten archivos .AppImage".into());
    }
    let bytes = BASE64
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("archivo no válido: {e}"))?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("el archivo supera el tamaño máximo admitido".into());
    }
    let tmp_dir = std::env::temp_dir().join("nebula-installer");
    fs::create_dir_all(&tmp_dir).map_err(|e| format!("no se pudo preparar una carpeta temporal: {e}"))?;
    let tmp_path = tmp_dir.join(&file_name);
    fs::write(&tmp_path, &bytes).map_err(|e| format!("no se pudo guardar el archivo elegido: {e}"))?;
    let result = finish_install(&tmp_path);
    let _ = fs::remove_file(&tmp_path); // limpieza best-effort, no es un error si falla
    result
}

/// Lanza el Nebula Launcher ya instalado.
#[tauri::command]
fn open_installed(path: String) -> CmdResult<()> {
    let path = PathBuf::from(path);
    let home = home_dir()?;
    if !path.starts_with(install_destination(&home)) {
        return Err("ruta no permitida".into());
    }
    std::process::Command::new(&path)
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

        std::env::set_var("HOME", &home);
        let result = finish_install(&source).expect("install");

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
        assert!(install_from_path(dir.join("missing.AppImage").to_string_lossy().into_owned()).is_err());
    }

    #[test]
    fn install_from_bytes_rejects_wrong_extension_and_oversize() {
        let err = install_from_bytes("script.sh".into(), "AAAA".into());
        assert!(err.is_err());
        let huge = "A".repeat(MAX_UPLOAD_BYTES + 10);
        let err = install_from_bytes("big.AppImage".into(), huge);
        assert!(err.is_err());
    }
}
