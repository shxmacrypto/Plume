// Ressources Windows de plume-suite.exe : icône, informations de version, manifeste (partagé avec le navigateur).
include!("../../icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand/plume-suite.ico"), "Plume Suite", "plume-suite.exe");
}
