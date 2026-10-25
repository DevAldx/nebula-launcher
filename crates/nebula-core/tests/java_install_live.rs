//! Prueba manual de red: descarga un JRE real de Adoptium y comprueba que
//! arranca. No forma parte de la suite normal; se ejecuta a mano con
//! `cargo test -p nebula-core --test java_install_live -- --ignored --nocapture`.

use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use nebula_core::{
    java::{install_major, JavaRuntime},
    minecraft::Reporter,
    NebulaPaths,
};

#[tokio::test]
#[ignore = "requiere red y descarga ~50 MB; ejecutar a mano"]
async fn downloads_a_real_temurin_jre_and_runs_it() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("nebula-java-live-{stamp}"));
    let paths = NebulaPaths::new(root.join("config"), root.join("data"));

    let result: Result<JavaRuntime, _> =
        install_major(&paths, 21, &Reporter::silent()).await;

    let runtime = match result {
        Ok(rt) => rt,
        Err(e) => {
            let _ = fs::remove_dir_all(&root);
            panic!("la instalación real de Java 21 falló: {e}");
        }
    };

    println!(
        "Java instalado: {} (major {}, {})",
        runtime.path.display(),
        runtime.major,
        runtime.version
    );
    assert_eq!(runtime.major, 21);
    assert!(runtime.path.is_file(), "no existe {}", runtime.path.display());
    assert!(
        runtime.path.starts_with(paths.java_dir()),
        "el JRE no quedó dentro de data/java: {}",
        runtime.path.display()
    );

    let _ = fs::remove_dir_all(&root);
}
