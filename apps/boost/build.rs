// Ressources Windows de plume-boost.exe : icone, informations de version, manifeste (partage avec le navigateur).
// Le manifeste reste "asInvoker" : Plume Boost ne demande les droits d'administrateur que quand l'utilisateur le veut.
include!("../../tools/icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand/plume-boost.ico"), "Plume Boost", "plume-boost.exe");
}
