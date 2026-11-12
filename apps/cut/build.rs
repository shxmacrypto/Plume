// Windows resources of plume-cut.exe: icon, version information, manifest (shared with the other apps).
include!("../../tools/icon_res.rs");
fn main() {
    embed_icon(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand/plume-cut.ico"), "Plume Cut", "plume-cut.exe");
}
