//! Le journal des modifications de NTFS (journal USN), lu sans droits d'administrateur.
//!
//! Windows y note chaque création, suppression, renommage ou écriture de fichier. Un programme ordinaire peut le
//! lire par `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL` : les noms de fichiers n'y figurent pas, mais on y trouve
//! l'identifiant du dossier concerné. Cela suffit : l'index relit les dossiers qui ont changé, et eux seulement.
//! Le volume n'est ouvert que le temps d'une lecture : aucun handle ne reste posé sur le disque.

use std::{io, path::Path};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_HANDLE_EOF, HANDLE},
        Storage::FileSystem::{CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
        System::IO::DeviceIoControl,
    },
    core::PCWSTR,
};

const FSCTL_QUERY_USN_JOURNAL: u32 = 0x0009_00F4;
const FSCTL_READ_UNPRIVILEGED_USN_JOURNAL: u32 = 0x0009_03AB;
/// FILE_TRAVERSE | SYNCHRONIZE : le minimum, accordé à tout utilisateur sur un volume.
const VOLUME_ACCESS: u32 = 0x0020 | 0x0010_0000;

/// Où en est le journal d'un volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// Change quand le journal est recréé : les positions notées avant ne valent plus rien.
    pub journal: u64,
    /// Position du prochain enregistrement à lire.
    pub next: i64,
}

/// Ce que la lecture du journal a donné.
#[derive(Debug, PartialEq, Eq)]
pub enum Read {
    /// Lu jusqu'au bout : voici où reprendre la prochaine fois.
    UpTo(Position),
    /// La position demandée n'existe plus (journal recréé, ou enregistrements trop anciens déjà effacés) :
    /// on ne peut pas savoir ce qui a changé, il faut tout relire.
    Lost,
}

struct Volume(HANDLE);

impl Drop for Volume {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Ouvre le volume d'une racine (« C:\ » → `\\.\C:`).
fn open(root: &Path) -> io::Result<Volume> {
    let letter = root.to_str().and_then(|s| s.chars().next()).filter(char::is_ascii_alphabetic).ok_or(io::ErrorKind::InvalidInput)?;
    let name: Vec<u16> = format!(r"\\.\{letter}:").encode_utf16().chain(Some(0)).collect();
    let h = unsafe {
        CreateFileW(PCWSTR(name.as_ptr()), VOLUME_ACCESS, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, None, OPEN_EXISTING, Default::default(), None)
    }
    .map_err(crate::listing::to_io)?;
    Ok(Volume(h))
}

fn le_u64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn query(vol: &Volume) -> io::Result<(u64, i64, i64)> {
    // USN_JOURNAL_DATA_V0 : UsnJournalID, FirstUsn, NextUsn, LowestValidUsn, MaxUsn, MaximumSize, AllocationDelta
    let mut out = [0u8; 80];
    let mut got = 0u32;
    unsafe { DeviceIoControl(vol.0, FSCTL_QUERY_USN_JOURNAL, None, 0, Some(out.as_mut_ptr() as *mut _), out.len() as u32, Some(&mut got), None) }
        .map_err(crate::listing::to_io)?;
    if got < 24 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok((le_u64(&out, 0), le_u64(&out, 8) as i64, le_u64(&out, 16) as i64))
}

/// La position actuelle du journal du volume `root` (là où s'écrira le prochain enregistrement).
/// Erreur : pas de journal (volume FAT, exFAT, réseau) ou accès refusé.
pub fn position(root: &Path) -> io::Result<Position> {
    let vol = open(root)?;
    let (journal, _, next) = query(&vol)?;
    Ok(Position { journal, next })
}

/// Lit tout ce qui s'est passé sur le volume `root` depuis `from`, et appelle `changed` avec l'identifiant NTFS
/// du dossier parent de chaque fichier ou dossier créé, supprimé, renommé ou modifié (plusieurs fois le même, souvent).
pub fn read(root: &Path, from: Position, changed: &mut dyn FnMut(u64)) -> io::Result<Read> {
    let vol = open(root)?;
    let (journal, first, _) = query(&vol)?;
    if journal != from.journal || from.next < first {
        return Ok(Read::Lost);
    }
    let mut next = from.next;
    let mut buf = vec![0u64; 64 * 1024 / 8];
    loop {
        // READ_USN_JOURNAL_DATA_V1 : StartUsn, ReasonMask, ReturnOnlyOnClose, Timeout, BytesToWaitFor, UsnJournalID,
        // MinMajorVersion, MaxMajorVersion
        let mut input = [0u8; 48];
        input[0..8].copy_from_slice(&next.to_le_bytes());
        input[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        input[32..40].copy_from_slice(&journal.to_le_bytes());
        input[40..42].copy_from_slice(&2u16.to_le_bytes());
        input[42..44].copy_from_slice(&3u16.to_le_bytes());
        let mut got = 0u32;
        let r = unsafe {
            DeviceIoControl(
                vol.0,
                FSCTL_READ_UNPRIVILEGED_USN_JOURNAL,
                Some(input.as_ptr() as *const _),
                input.len() as u32,
                Some(buf.as_mut_ptr() as *mut _),
                (buf.len() * 8) as u32,
                Some(&mut got),
                None,
            )
        };
        if let Err(e) = r {
            if e.code() == ERROR_HANDLE_EOF.to_hresult() {
                break;
            }
            // ERROR_JOURNAL_ENTRY_DELETED (1181) et consorts : la position n'est plus lisible
            let code = e.code().0 as u32 & 0xFFFF;
            if (1178..=1181).contains(&code) {
                return Ok(Read::Lost);
            }
            return Err(crate::listing::to_io(e));
        }
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, got as usize) };
        if bytes.len() < 8 {
            break;
        }
        let new_next = le_u64(bytes, 0) as i64;
        let mut at = 8;
        while at + 60 <= bytes.len() {
            let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            if len < 60 || at + len > bytes.len() {
                break;
            }
            let major = u16::from_le_bytes(bytes[at + 4..at + 6].try_into().unwrap());
            // V2 : identifiants sur 64 bits (parent à l'octet 16) ; V3 : sur 128 bits (parent à l'octet 24),
            // dont les 64 premiers sont l'identifiant NTFS habituel
            match major {
                2 => changed(le_u64(bytes, at + 16)),
                3 if len >= 76 => changed(le_u64(bytes, at + 24)),
                _ => {}
            }
            at += len;
        }
        if new_next <= next {
            break;
        }
        next = new_next;
        if bytes.len() == 8 {
            break;
        }
    }
    Ok(Read::UpTo(Position { journal, next }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::tests::TempDir;
    use std::collections::HashSet;

    #[test]
    fn follows_changes_without_admin_rights() {
        // dossier d'essai sur le disque système (NTFS avec journal, en général) ; sinon le test ne dit rien
        let dir = TempDir::new("usn");
        let root = std::path::PathBuf::from(format!("{}\\", &dir.0.display().to_string()[..2]));
        let Ok(start) = position(&root) else { return };
        let sub = dir.0.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("a.txt"), b"hello").unwrap();
        std::fs::rename(sub.join("a.txt"), sub.join("b.txt")).unwrap();
        let mut parents = HashSet::new();
        let r = read(&root, start, &mut |p| {
            parents.insert(p);
        })
        .expect("journal lisible");
        let Read::UpTo(end) = r else { panic!("position perdue") };
        assert!(end.next > start.next && end.journal == start.journal);
        // l'identifiant NTFS du sous-dossier figure parmi les dossiers touchés
        let id = crate::listing::list_dir(&dir.0, None).unwrap().into_iter().find(|e| e.name == "sub").unwrap().file_id;
        assert!(parents.contains(&id), "{id:#x} absent de {} dossiers", parents.len());
        // rien de neuf : la position ne bouge presque pas, et un journal inconnu est signalé
        assert!(matches!(read(&root, end, &mut |_| {}), Ok(Read::UpTo(_))));
        assert_eq!(read(&root, Position { journal: start.journal ^ 1, next: start.next }, &mut |_| {}).unwrap(), Read::Lost);
        assert_eq!(read(&root, Position { journal: start.journal, next: -5 }, &mut |_| {}).unwrap(), Read::Lost);
    }

    #[test]
    fn no_journal_is_an_error_not_a_crash() {
        assert!(position(Path::new(r"\\server\share\")).is_err());
        assert!(position(Path::new("")).is_err());
    }
}
