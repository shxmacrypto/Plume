// Calcule un identifiant de version à partir du binaire embarqué, pour décompresser
// chaque version dans son propre dossier (jamais de conflit avec une copie en cours d'exécution).
include!("../icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/plume.ico"));
    println!("cargo:rerun-if-env-changed=PLUME_CORE_EXE");
    println!("cargo:rerun-if-env-changed=PLUME_LOADER_DLL");
    let core = std::env::var("PLUME_CORE_EXE").expect("PLUME_CORE_EXE manquant (utiliser build.ps1)");
    let dll = std::env::var("PLUME_LOADER_DLL").expect("PLUME_LOADER_DLL manquant (utiliser build.ps1)");
    println!("cargo:rerun-if-changed={core}");
    println!("cargo:rerun-if-changed={dll}");
    let bytes = std::fs::read(&core).expect("lecture du binaire Plume");
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a
    for b in bytes {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    println!("cargo:rustc-env=PLUME_BUILD_ID={h:016x}");
}
