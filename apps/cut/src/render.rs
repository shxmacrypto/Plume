//! The edit as one FFmpeg run: each clip becomes an input (seeked to what it uses), its own chain of filters
//! (speed, crop, size, colour, opacity, rotation, transitions) and is laid over a black frame at its place; texts
//! are drawn with `drawtext`; sounds are delayed to their place, faded, mixed, and the music lowered under the
//! voices when asked. The same plan, smaller and without encoding, plays in the preview.
//!
//! The filter graph is written to a file and FFmpeg runs in a folder of its own, where the texts and fonts are
//! copied under plain names: nothing in the graph needs escaping.

use crate::{
    ffmpeg::Hw,
    model::{Anim, Clip, MediaKind, Project, Span, TextStyle, TrackKind},
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
}

impl Codec {
    pub fn key(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
        }
    }
    pub fn from_key(k: &str) -> Codec {
        if k == "hevc" { Codec::Hevc } else { Codec::H264 }
    }
}

/// What to make.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// An MP4 file.
    File { path: PathBuf, codec: Codec, hw: Option<Hw> },
    /// Raw BGRA frames on the standard output (the preview).
    Frames,
    /// Raw 48 kHz stereo 16-bit sound on the standard output (the preview).
    Sound,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// FFmpeg's arguments; the graph is in `graph.txt` next to it (see `files`).
    pub args: Vec<String>,
    pub graph: String,
    /// Files to put in the run's folder: name and contents (texts).
    pub texts: Vec<(String, String)>,
    /// Fonts to copy there: name and source.
    pub fonts: Vec<(String, PathBuf)>,
    /// Seconds of output.
    pub duration: f64,
}

pub const GRAPH: &str = "graph.txt";

/// Output sizes: the shorter side is `short`, the frame `aspect` (even sides, as encoders want).
pub fn out_size(aspect: [u32; 2], short: u32) -> (u32, u32) {
    let (a, b) = (aspect[0].max(1) as f64, aspect[1].max(1) as f64);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    if a >= b { (even(short as f64 * a / b), even(short as f64)) } else { (even(short as f64), even(short as f64 * b / a)) }
}

/// A picture of `sw` × `sh` placed by a clip in a `w` × `h` frame: its size once scaled and cropped, and its centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// Size of the whole (uncropped) picture.
    pub full_w: f64,
    pub full_h: f64,
    /// Size of what is left after the crop.
    pub w: f64,
    pub h: f64,
    pub cx: f64,
    pub cy: f64,
}

pub fn place(c: &Clip, sw: f64, sh: f64, w: f64, h: f64) -> Place {
    let (sw, sh) = (sw.max(1.0), sh.max(1.0));
    let fit = (w / sw).min(h / sh) * c.scale.max(0.01);
    let (full_w, full_h) = (sw * fit, sh * fit);
    let [l, t, r, b] = crop_of(c);
    Place { full_w, full_h, w: full_w * (1.0 - l - r), h: full_h * (1.0 - t - b), cx: w / 2.0 + c.x * w, cy: h / 2.0 + c.y * h }
}

/// The crop kept sensible: at least 5 % of the picture stays.
pub fn crop_of(c: &Clip) -> [f64; 4] {
    let [mut l, mut t, mut r, mut b] = c.crop.map(|v| v.clamp(0.0, 0.95));
    if l + r > 0.95 {
        let k = 0.95 / (l + r);
        l *= k;
        r *= k;
    }
    if t + b > 0.95 {
        let k = 0.95 / (t + b);
        t *= k;
        b *= k;
    }
    [l, t, r, b]
}

/// The font file of a family (Windows' fonts), bold when asked and there is one.
pub fn font_file(family: &str, bold: bool) -> PathBuf {
    let dir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts");
    let (regular, heavy) = match family {
        "Arial" => ("arial.ttf", "arialbd.ttf"),
        "Georgia" => ("georgia.ttf", "georgiab.ttf"),
        "Impact" => ("impact.ttf", "impact.ttf"),
        "Consolas" => ("consola.ttf", "consolab.ttf"),
        "Comic Sans MS" => ("comic.ttf", "comicbd.ttf"),
        "Times New Roman" => ("times.ttf", "timesbd.ttf"),
        "Verdana" => ("verdana.ttf", "verdanab.ttf"),
        _ => ("segoeui.ttf", "segoeuib.ttf"),
    };
    let want = dir.join(if bold { heavy } else { regular });
    if want.is_file() { want } else { dir.join(regular) }
}

pub const FONTS: [&str; 8] = ["Segoe UI", "Arial", "Georgia", "Impact", "Consolas", "Comic Sans MS", "Times New Roman", "Verdana"];

/// "1.5" / "2" / "0.033333": short, exact enough, never with a comma.
pub fn n(v: f64) -> String {
    let t = format!("{v:.6}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" { "0".into() } else { t.to_string() }
}

/// "#ff8800" → "0xff8800" (FFmpeg's colours), anything else white.
pub fn hex_color(s: &str) -> String {
    let h = s.trim().trim_start_matches('#');
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) { format!("0x{}", h.to_ascii_lowercase()) } else { "0xffffff".into() }
}

/// `atempo` only takes 0.5..2: longer changes are chained.
pub fn atempo(speed: f64) -> Vec<String> {
    let mut s = speed.clamp(0.1, 10.0);
    let mut out = Vec::new();
    while s > 2.0 {
        out.push("atempo=2".to_string());
        s /= 2.0;
    }
    while s < 0.5 {
        out.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    if (s - 1.0).abs() > 1e-6 {
        out.push(format!("atempo={}", n(s)));
    }
    out
}

struct Input {
    args: Vec<String>,
}

/// One clip's use of its media: what to read and how its timeline window maps to it.
struct Use {
    input: usize,
    span: Span,
    /// Seconds of the timeline at the start of the window, and its length.
    at: f64,
    len: f64,
    /// Timeline seconds of the window before the media has anything (a handle before the media's start).
    missing: f64,
}

fn s(v: &str) -> String {
    v.to_string()
}

/// Which video/overlay tracks have their sound used.
fn sound_on(p: &Project, track: usize) -> bool {
    !p.tracks[track].muted
}

fn anim_alpha(st: &TextStyle, a: f64, b: f64) -> Option<String> {
    let l = n(st.anim_len.max(0.05));
    let mut terms = Vec::new();
    if st.anim_in != Anim::None {
        terms.push(format!("(t-{})/{l}", n(a)));
    }
    if st.anim_out != Anim::None {
        terms.push(format!("({}-t)/{l}", n(b)));
    }
    match terms.len() {
        0 => None,
        1 => Some(format!("max(0,min(1,{}))", terms[0])),
        _ => Some(format!("max(0,min(1,min({},{})))", terms[0], terms[1])),
    }
}

/// How far a text moves while it comes in or goes out (parts of the frame).
pub const RISE: f64 = 0.04;
pub const SLIDE: f64 = 0.08;

/// The offset of an animated text at `t` seconds (`a`..`b` its time on screen): (dx, dy) as parts of the frame,
/// and its opacity. The preview and `drawtext` follow the same curves.
pub fn text_motion(st: &TextStyle, t: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let l = st.anim_len.max(0.05);
    let kin = ((t - a) / l).clamp(0.0, 1.0);
    let kout = ((b - t) / l).clamp(0.0, 1.0);
    let (mut dx, mut dy, mut alpha) = (0.0, 0.0, 1.0f64);
    match st.anim_in {
        Anim::None => {}
        Anim::Fade => alpha = alpha.min(kin),
        Anim::Rise => {
            alpha = alpha.min(kin);
            dy += RISE * (1.0 - kin);
        }
        Anim::Slide => {
            alpha = alpha.min(kin);
            dx -= SLIDE * (1.0 - kin);
        }
    }
    match st.anim_out {
        Anim::None => {}
        Anim::Fade => alpha = alpha.min(kout),
        Anim::Rise => {
            alpha = alpha.min(kout);
            dy -= RISE * (1.0 - kout);
        }
        Anim::Slide => {
            alpha = alpha.min(kout);
            dx += SLIDE * (1.0 - kout);
        }
    }
    (dx, dy, alpha)
}

fn motion_expr(anim_in: Anim, anim_out: Anim, slide: Anim, amount: f64, a: f64, b: f64, l: f64) -> String {
    let mut e = String::new();
    if anim_in == slide {
        e.push_str(&format!("{}{}*(1-min(1,max(0,(t-{})/{})))", if slide == Anim::Slide { "-" } else { "+" }, n(amount), n(a), n(l)));
    }
    if anim_out == slide {
        e.push_str(&format!("{}{}*(1-min(1,max(0,({}-t)/{})))", if slide == Anim::Slide { "+" } else { "-" }, n(amount), n(b), n(l)));
    }
    e
}

/// The plan of a whole project (or of `sub_range` of it) for `w` × `h` at the project's frame rate.
pub fn plan(p: &Project, w: u32, h: u32, target: &Target) -> Plan {
    let fps = p.fps();
    let fps_s = p.fps.max(1).to_string();
    let total_frames = p.duration().max(1);
    let total = total_frames as f64 / fps;
    let (fw, fh) = (w as f64, h as f64);
    let want_video = !matches!(target, Target::Sound);
    let want_sound = !matches!(target, Target::Frames);

    let mut inputs: Vec<Input> = Vec::new();
    let mut graph: Vec<String> = Vec::new();
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut fonts: Vec<(String, PathBuf)> = Vec::new();
    // (clip id) → its input and window, so the sound of a video clip reads the same input
    let mut uses: Vec<(u64, Use)> = Vec::new();

    let use_of = |p: &Project, t: usize, i: usize, inputs: &mut Vec<Input>| -> Option<Use> {
        let c = &p.tracks[t].clips[i];
        let m = p.media_of(c)?;
        if m.missing || m.path.as_os_str().is_empty() {
            return None;
        }
        let span = p.span(t, i);
        let at = span.from as f64 / fps;
        let len = (span.to - span.from) as f64 / fps;
        let path = m.path.to_string_lossy().into_owned();
        if m.kind == MediaKind::Image {
            inputs.push(Input { args: vec![s("-loop"), s("1"), s("-framerate"), fps_s.clone(), s("-t"), n(len + 0.5), s("-i"), path] });
            return Some(Use { input: inputs.len() - 1, span, at, len, missing: 0.0 });
        }
        let pre = (c.start - span.from) as f64 / fps * c.speed;
        let src0 = c.src_in - pre;
        let missing_src = (-src0).max(0.0);
        let s0 = src0.max(0.0);
        let need = len * c.speed - missing_src + 0.5;
        inputs.push(Input { args: vec![s("-ss"), n(s0), s("-t"), n(need.max(0.1)), s("-i"), path] });
        Some(Use { input: inputs.len() - 1, span, at, len, missing: missing_src / c.speed })
    };

    // ── pictures, bottom track first ──
    let mut cur = s("base");
    if want_video {
        graph.push(format!("color=c=black:s={w}x{h}:r={fps_s}:d={},format=rgba[base]", n(total)));
        let mut k = 0usize;
        for t in (0..p.tracks.len()).rev() {
            let tr = &p.tracks[t];
            if !tr.kind.visual() || tr.hidden {
                continue;
            }
            for i in 0..tr.clips.len() {
                let c = &tr.clips[i];
                k += 1;
                if let Some(st) = &c.text {
                    if st.text.trim().is_empty() {
                        continue;
                    }
                    let (a, b) = (c.start as f64 / fps, c.end() as f64 / fps);
                    let tname = format!("t{k}.txt");
                    texts.push((tname.clone(), st.text.replace("\r\n", "\n")));
                    let file = font_file(&st.font, st.bold);
                    let fname = match fonts.iter().find(|(_, f)| *f == file) {
                        Some((nm, _)) => nm.clone(),
                        None => {
                            let nm = format!("f{}.ttf", fonts.len());
                            fonts.push((nm.clone(), file));
                            nm
                        }
                    };
                    let size = (st.size * fh).max(4.0);
                    let l = st.anim_len.max(0.05);
                    let mx = motion_expr(st.anim_in, st.anim_out, Anim::Slide, SLIDE * fw, a, b, l);
                    let my = motion_expr(st.anim_in, st.anim_out, Anim::Rise, RISE * fh, a, b, l);
                    let mut f = format!(
                        "drawtext=fontfile={fname}:textfile={tname}:fontsize={}:fontcolor={}:line_spacing={}:x='(w-text_w)/2+{}{mx}':y='(h-text_h)/2+{}{my}'",
                        n(size.round()),
                        hex_color(&st.color),
                        n((size * 0.15).round()),
                        n((c.x * fw).round()),
                        n((c.y * fh).round()),
                    );
                    if st.bg_opacity > 0.01 {
                        f.push_str(&format!(
                            ":box=1:boxcolor={}@{}:boxborderw={}",
                            hex_color(&st.bg),
                            n(st.bg_opacity.clamp(0.0, 1.0)),
                            n((size * 0.3).round())
                        ));
                    }
                    f.push_str(&format!(":enable='between(t,{},{})'", n(a), n(b - 0.5 / fps)));
                    if let Some(al) = anim_alpha(st, a, b) {
                        f.push_str(&format!(":alpha='{al}'"));
                    }
                    graph.push(format!("[{cur}]{f}[d{k}]"));
                    cur = format!("d{k}");
                    continue;
                }
                let Some(m) = p.media_of(c).cloned() else { continue };
                let Some(u) = use_of(p, t, i, &mut inputs) else { continue };
                let frames = u.span.to - u.span.from;
                let mut f: Vec<String> = Vec::new();
                if m.kind == MediaKind::Image {
                    f.push(s("setpts=PTS-STARTPTS"));
                } else {
                    f.push(format!("setpts=(PTS-STARTPTS)/{}", n(c.speed)));
                }
                f.push(format!("fps={fps_s}"));
                if u.missing > 0.0 {
                    f.push(format!("tpad=start_duration={}:start_mode=clone", n(u.missing)));
                }
                f.push(s("tpad=stop_mode=clone:stop_duration=2"));
                f.push(format!("trim=end_frame={frames}"));
                let pl = place(c, m.width as f64, m.height as f64, fw, fh);
                let [l, tp, r, b] = crop_of(c);
                if l + tp + r + b > 1e-4 {
                    f.push(format!("crop=w=iw*{}:h=ih*{}:x=iw*{}:y=ih*{}", n(1.0 - l - r), n(1.0 - tp - b), n(l), n(tp)));
                }
                f.push(format!("scale={}:{}:flags=bicubic", (pl.w.round() as i64).max(2), (pl.h.round() as i64).max(2)));
                if c.has_color() {
                    f.push(format!(
                        "eq=brightness={}:contrast={}:saturation={}",
                        n(c.brightness.clamp(-1.0, 1.0)),
                        n(c.contrast.clamp(0.0, 2.0)),
                        n(c.saturation.clamp(0.0, 3.0))
                    ));
                }
                f.push(s("format=rgba"));
                if c.opacity < 0.999 {
                    f.push(format!("colorchannelmixer=aa={}", n(c.opacity.clamp(0.0, 1.0))));
                }
                if c.rotation.rem_euclid(360.0).abs() > 0.01 {
                    let a = n(c.rotation.to_radians());
                    f.push(format!("rotate={a}:c=none:ow=rotw({a}):oh=roth({a})"));
                }
                f.push(format!("setpts=PTS-STARTPTS+{}/TB", n(u.at)));
                let sec = |fr: i64| n(fr as f64 / fps);
                if let Some((a, len)) = u.span.alpha_in {
                    f.push(format!("fade=t=in:st={}:d={}:alpha=1", sec(a), sec(len)));
                }
                // on the main track, a dip goes through black; above it, the picture fades out instead
                let alpha = if tr.kind == TrackKind::Video { "" } else { ":alpha=1" };
                if let Some((a, len)) = u.span.black_in {
                    f.push(format!("fade=t=in:st={}:d={}{alpha}", sec(a), sec(len)));
                }
                if let Some((a, len)) = u.span.black_out {
                    f.push(format!("fade=t=out:st={}:d={}{alpha}", sec(a), sec(len)));
                }
                graph.push(format!("[{}:v]{}[v{k}]", u.input, f.join(",")));
                let to = u.at + u.len;
                graph.push(format!(
                    "[{cur}][v{k}]overlay=x={}-overlay_w/2:y={}-overlay_h/2:eof_action=pass:enable='between(t,{},{})'[o{k}]",
                    n(pl.cx),
                    n(pl.cy),
                    n(u.at),
                    n(to - 0.5 / fps)
                ));
                cur = format!("o{k}");
                uses.push((c.id, u));
            }
        }
        let last = match target {
            Target::Frames => "format=bgra",
            _ => "format=yuv420p",
        };
        graph.push(format!("[{cur}]{last}[vout]"));
    }

    // ── sounds ──
    let mut voices: Vec<String> = Vec::new();
    let mut music: Vec<String> = Vec::new();
    if want_sound {
        let mut k = 0usize;
        for t in 0..p.tracks.len() {
            let tr = &p.tracks[t];
            if tr.kind == TrackKind::Text || !sound_on(p, t) {
                continue;
            }
            for i in 0..tr.clips.len() {
                let c = &tr.clips[i];
                k += 1;
                let Some(m) = p.media_of(c) else { continue };
                if !m.has_audio || m.kind == MediaKind::Image || c.volume <= 0.001 {
                    continue;
                }
                // a video clip's sound reads the input its picture opened (when its track is shown)
                let u = match uses.iter().position(|(id, _)| *id == c.id) {
                    Some(at) => uses.remove(at).1,
                    None => match use_of(p, t, i, &mut inputs) {
                        Some(u) => u,
                        None => continue,
                    },
                };
                let mut f = vec![s("aformat=sample_rates=48000:channel_layouts=stereo"), s("asetpts=PTS-STARTPTS")];
                f.extend(atempo(c.speed));
                let start = u.at + u.missing;
                f.push(format!("atrim=end={}", n((u.len - u.missing).max(0.01))));
                f.push(format!("adelay={}:all=1", ((start * 1000.0).round() as i64).max(0)));
                if (c.volume - 1.0).abs() > 1e-3 {
                    f.push(format!("volume={}", n(c.volume.clamp(0.0, 4.0))));
                }
                let sec = |fr: i64| fr as f64 / fps;
                let (cs, ce) = (sec(c.start), sec(c.end()));
                if c.fade_in > 0.0 {
                    f.push(format!("afade=t=in:st={}:d={}", n(cs), n(c.fade_in.min(ce - cs))));
                }
                if c.fade_out > 0.0 {
                    let d = c.fade_out.min(ce - cs);
                    f.push(format!("afade=t=out:st={}:d={}", n(ce - d), n(d)));
                }
                for (win, dir) in [(u.span.alpha_in, "in"), (u.span.black_in, "in"), (u.span.xfade_out, "out"), (u.span.black_out, "out")] {
                    if let Some((a, len)) = win {
                        f.push(format!("afade=t={dir}:st={}:d={}", n(sec(a)), n(sec(len))));
                    }
                }
                let label = format!("a{k}");
                graph.push(format!("[{}:a]{}[{label}]", u.input, f.join(",")));
                if tr.kind == TrackKind::Music { music.push(label) } else { voices.push(label) }
            }
        }
        let mix = |graph: &mut Vec<String>, list: &[String], out: &str| {
            if list.len() == 1 {
                graph.push(format!("[{}]anull[{out}]", list[0]));
            } else {
                graph.push(format!(
                    "{}amix=inputs={}:normalize=0:dropout_transition=0[{out}]",
                    list.iter().map(|l| format!("[{l}]")).collect::<String>(),
                    list.len()
                ));
            }
        };
        graph.push(format!("anullsrc=r=48000:cl=stereo,atrim=duration={}[sil]", n(total)));
        let mut parts = vec![s("sil")];
        if !voices.is_empty() && !music.is_empty() && p.duck {
            mix(&mut graph, &voices, "voice");
            mix(&mut graph, &music, "music");
            graph.push(s("[voice]asplit=2[vk][vs]"));
            graph.push(s("[music][vs]sidechaincompress=threshold=0.02:ratio=8:attack=20:release=400[duck]"));
            parts.push(s("vk"));
            parts.push(s("duck"));
        } else {
            if !voices.is_empty() {
                mix(&mut graph, &voices, "voice");
                parts.push(s("voice"));
            }
            if !music.is_empty() {
                mix(&mut graph, &music, "music");
                parts.push(s("music"));
            }
        }
        graph.push(format!(
            "{}amix=inputs={}:duration=first:normalize=0:dropout_transition=0,aformat=sample_rates=48000:channel_layouts=stereo[aout]",
            parts.iter().map(|l| format!("[{l}]")).collect::<String>(),
            parts.len()
        ));
    }

    // ── the command line ──
    let mut args = vec![s("-hide_banner"), s("-nostdin"), s("-v"), s("error")];
    if matches!(target, Target::File { .. }) {
        args.extend([s("-y"), s("-progress"), s("pipe:1"), s("-nostats")]);
    }
    for i in &inputs {
        args.extend(i.args.iter().cloned());
    }
    args.extend([s("-/filter_complex"), s(GRAPH)]);
    match target {
        Target::File { path, codec, hw } => {
            args.extend([s("-map"), s("[vout]"), s("-map"), s("[aout]")]);
            venc(&mut args, *codec, *hw);
            args.extend([s("-c:a"), s("aac"), s("-b:a"), s("192k"), s("-ar"), s("48000")]);
            args.extend([s("-r"), fps_s.clone(), s("-t"), n(total), s("-movflags"), s("+faststart")]);
            args.push(path.to_string_lossy().into_owned());
        }
        Target::Frames => {
            args.extend([s("-map"), s("[vout]"), s("-t"), n(total), s("-f"), s("rawvideo"), s("-pix_fmt"), s("bgra"), s("pipe:1")]);
        }
        Target::Sound => {
            args.extend([s("-map"), s("[aout]"), s("-t"), n(total), s("-f"), s("s16le"), s("-ar"), s("48000"), s("-ac"), s("2"), s("pipe:1")]);
        }
    }
    Plan { args, graph: graph.join(";\n"), texts, fonts, duration: total }
}

/// The video encoder: the PC's hardware one, or x264 / x265.
pub fn venc(a: &mut Vec<String>, codec: Codec, hw: Option<Hw>) {
    let push = |a: &mut Vec<String>, items: &[&str]| a.extend(items.iter().map(|x| x.to_string()));
    match hw {
        Some(h) => {
            push(a, &["-c:v", &format!("{}_{}", codec.key(), h.suffix())]);
            match h {
                Hw::Nvenc => push(a, &["-preset", "p5", "-rc", "vbr", "-cq", if codec == Codec::H264 { "21" } else { "24" }, "-b:v", "0"]),
                Hw::Qsv => push(a, &["-preset", "medium", "-global_quality", if codec == Codec::H264 { "22" } else { "25" }]),
                Hw::Amf => push(a, &["-quality", "balanced", "-rc", "cqp", "-qp_i", "21", "-qp_p", "23", "-qp_b", "25"]),
            }
            push(a, &["-pix_fmt", if h == Hw::Nvenc { "yuv420p" } else { "nv12" }]);
        }
        None => match codec {
            Codec::H264 => push(a, &["-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p"]),
            Codec::Hevc => push(a, &["-c:v", "libx265", "-preset", "medium", "-crf", "23", "-x265-params", "log-level=error", "-pix_fmt", "yuv420p"]),
        },
    }
    if codec == Codec::Hevc {
        push(a, &["-tag:v", "hvc1"]);
    }
    if codec == Codec::H264 {
        push(a, &["-profile:v", "high"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Media, TransKind, Transition, tests::sample};

    fn with_paths(mut p: Project) -> Project {
        for m in &mut p.media {
            m.path = PathBuf::from(format!(r"C:\m\{}.bin", m.id));
        }
        p
    }

    fn file(codec: Codec, hw: Option<Hw>) -> Target {
        Target::File { path: PathBuf::from(r"C:\out\a.mp4"), codec, hw }
    }

    #[test]
    fn sizes() {
        assert_eq!(out_size([16, 9], 1080), (1920, 1080));
        assert_eq!(out_size([9, 16], 1080), (1080, 1920));
        assert_eq!(out_size([1, 1], 720), (720, 720));
        assert_eq!(out_size([16, 9], 2160), (3840, 2160));
        assert_eq!(out_size([4, 5], 1080), (1080, 1350));
    }

    #[test]
    fn placing() {
        let mut c = Clip::default();
        // 16:9 in 9:16: fits the width
        let pl = place(&c, 1920.0, 1080.0, 1080.0, 1920.0);
        assert_eq!((pl.w, pl.h.round(), pl.cx, pl.cy), (1080.0, 608.0, 540.0, 960.0));
        c.scale = 2.0;
        c.x = 0.25;
        c.crop = [0.25, 0.0, 0.25, 0.5];
        let pl = place(&c, 1920.0, 1080.0, 1920.0, 1080.0);
        assert_eq!((pl.full_w, pl.w, pl.h, pl.cx), (3840.0, 1920.0, 1080.0, 1440.0));
        c.crop = [0.9, 0.0, 0.9, 0.0];
        let [l, _, r, _] = crop_of(&c);
        assert!((l + r - 0.95).abs() < 1e-9);
    }

    #[test]
    fn numbers_and_tempo() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(2.5), "2.5");
        assert_eq!(n(1.0 / 30.0), "0.033333");
        assert_eq!(n(-0.0000001), "0");
        assert_eq!(atempo(1.0), Vec::<String>::new());
        assert_eq!(atempo(4.0), vec!["atempo=2", "atempo=2"]);
        assert_eq!(atempo(0.25), vec!["atempo=0.5", "atempo=0.5"]);
        assert_eq!(atempo(3.0), vec!["atempo=2", "atempo=1.5"]);
        assert_eq!(hex_color("#FF8800"), "0xff8800");
        assert_eq!(hex_color("red"), "0xffffff");
    }

    #[test]
    fn three_clips_text_crossfade_music() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        let b = p.append(100).unwrap();
        let _i = p.append(101).unwrap();
        p.clip_mut(a).unwrap().src_in = 2.0;
        p.trim_end(a, 150);
        p.move_clips(&[b, _i], -150, 0).unwrap();
        // b at twice the speed: 150 frames long, the picture moved back against it
        p.set_speed(b, 2.0);
        p.move_clips(&[_i], -150, 0).unwrap();
        p.set_transition(b, Some(Transition { kind: TransKind::Crossfade, len: 20 })).unwrap();
        p.clip_mut(b).unwrap().brightness = 0.1;
        p.clip_mut(b).unwrap().rotation = 90.0;
        p.clip_mut(b).unwrap().opacity = 0.5;
        let t = p.add_text(30, TextStyle { text: "Hello, world: 100%".into(), bg_opacity: 0.5, anim_in: Anim::Rise, ..TextStyle::default() });
        p.clip_mut(t).unwrap().y = 0.3;
        let song = p.add_music(102, 0).unwrap();
        p.trim_end(song, 390);
        p.duck = true;
        let p = with_paths(p);
        let plan = plan(&p, 1080, 1920, &file(Codec::H264, None));
        let g = &plan.graph;
        // inputs: a (seeked to its in point), b (reads 10 frames early for the crossfade), the picture, the music
        let joined = plan.args.join(" ");
        assert!(joined.contains(r"-ss 2 -t 5.833333 -i C:\m\100.bin"), "{joined}");
        assert!(joined.contains(r"-ss 0 -t 10.5 -i C:\m\100.bin"), "{joined}");
        assert!(joined.contains(r"-loop 1 -framerate 30 -t 3.5 -i C:\m\101.bin"), "{joined}");
        assert!(joined.contains(r"-i C:\m\102.bin"));
        assert!(joined.ends_with(r"-r 30 -t 13 -movflags +faststart C:\out\a.mp4"), "{joined}");
        assert!(joined.contains("-c:v libx264") && joined.contains("-c:a aac"));
        assert!(joined.contains("-/filter_complex graph.txt"));
        // b: handle before its start, missing media padded, speed, colour, opacity, rotation, crossfade
        assert!(g.contains("color=c=black:s=1080x1920:r=30:d=13"), "{g}");
        assert!(g.contains("setpts=(PTS-STARTPTS)/2"));
        assert!(g.contains("tpad=start_duration=0.333333:start_mode=clone"), "{g}");
        assert!(g.contains("eq=brightness=0.1:contrast=1:saturation=1"));
        assert!(g.contains("colorchannelmixer=aa=0.5"));
        assert!(g.contains("rotate=1.570796:c=none:ow=rotw(1.570796):oh=roth(1.570796)"));
        assert!(g.contains("setpts=PTS-STARTPTS+4.666667/TB,fade=t=in:st=4.666667:d=0.666667:alpha=1"), "{g}");
        // the 16:9 clip fits the width of the 9:16 frame
        assert!(g.contains("scale=1080:608"));
        assert!(g.contains("overlay=x=540-overlay_w/2:y=960-overlay_h/2:eof_action=pass"));
        // the text: from its file, risen in, box, at its time; written to the run's folder
        assert!(g.contains("drawtext=fontfile=f0.ttf:textfile="), "{g}");
        assert!(g.contains(":box=1:boxcolor=0x000000@0.5"));
        assert!(g.contains("enable='between(t,1,3.983333)'"), "{g}");
        assert!(g.contains("y='(h-text_h)/2+576+76.8*(1-min(1,max(0,(t-1)/0.4)))"), "{g}");
        assert_eq!(plan.texts.len(), 1);
        assert_eq!(plan.texts[0].1, "Hello, world: 100%");
        assert!(plan.fonts[0].1.to_string_lossy().to_lowercase().ends_with("segoeuib.ttf"));
        // sounds: a and b fade across the cut, the music is lowered under them
        assert!(g.contains("atempo=2"));
        assert!(g.contains("afade=t=out:st=4.666667:d=0.666667"), "{g}");
        assert!(g.contains("sidechaincompress"));
        assert!(g.contains("[sil][vk][duck]amix=inputs=3:duration=first"));
        assert!((plan.duration - 13.0).abs() < 1e-9);
    }

    #[test]
    fn hardware_and_hevc() {
        let mut p = sample();
        p.append(100).unwrap();
        let p = with_paths(p);
        let j = plan(&p, 1920, 1080, &file(Codec::Hevc, Some(Hw::Nvenc))).args.join(" ");
        assert!(j.contains("-c:v hevc_nvenc") && j.contains("-tag:v hvc1"));
        let j = plan(&p, 1920, 1080, &file(Codec::Hevc, None)).args.join(" ");
        assert!(j.contains("-c:v libx265") && j.contains("-crf 23"));
        let j = plan(&p, 1920, 1080, &file(Codec::H264, Some(Hw::Qsv))).args.join(" ");
        assert!(j.contains("-c:v h264_qsv") && j.contains("-pix_fmt nv12"));
    }

    #[test]
    fn preview_plans_and_muted_or_hidden_tracks() {
        let mut p = sample();
        p.append(100).unwrap();
        p.append(102).unwrap();
        let p = with_paths(p);
        let v = plan(&p, 640, 360, &Target::Frames);
        assert!(v.args.join(" ").ends_with("-f rawvideo -pix_fmt bgra pipe:1"));
        assert!(!v.graph.contains("[aout]"));
        assert!(v.graph.contains("format=bgra[vout]"));
        let a = plan(&p, 640, 360, &Target::Sound);
        assert!(!a.graph.contains("[vout]"));
        assert!(a.args.join(" ").contains("-f s16le -ar 48000 -ac 2 pipe:1"));
        // the video clip's sound and the song
        assert_eq!(a.graph.matches("[1:a]").count() + a.graph.matches("[0:a]").count(), 2, "{}", a.graph);
        let mut q = p.clone();
        let main = q.first_track(TrackKind::Video).unwrap();
        q.tracks[main].muted = true;
        q.tracks[main].hidden = true;
        let g = plan(&q, 640, 360, &file(Codec::H264, None)).graph;
        assert!(!g.contains("overlay=") && g.matches(":a]").count() == 1, "{g}");
        // a missing file is left out instead of failing the whole export
        let mut r = p.clone();
        r.media[0] = Media { missing: true, ..r.media[0].clone() };
        assert!(!plan(&r, 640, 360, &Target::Frames).args.join(" ").contains("100.bin"));
    }

    #[test]
    fn dip_on_main_track_goes_through_black() {
        let mut p = sample();
        let _a = p.append(101).unwrap();
        let b = p.append(101).unwrap();
        p.set_transition(b, Some(Transition { kind: TransKind::Dip, len: 30 })).unwrap();
        let g = plan(&with_paths(p), 640, 360, &Target::Frames).graph;
        assert!(g.contains("fade=t=out:st=2.5:d=0.5[v1]") || g.contains("fade=t=out:st=2.5:d=0.5"), "{g}");
        assert!(g.contains("fade=t=in:st=3:d=0.5"), "{g}");
        assert!(!g.contains("alpha=1"));
    }

    #[test]
    fn text_motion_matches() {
        let st = TextStyle { anim_in: Anim::Slide, anim_out: Anim::Rise, anim_len: 0.5, ..TextStyle::default() };
        let (dx, dy, a) = text_motion(&st, 1.0, 1.0, 4.0);
        assert!((dx + SLIDE).abs() < 1e-9 && dy == 0.0 && a == 0.0);
        let (dx, dy, a) = text_motion(&st, 2.0, 1.0, 4.0);
        assert!(dx == 0.0 && dy == 0.0 && a == 1.0);
        let (_, dy, a) = text_motion(&st, 3.75, 1.0, 4.0);
        assert!((dy + RISE * 0.5).abs() < 1e-9 && (a - 0.5).abs() < 1e-9);
    }
}
