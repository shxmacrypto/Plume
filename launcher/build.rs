// Intègre l'icône, et recompile l'installeur quand l'un des programmes qu'il porte change.
include!("../icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/brand/plume-suite.ico"), "Plume Suite Setup", "PlumeSetup.exe");
    for var in ["PLUME_SUITE_EXE", "PLUME_CORE_EXE", "PLUME_LOADER_DLL", "PLUME_FEX_EXE", "PLUME_MAIL_EXE"] {
        println!("cargo:rerun-if-env-changed={var}");
        let path = std::env::var(var).unwrap_or_else(|_| panic!("{var} manquant (utiliser build.ps1)"));
        println!("cargo:rerun-if-changed={path}");
    }
}
