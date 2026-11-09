# wry 0.53.5 (copie locale pour Plume)

Copie de [wry](https://github.com/tauri-apps/wry) 0.53.5 (MIT / Apache-2.0), avec deux modifications :

- `WebViewBuilderExtWindows::with_profile_name()` règle `ICoreWebView2ControllerOptions::ProfileName`,
  ce qui permet les conteneurs de Plume (cookies et stockage séparés dans le même processus WebView2).
- Messages IPC des pages (`src/webview2/mod.rs`, `attach_ipc_handler`) : `http::Uri` refuse les adresses
  `file:///` (hôte vide), `about:` ou `data:`, et l'`unwrap()` d'origine faisait planter Plume dès qu'une page
  locale envoyait un message. L'adresse devient `file://localhost/…` ou `<schéma>://unparsed/`.

Branchée par `[patch.crates-io]` dans le Cargo.toml de Plume. Pour mettre wry à jour : recopier la
nouvelle version ici et réappliquer les blocs commentés « Plume » dans `src/lib.rs` et `src/webview2/mod.rs`.
