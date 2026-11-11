//! Téléchargement de vidéos / audio (YouTube et ~1 800 autres sites) via yt-dlp + FFmpeg.
//! Les deux outils (libres) sont téléchargés une seule fois, à la demande, dans %LOCALAPPDATA%\Plume\tools.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, atomic::Ordering},
};
use tao::event_loop::EventLoopProxy;
use windows::{
    Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, Urlmon::URLDownloadToFileW},
    core::HSTRING,
};

use crate::{DL_ID, Ev};

const NO_WINDOW: u32 = 0x0800_0000; // pas de console qui s'ouvre
const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

/// Téléchargements en cours (pour pouvoir les annuler).
static JOBS: Mutex<Option<HashMap<u32, Child>>> = Mutex::new(None);

pub fn tools_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(r"Plume\tools")
}
fn ytdlp() -> PathBuf {
    tools_dir().join("yt-dlp.exe")
}
fn ffmpeg() -> PathBuf {
    tools_dir().join("ffmpeg.exe")
}
pub fn have_ytdlp() -> bool {
    ytdlp().exists()
}
pub fn have_ffmpeg() -> bool {
    ffmpeg().exists()
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        URLDownloadToFileW(None, &HSTRING::from(url), &HSTRING::from(to), 0, None).map_err(|e| e.message().to_string())
    }
}

/// Installe yt-dlp (et FFmpeg si demandé). Bloquant : à appeler dans un fil.
pub fn install(with_ffmpeg: bool, p: &EventLoopProxy<Ev>) -> Result<(), String> {
    let dir = tools_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !have_ytdlp() {
        let _ = p.send_event(Ev::Media(json!({"state": "installing", "tool": "yt-dlp"})));
        let tmp = dir.join("yt-dlp.exe.part");
        download(YTDLP_URL, &tmp)?;
        std::fs::rename(&tmp, ytdlp()).map_err(|e| e.to_string())?;
    }
    if with_ffmpeg && !have_ffmpeg() {
        let _ = p.send_event(Ev::Media(json!({"state": "installing", "tool": "FFmpeg"})));
        let zip = dir.join("ffmpeg.zip.part");
        download(FFMPEG_URL, &zip)?;
        let data = std::fs::read(&zip).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&zip);
        // seuls ffmpeg.exe et ffprobe.exe nous intéressent
        crate::ext::unzip_only(&data, &dir, &["ffmpeg.exe", "ffprobe.exe"])?;
        if !have_ffmpeg() {
            return Err("ffmpeg.exe introuvable dans l'archive".into());
        }
    }
    Ok(())
}

fn cmd() -> Command {
    let mut c = Command::new(ytdlp());
    c.creation_flags(NO_WINDOW).stdin(Stdio::null()).arg("--encoding").arg("utf-8").arg("--no-playlist");
    if have_ffmpeg() {
        c.arg("--ffmpeg-location").arg(tools_dir());
    }
    c
}

/// Infos de la vidéo : titre, miniature, durée, liste complète des formats.
pub fn probe(url: &str) -> Result<Value, String> {
    let out = cmd().args(["-J", "--no-warnings", "--", url]).stdout(Stdio::piped()).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().rev().find(|l| l.contains("ERROR")).unwrap_or("yt-dlp error");
        return Err(line.replace("ERROR: ", "").chars().take(300).collect());
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let formats: Vec<Value> = v["formats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["protocol"].as_str().is_none_or(|p| !p.contains("mhtml")))
        .map(|f| {
            let size = f["filesize"].as_f64().or(f["filesize_approx"].as_f64()).unwrap_or(0.0);
            json!({"id": f["format_id"], "ext": f["ext"], "res": f["resolution"], "h": f["height"], "fps": f["fps"],
                   "v": f["vcodec"].as_str().is_some_and(|c| c != "none"), "a": f["acodec"].as_str().is_some_and(|c| c != "none"),
                   "vc": f["vcodec"], "ac": f["acodec"], "abr": f["abr"], "size": size, "note": f["format_note"]})
        })
        .collect();
    let maxh = formats.iter().filter_map(|f| f["h"].as_u64()).max().unwrap_or(0);
    Ok(json!({"title": v["title"], "thumb": v["thumbnail"], "dur": v["duration"], "by": v["uploader"].as_str().or(v["channel"].as_str()),
              "site": v["extractor_key"], "url": v["webpage_url"].as_str().unwrap_or(url), "formats": formats, "maxh": maxh}))
}

/// Arguments yt-dlp selon le format choisi.
fn format_args(kind: &str, fmt: &str, h: u64) -> Vec<String> {
    let s = |x: &str| x.to_string();
    let audio = |codec: &str| vec![s("-f"), s("bestaudio/best"), s("-x"), s("--audio-format"), s(codec), s("--audio-quality"), s("0")];
    match kind {
        "mp3" | "wav" | "flac" | "opus" | "m4a" | "aac" | "ogg" => audio(if kind == "ogg" { "vorbis" } else { kind }),
        "mp4" => vec![
            s("-f"),
            format!("bv*[height<={h}][ext=mp4]+ba[ext=m4a]/bv*[height<={h}]+ba/b[height<={h}]/b"),
            s("--merge-output-format"),
            s("mp4"),
            s("--remux-video"),
            s("mp4"),
        ],
        "webm" => vec![s("-f"), format!("bv*[height<={h}][ext=webm]+ba[ext=webm]/bv*[height<={h}]+ba/b"), s("--merge-output-format"), s("webm")],
        "mkv" => vec![s("-f"), format!("bv*[height<={h}]+ba/b"), s("--merge-output-format"), s("mkv")],
        "mov" | "avi" => vec![s("-f"), format!("bv*[height<={h}]+ba/b"), s("--recode-video"), s(kind)],
        // format précis choisi dans la liste complète (vidéo seule → on ajoute la meilleure piste audio)
        _ => vec![s("-f"), if fmt.is_empty() { s("b") } else { format!("{fmt}+ba/{fmt}") }],
    }
}

/// Lance un téléchargement ; la progression arrive dans le panneau Téléchargements.
pub fn start(url: String, kind: String, fmt: String, h: u64, title: String, p: EventLoopProxy<Ev>) {
    let id = DL_ID.fetch_add(1, Ordering::Relaxed) + 1;
    let dir = std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Downloads")).unwrap_or_else(std::env::temp_dir);
    let label = format!("{title} ({})", kind.to_uppercase());
    let _ = p.send_event(Ev::Dl(
        json!({"id": id, "name": label, "path": "", "url": url, "total": 0, "recv": 0, "state": "run", "media": true, "t": crate::now_ms()}),
    ));
    std::thread::spawn(move || {
        let mut c = cmd();
        c.args(format_args(&kind, &fmt, if h == 0 { 4320 } else { h }))
            .args(["--newline", "--progress", "--quiet", "--no-warnings", "--no-mtime", "--windows-filenames", "--progress-template"])
            .arg("download:P|%(progress.downloaded_bytes)s|%(progress.total_bytes,progress.total_bytes_estimate)s")
            .args(["--print", "after_move:F|%(filepath)s", "-o"])
            .arg(dir.join("%(title).120B [%(id)s].%(ext)s"))
            .arg("--")
            .arg(&url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match c.spawn() {
            Ok(ch) => ch,
            Err(e) => {
                let _ = p.send_event(Ev::Dl(json!({"id": id, "state": "fail", "err": e.to_string()})));
                return;
            }
        };
        let out = child.stdout.take();
        let err = child.stderr.take();
        JOBS.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, child);
        // la progression peut arriver sur stderr (mode --quiet) : on la lit des deux côtés
        let pe = p.clone();
        let errt = std::thread::spawn(move || {
            let mut last = None;
            for l in err.into_iter().flat_map(|e| BufReader::new(e).lines().map_while(Result::ok)) {
                if let Some(r) = l.strip_prefix("P|") {
                    let mut it = r.split('|').map(|x| x.trim().parse::<f64>().unwrap_or(0.0));
                    let (got, tot) = (it.next().unwrap_or(0.0), it.next().unwrap_or(0.0));
                    let _ = pe.send_event(Ev::Dl(json!({"id": id, "recv": got as i64, "total": tot as i64})));
                } else if l.contains("ERROR") {
                    last = Some(l);
                }
            }
            last
        });
        let mut path = String::new();
        if let Some(o) = out {
            for line in BufReader::new(o).lines().map_while(Result::ok) {
                if let Some(r) = line.strip_prefix("P|") {
                    let mut it = r.split('|').map(|x| x.trim().parse::<f64>().unwrap_or(0.0));
                    let (got, tot) = (it.next().unwrap_or(0.0), it.next().unwrap_or(0.0));
                    let _ = p.send_event(Ev::Dl(json!({"id": id, "recv": got as i64, "total": tot as i64})));
                } else if let Some(f) = line.strip_prefix("F|") {
                    path = f.trim().to_string();
                }
            }
        }
        let child = JOBS.lock().unwrap().as_mut().and_then(|m| m.remove(&id));
        let ok = child.map(|mut c| c.wait().is_ok_and(|s| s.success())).unwrap_or(false);
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let name = Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned());
        let msg = if ok { Value::Null } else { json!(errt.join().ok().flatten().unwrap_or_else(|| "cancelled".into()).replace("ERROR: ", "")) };
        let _ = p.send_event(Ev::Dl(json!({"id": id, "state": if ok && !path.is_empty() { "done" } else { "fail" },
            "path": path, "name": name.unwrap_or(label), "recv": size, "total": size, "err": msg})));
    });
}

/// Annule un téléchargement média (renvoie false si ce n'en est pas un).
pub fn cancel(id: u32) -> bool {
    match JOBS.lock().unwrap().as_mut().and_then(|m| m.get_mut(&id)) {
        Some(c) => {
            let _ = c.kill();
            true
        }
        None => false,
    }
}

/// Mise à jour silencieuse de yt-dlp (YouTube change souvent) — au plus une fois par jour.
pub fn self_update() {
    let stamp = tools_dir().join("yt-dlp.updated");
    let fresh = std::fs::metadata(&stamp).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|e| e.as_secs() < 86_400));
    if have_ytdlp() && !fresh {
        let _ = std::fs::write(&stamp, b"");
        std::thread::spawn(|| {
            let _ = Command::new(ytdlp()).creation_flags(NO_WINDOW).arg("-U").stdout(Stdio::null()).stderr(Stdio::null()).status();
        });
    }
}
