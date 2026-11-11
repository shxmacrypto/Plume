//! Archives : extraire (zip, 7z, rar, tar, gz, xz, zst, cab, iso…), compresser en zip, protéger par mot de passe.
//!
//! - Sans mot de passe, le travail est fait par `tar.exe`, livré avec Windows depuis Windows 10 1803 (c'est bsdtar,
//!   de libarchive) : rien à installer, rien à embarquer.
//! - Les ZIP et 7z protégés (AES-256) sont lus et écrits ici même : le mot de passe ne passe par aucune ligne de
//!   commande.
//! - Les RAR protégés passent par WinRAR s'il est installé (`Rar.exe`, `UnRAR.exe`) : le format RAR appartient à
//!   son auteur, lui seul sait en créer. Le mot de passe leur est donné sur leur entrée standard.
//!
//! Une extraction ne sort jamais du dossier choisi (chemins « .. » refusés), et un échec ne laisse rien derrière lui.

use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime},
};
use windows::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    System::Time::{FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime, TzSpecificLocalTimeToSystemTime},
};
use zip::{AesMode, CompressionMethod, ZipArchive, ZipWriter, result::ZipError, write::SimpleFileOptions};

/// Pas de fenêtre de console pour les outils.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Centaines de nanosecondes entre 1601 (origine des dates de Windows) et 1970.
const EPOCH_GAP: u64 = 116_444_736_000_000_000;

/// Extensions doubles : « sauvegarde.tar.gz » s'extrait vers « sauvegarde ».
const DOUBLE: [&str; 5] = [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tar.lz4"];
const SINGLE: [&str; 18] = ["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz2", "tbz", "xz", "txz", "zst", "tzst", "cab", "iso", "lzh", "lha", "jar"];

/// Format d'une archive protégée par mot de passe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// ZIP chiffré en AES-256 : s'ouvre dans WinRAR, 7-Zip, WinZip et Windows 11.
    Zip,
    /// 7z chiffré en AES-256, noms de fichiers cachés : s'ouvre dans 7-Zip, WinRAR et NanaZip.
    SevenZ,
    /// RAR, noms de fichiers cachés. Demande WinRAR.
    Rar,
}

/// Pourquoi une extraction n'a pas abouti.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// L'archive est protégée : il faut demander le mot de passe.
    NeedPassword,
    WrongPassword,
    /// L'archive est protégée dans un format que seul WinRAR sait ouvrir, et il n'est pas installé.
    NeedWinRar,
    Message(String),
}

impl From<io::Error> for Failure {
    fn from(e: io::Error) -> Self {
        Failure::Message(e.to_string())
    }
}

/// Ce nom est-il celui d'une archive que l'on sait extraire ?
pub fn is_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match lower.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && SINGLE.contains(&ext),
        None => false,
    }
}

/// Nom du dossier d'extraction : le nom de l'archive sans son extension (double comprise).
pub fn stem(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    if let Some(d) = DOUBLE.iter().find(|d| lower.ends_with(**d)) {
        return &name[..name.len() - d.len()];
    }
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

// ───────────────────────── outils ─────────────────────────

fn tar() -> Result<PathBuf, String> {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let exe = root.join("System32").join("tar.exe");
    if exe.is_file() { Ok(exe) } else { Err("tar.exe is missing (it comes with Windows 10 version 1803 and later).".to_string()) }
}

/// Les outils en ligne de commande de WinRAR, s'il est installé : (Rar.exe, UnRAR.exe).
pub fn winrar() -> Option<(PathBuf, PathBuf)> {
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).map(|p| PathBuf::from(p).join("WinRAR")).find_map(|dir| {
        let (rar, unrar) = (dir.join("Rar.exe"), dir.join("UnRAR.exe"));
        (rar.is_file() && unrar.is_file()).then_some((rar, unrar))
    })
}

/// Lance un outil sans fenêtre et renvoie son code de sortie et ce qu'il a écrit sur sa sortie d'erreur.
/// `input` est écrit sur son entrée standard (un mot de passe).
fn run(cmd: &mut Command, input: Option<&str>) -> io::Result<(i32, String)> {
    cmd.creation_flags(CREATE_NO_WINDOW).stdout(Stdio::null()).stderr(Stdio::piped());
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd.spawn()?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
        let _ = stdin.write_all(b"\n");
    }
    let out = child.wait_with_output()?;
    Ok((out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).into_owned()))
}

/// Dernière ligne utile de ce que tar.exe a répondu (« Unrecognized archive format »…).
fn tar_message(stderr: &str) -> String {
    let line = stderr.lines().rev().map(str::trim).find(|l| !l.is_empty() && !l.contains("Error exit delayed")).unwrap_or("tar.exe failed");
    line.trim_start_matches("tar.exe: ").trim_start_matches("tar: ").to_string()
}

fn tar_extract(archive: &Path, dest: &Path) -> Result<(), String> {
    let (code, stderr) = run(Command::new(tar()?).arg("-xf").arg(archive).arg("-C").arg(dest), None).map_err(|e| e.to_string())?;
    if code == 0 { Ok(()) } else { Err(tar_message(&stderr)) }
}

// ───────────────────────── dates ─────────────────────────

/// Date d'un fichier → date d'une entrée ZIP (heure locale, de 1980 à 2107).
fn zip_time(t: SystemTime) -> Option<zip::DateTime> {
    let ft = t.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_nanos() as u64 / 100 + EPOCH_GAP;
    let f = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
    let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        FileTimeToSystemTime(&f, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
    }
    zip::DateTime::from_date_and_time(local.wYear, local.wMonth as u8, local.wDay as u8, local.wHour as u8, local.wMinute as u8, local.wSecond as u8).ok()
}

/// Date d'une entrée ZIP → date à donner au fichier extrait.
fn file_time(d: zip::DateTime) -> Option<SystemTime> {
    let local = SYSTEMTIME {
        wYear: d.year(),
        wMonth: d.month() as u16,
        wDay: d.day() as u16,
        wHour: d.hour() as u16,
        wMinute: d.minute() as u16,
        wSecond: d.second() as u16,
        ..Default::default()
    };
    let (mut utc, mut f) = (SYSTEMTIME::default(), FILETIME::default());
    unsafe {
        TzSpecificLocalTimeToSystemTime(None, &local, &mut utc).ok()?;
        SystemTimeToFileTime(&utc, &mut f).ok()?;
    }
    let ft = ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    Some(SystemTime::UNIX_EPOCH + Duration::from_nanos(ft.checked_sub(EPOCH_GAP)? * 100))
}

// ───────────────────────── extraire ─────────────────────────

#[derive(PartialEq)]
enum Kind {
    Zip,
    SevenZ,
    Rar,
    Other,
}

fn kind(archive: &Path) -> Kind {
    let mut head = [0u8; 8];
    let n = File::open(archive).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
    match &head[..n] {
        [b'P', b'K', 3, 4, ..] | [b'P', b'K', 5, 6, ..] => Kind::Zip,
        [b'R', b'a', b'r', b'!', 0x1a, 7, ..] => Kind::Rar,
        [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C, ..] => Kind::SevenZ,
        _ => Kind::Other,
    }
}

/// ZIP : sans chiffrement, bsdtar s'en charge (il connaît plus de méthodes de compression) ; chiffré, on le lit ici.
fn extract_zip(archive: &Path, dest: &Path, password: Option<&str>) -> Result<(), Failure> {
    let open = || -> Result<ZipArchive<BufReader<File>>, Failure> {
        ZipArchive::new(BufReader::new(File::open(archive)?)).map_err(|e| Failure::Message(e.to_string()))
    };
    let mut zip = match open() {
        Ok(z) => z,
        // une archive que cette bibliothèque ne comprend pas : bsdtar dira ce qu'il en est
        Err(_) => return tar_extract(archive, dest).map_err(Failure::Message),
    };
    let encrypted = (0..zip.len()).any(|i| zip.by_index_raw(i).is_ok_and(|f| f.encrypted()));
    if !encrypted {
        return tar_extract(archive, dest).map_err(Failure::Message);
    }
    let Some(password) = password else { return Err(Failure::NeedPassword) };
    for i in 0..zip.len() {
        let mut entry = match zip.by_index_decrypt(i, password.as_bytes()) {
            Ok(f) => f,
            Err(ZipError::InvalidPassword) => return Err(Failure::WrongPassword),
            Err(e) => return Err(Failure::Message(e.to_string())),
        };
        // `enclosed_name` refuse les chemins absolus et ceux qui remontent par « .. »
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&out)?;
        // le contenu est vérifié à la fin de la lecture : une erreur ici, c'est le plus souvent un mauvais mot de passe
        if let Err(e) = io::copy(&mut entry, &mut file) {
            let text = e.to_string().to_lowercase();
            return Err(if text.contains("auth") || text.contains("checksum") || text.contains("crc") || text.contains("corrupt") {
                Failure::WrongPassword
            } else {
                e.into()
            });
        }
        if let Some(t) = entry.last_modified().and_then(file_time) {
            let _ = file.set_modified(t);
        }
    }
    Ok(())
}

/// 7z : sans chiffrement, bsdtar s'en charge ; chiffré (contenu, ou noms aussi), on le lit ici.
fn extract_7z(archive: &Path, dest: &Path, password: Option<&str>) -> Result<(), Failure> {
    use sevenz_rust2::{ArchiveReader, Error as E, Password};
    let open = |pw: Password| -> Result<ArchiveReader<BufReader<File>>, E> { ArchiveReader::new(BufReader::new(File::open(archive)?), pw) };
    let pw = password.map(Password::from).unwrap_or_else(Password::empty);
    let encrypted = match open(pw.clone()) {
        Ok(r) => (0..r.archive().files.len()).any(|i| r.archive().is_file_encrypted(i)),
        // les noms eux-mêmes sont chiffrés
        Err(E::PasswordRequired) => return Err(if password.is_some() { Failure::WrongPassword } else { Failure::NeedPassword }),
        // un mot de passe a été donné et l'en-tête ne se lit pas : sans mot de passe, il en demanderait un ?
        Err(_) if password.is_some() && matches!(open(Password::empty()), Err(E::PasswordRequired)) => return Err(Failure::WrongPassword),
        // une archive que cette bibliothèque ne comprend pas : bsdtar dira ce qu'il en est
        Err(_) => return tar_extract(archive, dest).map_err(Failure::Message),
    };
    if !encrypted {
        return tar_extract(archive, dest).map_err(Failure::Message);
    }
    if password.is_none() {
        return Err(Failure::NeedPassword);
    }
    // les chemins qui sortiraient du dossier (« .. », absolus) sont refusés par la bibliothèque
    match sevenz_rust2::decompress_file_with_password(archive, dest, pw) {
        Ok(()) => Ok(()),
        Err(E::MaybeBadPassword(_) | E::ChecksumVerificationFailed | E::PasswordRequired) => Err(Failure::WrongPassword),
        Err(e) => Err(Failure::Message(e.to_string())),
    }
}

/// RAR : bsdtar lit ceux qui ne sont pas protégés ; pour les autres il faut UnRAR, qui reçoit le mot de passe sur
/// son entrée standard (ou en argument s'il contient autre chose que de l'ASCII, que l'entrée standard déforme).
fn extract_rar(archive: &Path, dest: &Path, password: Option<&str>) -> Result<(), Failure> {
    let Some(password) = password else {
        return match tar_extract(archive, dest) {
            Ok(()) => Ok(()),
            Err(msg) if msg.to_lowercase().contains("ncrypt") => Err(if winrar().is_some() { Failure::NeedPassword } else { Failure::NeedWinRar }),
            Err(msg) => Err(Failure::Message(msg)),
        };
    };
    let Some((_, unrar)) = winrar() else { return Err(Failure::NeedWinRar) };
    let mut target = dest.as_os_str().to_os_string();
    target.push("\\");
    let mut cmd = Command::new(unrar);
    cmd.args(["x", "-idq", "-y", "-o+"]);
    if !password.is_ascii() {
        cmd.arg(format!("-p{password}"));
    }
    cmd.arg(archive).arg(target);
    match run(&mut cmd, password.is_ascii().then_some(password))? {
        (0, _) => Ok(()),
        // 11 : mot de passe incorrect ; 12 : UnRAR l'a redemandé sur son entrée standard, où il n'y avait plus rien
        (11, _) | (12, _) => Err(Failure::WrongPassword),
        // 3 : somme de contrôle fausse. Archive abîmée, ou mauvais mot de passe sur un RAR d'ancien format :
        // on ne peut pas trancher, et redemander sans fin le mot de passe d'une archive abîmée n'aiderait personne
        (3, _) => Err(Failure::Message("The archive is damaged, or the password is wrong.".to_string())),
        (code, _) => Err(Failure::Message(format!("WinRAR could not extract this archive (error {code})."))),
    }
}

fn extract_any(archive: &Path, dest: &Path, password: Option<&str>) -> Result<(), Failure> {
    match kind(archive) {
        Kind::Zip => extract_zip(archive, dest, password),
        Kind::Rar => extract_rar(archive, dest, password),
        Kind::SevenZ => extract_7z(archive, dest, password),
        Kind::Other => tar_extract(archive, dest).map_err(|msg| {
            let low = msg.to_lowercase();
            if low.contains("ncrypt") || low.contains("passphrase") {
                Failure::Message("This kind of password-protected archive is not supported yet.".to_string())
            } else {
                Failure::Message(msg)
            }
        }),
    }
}

/// Extrait `archive` dans un dossier neuf, `dest`, qui ne doit pas exister. En cas d'échec il est retiré.
pub fn extract_new(archive: &Path, dest: &Path, password: Option<&str>) -> Result<(), Failure> {
    std::fs::create_dir(dest)?;
    let r = extract_any(archive, dest, password);
    if r.is_err() {
        // ce dossier vient d'être créé ici : tout ce qu'il contient vient de l'extraction manquée
        let _ = std::fs::remove_dir_all(dest);
    }
    r
}

/// Range le contenu de `from` dans `to` sans rien écraser : ce qui existe déjà reste tel quel.
fn merge(from: &Path, to: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if !target.exists() {
            std::fs::rename(entry.path(), &target)?;
        } else if target.is_dir() && entry.file_type()?.is_dir() {
            merge(&entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Extrait `archive` directement dans le dossier `dir`, sans écraser ce qui s'y trouve. L'extraction se fait
/// d'abord dans un dossier provisoire : si elle échoue (mauvais mot de passe, archive abîmée), rien n'est ajouté.
pub fn extract_here(archive: &Path, dir: &Path, password: Option<&str>) -> Result<(), Failure> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let tmp = dir.join(format!(".plume-extract-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir(&tmp)?;
    let r = extract_any(archive, &tmp, password).and_then(|_| merge(&tmp, dir).map_err(Failure::from));
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

// ───────────────────────── compresser ─────────────────────────

/// Compresse les éléments `names` du dossier `dir` dans `out`, sans mot de passe ; le format suit l'extension
/// (.zip, .7z, .tar.gz).
pub fn compress(out: &Path, dir: &Path, names: &[String]) -> Result<(), String> {
    let mut cmd = Command::new(tar()?);
    cmd.arg("-a").arg("-cf").arg(out).arg("-C").arg(dir).arg("--");
    for n in names {
        // « @fichier » et « -x » auraient un sens particulier pour l'outil
        if n.starts_with('@') || n.starts_with('-') {
            cmd.arg(format!(".\\{n}"))
        } else {
            cmd.arg(n)
        };
    }
    let r = match run(&mut cmd, None) {
        Ok((0, _)) => Ok(()),
        Ok((_, stderr)) => Err(tar_message(&stderr)),
        Err(e) => Err(e.to_string()),
    };
    if r.is_err() {
        let _ = std::fs::remove_file(out);
    }
    r
}

/// Ajoute un fichier ou un dossier (avec tout ce qu'il contient) à un ZIP chiffré. `rel` : son chemin dans l'archive.
fn zip_add(zip: &mut ZipWriter<BufWriter<File>>, path: &Path, rel: &str, password: &str) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    if let Some(t) = meta.modified().ok().and_then(zip_time) {
        options = options.last_modified_time(t);
    }
    if meta.is_dir() {
        zip.add_directory(format!("{rel}/"), options).map_err(|e| e.to_string())?;
        // on ne suit pas les liens et les jonctions : ils pourraient mener n'importe où, voire tourner en rond
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        let mut children: Vec<_> = std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?.filter_map(Result::ok).collect();
        children.sort_by_key(|c| c.file_name());
        for child in children {
            zip_add(zip, &child.path(), &format!("{rel}/{}", child.file_name().to_string_lossy()), password)?;
        }
        return Ok(());
    }
    let options = options.large_file(meta.len() >= u32::MAX as u64).with_aes_encryption(AesMode::Aes256, password);
    zip.start_file(rel, options).map_err(|e| e.to_string())?;
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    io::copy(&mut file, zip).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn zip_locked(out: &Path, dir: &Path, names: &[String], password: &str) -> Result<(), String> {
    let mut zip = ZipWriter::new(BufWriter::new(File::create(out).map_err(|e| e.to_string())?));
    for n in names {
        zip_add(&mut zip, &dir.join(n), n, password)?;
    }
    zip.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())
}

/// Ajoute un fichier ou un dossier (avec tout ce qu'il contient) à un 7z chiffré. `rel` : son chemin dans l'archive.
fn seven_add<W: Write + io::Seek>(sz: &mut sevenz_rust2::ArchiveWriter<W>, path: &Path, rel: &str) -> Result<(), String> {
    use sevenz_rust2::ArchiveEntry;
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_dir() {
        sz.push_archive_entry::<File>(ArchiveEntry::from_path(path, rel.to_string()), None).map_err(|e| e.to_string())?;
        // on ne suit pas les liens et les jonctions : ils pourraient mener n'importe où, voire tourner en rond
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        let mut children: Vec<_> = std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?.filter_map(Result::ok).collect();
        children.sort_by_key(|c| c.file_name());
        for child in children {
            seven_add(sz, &child.path(), &format!("{rel}/{}", child.file_name().to_string_lossy()))?;
        }
        return Ok(());
    }
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    sz.push_archive_entry(ArchiveEntry::from_path(path, rel.to_string()), Some(BufReader::new(file))).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

/// 7z chiffré en AES-256, compressé en LZMA2 ; les noms des fichiers sont chiffrés aussi.
fn seven_locked(out: &Path, dir: &Path, names: &[String], password: &str) -> Result<(), String> {
    use sevenz_rust2::{ArchiveWriter, EncoderMethod, Password, encoder_options::AesEncoderOptions};
    let mut sz = ArchiveWriter::new(BufWriter::new(File::create(out).map_err(|e| e.to_string())?)).map_err(|e| e.to_string())?;
    sz.set_content_methods(vec![AesEncoderOptions::new(Password::from(password)).into(), EncoderMethod::LZMA2.into()]);
    sz.set_encrypt_header(true);
    for n in names {
        seven_add(&mut sz, &dir.join(n), n)?;
    }
    sz.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())
}

fn rar_locked(out: &Path, dir: &Path, names: &[String], password: &str) -> Result<(), String> {
    let Some((rar, _)) = winrar() else { return Err("WinRAR is not installed.".to_string()) };
    let mut cmd = Command::new(rar);
    // a : ajouter (un dossier vient avec tout son contenu) ; -hp : contenu et noms de fichiers chiffrés.
    // Pas de -r : il ferait aussi chercher chaque nom de fichier dans tous les sous-dossiers.
    cmd.current_dir(dir).args(["a", "-idq", "-y"]);
    cmd.arg(if password.is_ascii() { "-hp".to_string() } else { format!("-hp{password}") });
    cmd.arg("--").arg(out);
    for n in names {
        if n.starts_with('@') {
            cmd.arg(format!(".\\{n}"))
        } else {
            cmd.arg(n)
        };
    }
    match run(&mut cmd, password.is_ascii().then_some(password)) {
        Ok((0, _)) => Ok(()),
        Ok((code, _)) => Err(format!("WinRAR could not create the archive (error {code}).")),
        Err(e) => Err(e.to_string()),
    }
}

/// Compresse les éléments `names` du dossier `dir` dans `out`, protégé par `password`.
pub fn compress_locked(out: &Path, dir: &Path, names: &[String], password: &str, format: Format) -> Result<(), String> {
    let r = match format {
        Format::Zip => zip_locked(out, dir, names, password),
        Format::SevenZ => seven_locked(out, dir, names, password),
        Format::Rar => rar_locked(out, dir, names, password),
    };
    if r.is_err() {
        let _ = std::fs::remove_file(out);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fex-archive-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Dossier").join("Vide")).unwrap();
        std::fs::write(dir.join("Dossier").join("été.txt"), b"bonjour").unwrap();
        std::fs::write(dir.join("-odd name.txt"), b"x").unwrap();
        dir
    }

    fn list(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn names() {
        for a in ["a.zip", "B.RAR", "x.7z", "backup.tar.gz", "disk.iso", "lib.tgz"] {
            assert!(is_archive(a), "{a}");
        }
        for a in ["a.txt", "zip", ".zip", "photo.png", "archive.zip.part"] {
            assert!(!is_archive(a), "{a}");
        }
        assert_eq!(stem("Photos 2026.zip"), "Photos 2026");
        assert_eq!(stem("backup.TAR.GZ"), "backup");
        assert_eq!(stem("v1.2.7z"), "v1.2");
        assert_eq!(stem("noext"), "noext");
    }

    #[test]
    fn compress_then_extract() {
        let dir = scratch("plain");
        let names = ["Dossier".to_string(), "-odd name.txt".to_string()];
        for ext in ["zip", "7z", "tar.gz"] {
            let archive = dir.join(format!("out.{ext}"));
            compress(&archive, &dir, &names).unwrap();
            let dest = dir.join(format!("extracted-{ext}"));
            extract_new(&archive, &dest, None).unwrap();
            assert_eq!(std::fs::read(dest.join("Dossier").join("été.txt")).unwrap(), b"bonjour", "{ext}");
            assert!(dest.join("-odd name.txt").is_file(), "{ext}");
        }
        // un fichier qui n'est pas une archive : une erreur lisible, et pas de dossier laissé derrière
        std::fs::write(dir.join("fake.zip"), b"not a zip at all").unwrap();
        assert!(matches!(extract_new(&dir.join("fake.zip"), &dir.join("nope"), None), Err(Failure::Message(_))));
        assert!(!dir.join("nope").exists());
        // extraire sur place sans écraser : le fichier déjà là garde son contenu, le reste arrive
        let here = dir.join("here");
        std::fs::create_dir_all(here.join("Dossier")).unwrap();
        std::fs::write(here.join("-odd name.txt"), b"mine").unwrap();
        std::fs::write(here.join("Dossier").join("autre.txt"), b"keep").unwrap();
        extract_here(&dir.join("out.zip"), &here, None).unwrap();
        assert_eq!(std::fs::read(here.join("-odd name.txt")).unwrap(), b"mine");
        assert_eq!(std::fs::read(here.join("Dossier").join("été.txt")).unwrap(), b"bonjour");
        assert_eq!(list(&here), ["-odd name.txt", "Dossier"]);
        assert_eq!(list(&here.join("Dossier")), ["Vide", "autre.txt", "été.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zip_with_a_password() {
        let dir = scratch("locked");
        let names = ["Dossier".to_string(), "-odd name.txt".to_string()];
        let archive = dir.join("secret.zip");
        compress_locked(&archive, &dir, &names, "s3cret é", Format::Zip).unwrap();
        // le contenu n'est pas lisible dans le fichier
        let raw = std::fs::read(&archive).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"bonjour"));
        // sans mot de passe, avec le mauvais, avec le bon
        assert_eq!(extract_new(&archive, &dir.join("a"), None), Err(Failure::NeedPassword));
        assert_eq!(extract_new(&archive, &dir.join("a"), Some("nope")), Err(Failure::WrongPassword));
        assert!(!dir.join("a").exists());
        extract_new(&archive, &dir.join("a"), Some("s3cret é")).unwrap();
        assert_eq!(std::fs::read(dir.join("a").join("Dossier").join("été.txt")).unwrap(), b"bonjour");
        assert!(dir.join("a").join("Dossier").join("Vide").is_dir());
        assert_eq!(list(&dir.join("a")), ["-odd name.txt", "Dossier"]);
        // la date du fichier est conservée (à deux secondes près, précision du format ZIP)
        let (was, now) = (
            std::fs::metadata(dir.join("Dossier").join("été.txt")).unwrap().modified().unwrap(),
            std::fs::metadata(dir.join("a").join("Dossier").join("été.txt")).unwrap().modified().unwrap(),
        );
        let gap = was.duration_since(now).or_else(|_| now.duration_since(was)).unwrap();
        assert!(gap < Duration::from_secs(3), "{gap:?}");
        // un mauvais mot de passe sur place n'ajoute rien au dossier
        let here = dir.join("here");
        std::fs::create_dir(&here).unwrap();
        assert_eq!(extract_here(&archive, &here, Some("wrong")), Err(Failure::WrongPassword));
        assert!(list(&here).is_empty());
        extract_here(&archive, &here, Some("s3cret é")).unwrap();
        assert_eq!(list(&here), ["-odd name.txt", "Dossier"]);
        // Windows (bsdtar) ouvre aussi ce ZIP avec le mot de passe : il est au format standard
        let (code, _) = run(Command::new(tar().unwrap()).arg("-tf").arg(&archive), None).unwrap();
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seven_z_with_a_password() {
        let dir = scratch("7z");
        let names = ["Dossier".to_string(), "-odd name.txt".to_string()];
        let archive = dir.join("secret.7z");
        compress_locked(&archive, &dir, &names, "s3cret é", Format::SevenZ).unwrap();
        // ni le contenu ni les noms ne sont lisibles
        let raw = std::fs::read(&archive).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"bonjour"));
        let utf16: Vec<u8> = "Dossier".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert!(!raw.windows(utf16.len()).any(|w| w == utf16.as_slice()));
        // sans mot de passe, avec le mauvais, avec le bon
        assert_eq!(extract_new(&archive, &dir.join("a"), None), Err(Failure::NeedPassword));
        assert_eq!(extract_new(&archive, &dir.join("a"), Some("nope")), Err(Failure::WrongPassword));
        assert!(!dir.join("a").exists());
        extract_new(&archive, &dir.join("a"), Some("s3cret é")).unwrap();
        assert_eq!(std::fs::read(dir.join("a").join("Dossier").join("été.txt")).unwrap(), b"bonjour");
        assert!(dir.join("a").join("Dossier").join("Vide").is_dir());
        assert_eq!(list(&dir.join("a")), ["-odd name.txt", "Dossier"]);
        // un mauvais mot de passe sur place n'ajoute rien au dossier
        let here = dir.join("here");
        std::fs::create_dir(&here).unwrap();
        assert_eq!(extract_here(&archive, &here, Some("wrong")), Err(Failure::WrongPassword));
        assert!(list(&here).is_empty());
        extract_here(&archive, &here, Some("s3cret é")).unwrap();
        assert_eq!(list(&here), ["-odd name.txt", "Dossier"]);
        // un 7z sans mot de passe (fait par Windows) s'extrait toujours sans rien demander
        let plain = dir.join("plain.7z");
        compress(&plain, &dir, &names).unwrap();
        extract_new(&plain, &dir.join("b"), None).unwrap();
        assert_eq!(std::fs::read(dir.join("b").join("Dossier").join("été.txt")).unwrap(), b"bonjour");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un 7z dont seul le contenu est chiffré (les noms restent lisibles), comme en fait 7-Zip sans « -mhe ».
    #[test]
    fn seven_z_with_visible_names() {
        use sevenz_rust2::{ArchiveWriter, EncoderMethod, Password, encoder_options::AesEncoderOptions};
        let dir = scratch("7z-names");
        let archive = dir.join("names.7z");
        let mut sz = ArchiveWriter::new(BufWriter::new(File::create(&archive).unwrap())).unwrap();
        sz.set_content_methods(vec![AesEncoderOptions::new(Password::from("pw")).into(), EncoderMethod::LZMA2.into()]);
        sz.set_encrypt_header(false);
        seven_add(&mut sz, &dir.join("Dossier"), "Dossier").unwrap();
        sz.finish().unwrap().flush().unwrap();
        assert_eq!(extract_new(&archive, &dir.join("a"), None), Err(Failure::NeedPassword));
        assert_eq!(extract_new(&archive, &dir.join("a"), Some("nope")), Err(Failure::WrongPassword));
        extract_new(&archive, &dir.join("a"), Some("pw")).unwrap();
        assert_eq!(std::fs::read(dir.join("a").join("Dossier").join("été.txt")).unwrap(), b"bonjour");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rar_with_a_password_when_winrar_is_installed() {
        if winrar().is_none() {
            return;
        }
        let dir = scratch("rar");
        let names = ["Dossier".to_string(), "-odd name.txt".to_string()];
        for (k, password) in ["s3cret pass", "mot de passe été €"].into_iter().enumerate() {
            let archive = dir.join(format!("secret{k}.rar"));
            compress_locked(&archive, &dir, &names, password, Format::Rar).unwrap();
            // ni le contenu ni les noms ne sont lisibles
            let raw = std::fs::read(&archive).unwrap();
            assert!(!raw.windows(7).any(|w| w == b"bonjour") && !raw.windows(7).any(|w| w == b"Dossier"));
            let out = dir.join(format!("out{k}"));
            assert_eq!(extract_new(&archive, &out, None), Err(Failure::NeedPassword), "{password}");
            assert_eq!(extract_new(&archive, &out, Some("nope")), Err(Failure::WrongPassword), "{password}");
            assert!(!out.exists());
            extract_new(&archive, &out, Some(password)).unwrap();
            assert_eq!(std::fs::read(out.join("Dossier").join("été.txt")).unwrap(), b"bonjour", "{password}");
            assert_eq!(list(&out), ["-odd name.txt", "Dossier"]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
