// Partagé par les build.rs : ressources Windows de l'exe — icône (Explorateur, barre des tâches, Alt+Tab),
// informations de version (Propriétés › Détails : nom, éditeur, version) et manifeste (droits utilisateur,
// Windows 10/11, contrôles modernes). windres vient des binutils MSYS2.
pub fn embed_icon(ico: &std::path::Path, description: &str, file_name: &str) {
    println!("cargo:rerun-if-changed={}", ico.display());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ver = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let n: Vec<u16> = ver.split(['.', '-']).take(3).map(|x| x.parse().unwrap_or(0)).collect();
    let (a, b, c) = (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0));
    let manifest = out.join("app.manifest");
    std::fs::write(
        &manifest,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="Plume" version="1.0.0.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application>
  </compatibility>
  <dependency><dependentAssembly>
    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
</assembly>
"#,
    )
    .unwrap();
    let path = |p: &std::path::Path| p.display().to_string().replace('\\', "/");
    let rc = out.join("icon.rc");
    let pp = out.join("pp.bat");
    let obj = out.join("icon.o");
    std::fs::write(
        &rc,
        format!(
            r#"1 ICON "{ico}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
FILEFLAGSMASK 0x3f
FILEFLAGS 0x0
FILEOS 0x40004
FILETYPE 0x1
FILESUBTYPE 0x0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "shxmacrypto"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{ver}"
      VALUE "InternalName", "{file_name}"
      VALUE "LegalCopyright", "Copyright (c) 2026 shxmacrypto. MIT License."
      VALUE "OriginalFilename", "{file_name}"
      VALUE "ProductName", "Plume"
      VALUE "ProductVersion", "{ver}"
      VALUE "Comments", "Open source: https://github.com/shxmacrypto/Plume"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
            ico = path(ico),
            manifest = path(&manifest),
        ),
    )
    .unwrap();
    std::fs::write(&pp, "@type %*\r\n").unwrap(); // pas de gcc : préprocesseur « identité »
    let ok = std::process::Command::new("windres")
        .arg(format!("--preprocessor={}", pp.display()))
        .args(["-O", "coff", "-i"]).arg(&rc).arg("-o").arg(&obj)
        .status().is_ok_and(|s| s.success());
    if ok {
        println!("cargo:rustc-link-arg={}", obj.display());
    } else {
        println!("cargo:warning=windres a échoué : exe sans icône ni informations de version");
    }
}
