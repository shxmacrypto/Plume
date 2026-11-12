// Ressources Windows de plume-notepad.exe : icône, informations de version, manifeste (partagé avec le navigateur).
include!("../../tools/icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand/plume-notepad.ico"), "Plume Notepad", "plume-notepad.exe");
}
