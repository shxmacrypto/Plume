// Partagé par les build.rs : intègre assets/plume.ico comme ressource n°1 de l'exe
// (icône dans l'Explorateur, la barre des tâches et Alt+Tab). windres vient des binutils MSYS2.
pub fn embed_icon(ico: &std::path::Path) {
    println!("cargo:rerun-if-changed={}", ico.display());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("icon.rc");
    let pp = out.join("pp.bat");
    let obj = out.join("icon.o");
    std::fs::write(&rc, format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "/"))).unwrap();
    std::fs::write(&pp, "@type %*\r\n").unwrap(); // pas de gcc : préprocesseur « identité »
    let ok = std::process::Command::new("windres")
        .arg(format!("--preprocessor={}", pp.display()))
        .args(["-O", "coff", "-i"]).arg(&rc).arg("-o").arg(&obj)
        .status().is_ok_and(|s| s.success());
    if ok {
        println!("cargo:rustc-link-arg={}", obj.display());
    } else {
        println!("cargo:warning=windres introuvable : exe sans icône");
    }
}
