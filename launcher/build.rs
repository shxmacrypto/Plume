// Intègre l'icône, et recompile l'installeur quand le navigateur ou WebView2Loader.dll changent.
include!("../icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/plume.ico"), "Plume Setup", "PlumeSetup.exe");
    println!("cargo:rerun-if-env-changed=PLUME_CORE_EXE");
    println!("cargo:rerun-if-env-changed=PLUME_LOADER_DLL");
    let core = std::env::var("PLUME_CORE_EXE").expect("PLUME_CORE_EXE manquant (utiliser build.ps1)");
    let dll = std::env::var("PLUME_LOADER_DLL").expect("PLUME_LOADER_DLL manquant (utiliser build.ps1)");
    println!("cargo:rerun-if-changed={core}");
    println!("cargo:rerun-if-changed={dll}");
}
