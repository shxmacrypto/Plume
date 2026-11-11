// Intègre l'icône, et recompile l'installeur quand l'un des programmes qu'il porte change.
include!("../tools/icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/brand/plume-suite.ico"), "Plume Suite Setup", "PlumeSetup.exe");
    // Plume Mail n'est dans l'installeur que si PLUME_WITH_MAIL le demande (il attend la connexion Microsoft)
    println!("cargo::rustc-check-cfg=cfg(plume_mail)");
    println!("cargo:rerun-if-env-changed=PLUME_WITH_MAIL");
    let mail = std::env::var("PLUME_WITH_MAIL").is_ok_and(|v| v == "1");
    if mail {
        println!("cargo:rustc-cfg=plume_mail");
    }
    let mut vars = vec!["PLUME_SUITE_EXE", "PLUME_CORE_EXE", "PLUME_LOADER_DLL", "PLUME_FEX_EXE"];
    if mail {
        vars.push("PLUME_MAIL_EXE");
    }
    for var in vars {
        println!("cargo:rerun-if-env-changed={var}");
        let path = std::env::var(var).unwrap_or_else(|_| panic!("{var} manquant (utiliser build.ps1)"));
        println!("cargo:rerun-if-changed={path}");
    }
}
