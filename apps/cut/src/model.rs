//! The edit itself: the media of a project, its tracks and their clips, and every change made to them (add, move,
//! trim, split, delete, ripple delete, speed, transitions). Timeline positions are whole frames at the project's
//! frame rate, so a cut always falls on a frame; source positions are seconds in the media file.
//!
//! Nothing here touches the screen or FFmpeg: the window, the preview and the export all read the same model.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type Id = u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    #[default]
    Video,
    Image,
    Audio,
}

/// A file imported into the project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Media {
    pub id: Id,
    /// Where the file is (absolute, resolved when the project is opened).
    #[serde(skip)]
    pub path: PathBuf,
    /// Saved: the path relative to the project file, and the absolute one as a fallback.
    pub rel: String,
    pub abs: String,
    pub kind: MediaKind,
    /// Seconds (0 for a still image: it lasts as long as its clip).
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    /// The file could not be found when the project was opened.
    #[serde(skip)]
    pub missing: bool,
}

impl Media {
    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.abs.clone())
    }
    pub fn visual(&self) -> bool {
        self.kind != MediaKind::Audio
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    /// The main track: the pictures everything else sits on.
    #[default]
    Video,
    /// Pictures over the main track (picture in picture, logos, B-roll).
    Overlay,
    Text,
    /// Voices, sounds.
    Audio,
    /// The music bed (can be lowered under the other sounds).
    Music,
}

impl TrackKind {
    pub fn visual(self) -> bool {
        matches!(self, TrackKind::Video | TrackKind::Overlay | TrackKind::Text)
    }
    pub fn sound(self) -> bool {
        matches!(self, TrackKind::Audio | TrackKind::Music)
    }
    /// Can a clip of this media (None: a text) sit on this track?
    pub fn accepts(self, media: Option<MediaKind>) -> bool {
        matches!(
            (self, media),
            (TrackKind::Text, None)
                | (TrackKind::Video | TrackKind::Overlay, Some(MediaKind::Video | MediaKind::Image))
                | (TrackKind::Audio | TrackKind::Music, Some(MediaKind::Audio))
        )
    }
    /// Tracks a clip may be dragged between.
    pub fn family(self) -> u8 {
        match self {
            TrackKind::Video | TrackKind::Overlay => 0,
            TrackKind::Text => 1,
            TrackKind::Audio | TrackKind::Music => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anim {
    #[default]
    None,
    Fade,
    /// Fades while moving up a little.
    Rise,
    /// Fades while sliding in from the left (out: to the right).
    Slide,
}

impl Anim {
    pub const ALL: [Anim; 4] = [Anim::None, Anim::Fade, Anim::Rise, Anim::Slide];
}

/// What a text clip shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub text: String,
    pub font: String,
    pub bold: bool,
    /// Height of the letters as a part of the frame's height.
    pub size: f64,
    /// "#rrggbb".
    pub color: String,
    pub bg: String,
    /// 0: no box behind the text.
    pub bg_opacity: f64,
    pub anim_in: Anim,
    pub anim_out: Anim,
    /// Seconds each animation lasts.
    pub anim_len: f64,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            text: "Title".into(),
            font: "Segoe UI".into(),
            bold: true,
            size: 0.08,
            color: "#ffffff".into(),
            bg: "#000000".into(),
            bg_opacity: 0.0,
            anim_in: Anim::Fade,
            anim_out: Anim::Fade,
            anim_len: 0.4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransKind {
    #[default]
    Crossfade,
    /// To black, then from black.
    Dip,
}

/// The way into a clip from the one just before it on the same track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransKind,
    /// Frames.
    pub len: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub id: Id,
    /// The media shown or heard; None for a text.
    pub media: Option<Id>,
    /// Timeline frames.
    pub start: i64,
    pub len: i64,
    /// Seconds into the media where the clip begins.
    pub src_in: f64,
    /// 2.0 plays twice as fast.
    pub speed: f64,
    /// 1.0 as recorded, up to 2.0.
    pub volume: f64,
    /// Sound fades, seconds.
    pub fade_in: f64,
    pub fade_out: f64,
    pub opacity: f64,
    /// Centre offset from the frame's centre, as parts of the frame's width and height.
    pub x: f64,
    pub y: f64,
    /// 1.0: the picture fits the frame.
    pub scale: f64,
    /// Degrees, clockwise.
    pub rotation: f64,
    /// Parts cut from the left, top, right and bottom of the picture.
    pub crop: [f64; 4],
    /// -1..1, 0 unchanged.
    pub brightness: f64,
    /// 0..2, 1 unchanged.
    pub contrast: f64,
    /// 0..2, 1 unchanged.
    pub saturation: f64,
    pub text: Option<TextStyle>,
    pub trans: Option<Transition>,
}

impl Default for Clip {
    fn default() -> Self {
        Clip {
            id: 0,
            media: None,
            start: 0,
            len: 1,
            src_in: 0.0,
            speed: 1.0,
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            crop: [0.0; 4],
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            text: None,
            trans: None,
        }
    }
}

impl Clip {
    pub fn end(&self) -> i64 {
        self.start + self.len
    }
    /// Seconds of media the clip uses.
    pub fn src_len(&self, fps: f64) -> f64 {
        self.len as f64 / fps * self.speed
    }
    /// Seconds into the media at a timeline frame.
    pub fn src_at(&self, frame: i64, fps: f64) -> f64 {
        self.src_in + (frame - self.start) as f64 / fps * self.speed
    }
    pub fn has_color(&self) -> bool {
        self.brightness.abs() > 1e-4 || (self.contrast - 1.0).abs() > 1e-4 || (self.saturation - 1.0).abs() > 1e-4
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub id: Id,
    pub kind: TrackKind,
    /// Sound off (audio tracks and the sound of video clips).
    pub muted: bool,
    /// Pictures off.
    pub hidden: bool,
    /// Sorted by start, never overlapping.
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn end(&self) -> i64 {
        self.clips.iter().map(Clip::end).max().unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub fps: u32,
    /// Width : height of the frame.
    pub aspect: [u32; 2],
    pub media: Vec<Media>,
    /// Top to bottom as the timeline shows them; pictures stack from the bottom up.
    pub tracks: Vec<Track>,
    pub next_id: Id,
    /// Lower the music while other sounds play.
    pub duck: bool,
}

impl Default for Project {
    fn default() -> Self {
        let kinds = [TrackKind::Text, TrackKind::Overlay, TrackKind::Video, TrackKind::Audio, TrackKind::Music];
        Project {
            version: 1,
            name: "Untitled".into(),
            fps: 30,
            aspect: [16, 9],
            media: Vec::new(),
            tracks: kinds.iter().enumerate().map(|(i, k)| Track { id: i as Id + 1, kind: *k, ..Track::default() }).collect(),
            next_id: kinds.len() as Id + 1,
            duck: false,
        }
    }
}

/// Why an edit was refused (shown to the user).
pub type Refused = &'static str;
pub const NO_ROOM: Refused = "No room there";

/// Where a clip of a visual track can be seen, transitions included.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Span {
    /// Frames it is on screen (or heard), handles included.
    pub from: i64,
    pub to: i64,
    /// Crossfade into this clip: (start, frames) of its fade from transparent.
    pub alpha_in: Option<(i64, i64)>,
    /// Crossfade out of this clip (the next one fades in over it): (start, frames), for its sound.
    pub xfade_out: Option<(i64, i64)>,
    /// Dip: from black at its start, to black at its end.
    pub black_in: Option<(i64, i64)>,
    pub black_out: Option<(i64, i64)>,
}

/// One picture to draw at a frame, bottom first.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub track: usize,
    pub clip: Id,
    /// Seconds into the media (clamped to it).
    pub src: f64,
    /// 0..1 from the transitions (times the clip's own opacity when drawn).
    pub alpha: f64,
    /// 0..1 of black over it (dip).
    pub black: f64,
}

/// The nearest point within `thr` frames of `v`.
pub fn snap(v: i64, points: &[i64], thr: i64) -> Option<i64> {
    points.iter().copied().filter(|p| (p - v).abs() <= thr).min_by_key(|p| (p - v).abs())
}

fn halves(len: i64) -> (i64, i64) {
    let a = len / 2;
    (a, len - a)
}

impl Project {
    pub fn fps(&self) -> f64 {
        self.fps.max(1) as f64
    }

    pub fn new_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn media(&self, id: Id) -> Option<&Media> {
        self.media.iter().find(|m| m.id == id)
    }

    pub fn media_of(&self, clip: &Clip) -> Option<&Media> {
        clip.media.and_then(|m| self.media(m))
    }

    /// (track, index) of a clip.
    pub fn find(&self, id: Id) -> Option<(usize, usize)> {
        self.tracks.iter().enumerate().find_map(|(t, tr)| tr.clips.iter().position(|c| c.id == id).map(|i| (t, i)))
    }

    pub fn clip(&self, id: Id) -> Option<&Clip> {
        self.find(id).map(|(t, i)| &self.tracks[t].clips[i])
    }

    pub fn clip_mut(&mut self, id: Id) -> Option<&mut Clip> {
        self.find(id).map(|(t, i)| &mut self.tracks[t].clips[i])
    }

    pub fn track_of(&self, id: Id) -> Option<&Track> {
        self.find(id).map(|(t, _)| &self.tracks[t])
    }

    /// Frames from the start to the end of the last clip.
    pub fn duration(&self) -> i64 {
        self.tracks.iter().map(Track::end).max().unwrap_or(0)
    }

    pub fn first_track(&self, kind: TrackKind) -> Option<usize> {
        self.tracks.iter().position(|t| t.kind == kind)
    }

    pub fn all_ids(&self) -> Vec<Id> {
        self.tracks.iter().flat_map(|t| t.clips.iter().map(|c| c.id)).collect()
    }

    fn sort(&mut self) {
        for t in &mut self.tracks {
            t.clips.sort_by_key(|c| c.start);
        }
    }

    /// No clip of the track (other than `ignore`) overlaps [start, start + len).
    pub fn fits(&self, track: usize, start: i64, len: i64, ignore: &[Id]) -> bool {
        start >= 0
            && len > 0
            && self.tracks.get(track).is_some_and(|t| t.clips.iter().all(|c| ignore.contains(&c.id) || c.end() <= start || c.start >= start + len))
    }

    /// Frames a clip of this media lasts when it is first placed.
    pub fn natural_len(&self, media: &Media) -> i64 {
        match media.kind {
            MediaKind::Image => 3 * self.fps as i64,
            _ => ((media.duration * self.fps()).floor() as i64).max(1),
        }
    }

    /// A clip of `media` on `track` at `start`.
    pub fn add_media(&mut self, media: Id, track: usize, start: i64) -> Result<Id, Refused> {
        let m = self.media(media).ok_or("Unknown media")?.clone();
        let tk = self.tracks.get(track).ok_or("Unknown track")?.kind;
        if !tk.accepts(Some(m.kind)) {
            return Err("That track does not take this kind of file");
        }
        let len = self.natural_len(&m);
        if !self.fits(track, start, len, &[]) {
            return Err(NO_ROOM);
        }
        let id = self.new_id();
        self.tracks[track].clips.push(Clip { id, media: Some(media), start, len, ..Clip::default() });
        self.sort();
        Ok(id)
    }

    /// The track a media goes on by default: the main track, or the first sound track.
    pub fn home_track(&self, kind: MediaKind) -> Option<usize> {
        match kind {
            MediaKind::Audio => self.first_track(TrackKind::Audio).or_else(|| self.first_track(TrackKind::Music)),
            _ => self.first_track(TrackKind::Video),
        }
    }

    /// At the end of its home track.
    pub fn append(&mut self, media: Id) -> Result<Id, Refused> {
        let kind = self.media(media).ok_or("Unknown media")?.kind;
        let track = self.home_track(kind).ok_or("No track for it")?;
        let at = self.tracks[track].end();
        self.add_media(media, track, at)
    }

    /// Inserts a track of `kind` at `index` (top to bottom order).
    pub fn insert_track(&mut self, index: usize, kind: TrackKind) -> usize {
        let id = self.new_id();
        let i = index.min(self.tracks.len());
        self.tracks.insert(i, Track { id, kind, ..Track::default() });
        i
    }

    /// A text clip at `at`: on a text track with room, or on a new one above the others.
    pub fn add_text(&mut self, at: i64, style: TextStyle) -> Id {
        let len = 3 * self.fps as i64;
        let track = (0..self.tracks.len()).find(|&t| self.tracks[t].kind == TrackKind::Text && self.fits(t, at, len, &[]));
        let track = match track {
            Some(t) => t,
            None => self.insert_track(0, TrackKind::Text),
        };
        let id = self.new_id();
        self.tracks[track].clips.push(Clip { id, start: at.max(0), len, text: Some(style), ..Clip::default() });
        self.sort();
        id
    }

    /// A music file on the music track (a new one when the existing are busy there).
    pub fn add_music(&mut self, media: Id, at: i64) -> Result<Id, Refused> {
        let m = self.media(media).ok_or("Unknown media")?.clone();
        if m.kind != MediaKind::Audio {
            return Err("That is not a sound file");
        }
        let len = self.natural_len(&m);
        let track = (0..self.tracks.len()).find(|&t| self.tracks[t].kind == TrackKind::Music && self.fits(t, at, len, &[]));
        let track = match track {
            Some(t) => t,
            None => self.insert_track(self.tracks.len(), TrackKind::Music),
        };
        self.add_media(media, track, at)
    }

    /// Cuts the clips under `at` in two (only `among` when given). Returns the ids of the new right halves.
    pub fn split(&mut self, at: i64, among: Option<&[Id]>) -> Vec<Id> {
        let fps = self.fps();
        let mut cuts = Vec::new();
        for t in 0..self.tracks.len() {
            for i in 0..self.tracks[t].clips.len() {
                let c = &self.tracks[t].clips[i];
                if c.start < at && at < c.end() && among.is_none_or(|ids| ids.contains(&c.id)) {
                    cuts.push((t, c.id));
                }
            }
        }
        let mut made = Vec::new();
        for (t, id) in cuts {
            let new_id = self.new_id();
            let tr = &mut self.tracks[t];
            let Some(i) = tr.clips.iter().position(|c| c.id == id) else { continue };
            let left = &mut tr.clips[i];
            let mut right = left.clone();
            let cut = at - left.start;
            right.id = new_id;
            right.start = at;
            right.len = left.len - cut;
            right.src_in = left.src_in + cut as f64 / fps * left.speed;
            right.trans = None;
            right.fade_in = 0.0;
            left.len = cut;
            left.fade_out = 0.0;
            if let Some(tx) = &mut left.trans {
                tx.len = tx.len.min(cut);
            }
            tr.clips.insert(i + 1, right);
            made.push(new_id);
        }
        made
    }

    /// How far the media allows a clip to reach: (earliest start, latest end) in frames, from its source alone.
    fn source_bounds(&self, c: &Clip) -> (i64, i64) {
        let fps = self.fps();
        match self.media_of(c) {
            Some(m) if m.kind != MediaKind::Image && m.duration > 0.0 => {
                let before = (c.src_in / c.speed * fps).floor() as i64;
                let after = ((m.duration - c.src_in) / c.speed * fps).floor() as i64;
                (c.start - before, c.start + after.max(1))
            }
            _ => (i64::MIN / 4, i64::MAX / 4),
        }
    }

    /// Neighbours on its track: (end of the one before, start of the one after).
    fn room(&self, id: Id) -> Option<(i64, i64)> {
        let (t, i) = self.find(id)?;
        let tr = &self.tracks[t];
        let before = if i > 0 { tr.clips[i - 1].end() } else { 0 };
        let after = tr.clips.get(i + 1).map_or(i64::MAX / 4, |c| c.start);
        Some((before, after))
    }

    /// Moves the left edge of a clip to `new_start` (as near as its neighbours and its media allow). Returns where
    /// it went.
    pub fn trim_start(&mut self, id: Id, new_start: i64) -> Option<i64> {
        let (before, _) = self.room(id)?;
        let c = self.clip(id)?.clone();
        let (src_min, _) = self.source_bounds(&c);
        let s = new_start.max(before).max(src_min).max(0).min(c.end() - 1);
        let fps = self.fps();
        let c = self.clip_mut(id)?;
        c.src_in = (c.src_in + (s - c.start) as f64 / fps * c.speed).max(0.0);
        c.len = c.end() - s;
        c.start = s;
        if let Some(tx) = &mut c.trans {
            tx.len = tx.len.min(c.len);
        }
        Some(s)
    }

    /// Moves the right edge of a clip to `new_end`. Returns where it went.
    pub fn trim_end(&mut self, id: Id, new_end: i64) -> Option<i64> {
        let (_, after) = self.room(id)?;
        let c = self.clip(id)?.clone();
        let (_, src_max) = self.source_bounds(&c);
        let e = new_end.min(after).min(src_max).max(c.start + 1);
        self.clip_mut(id)?.len = e - c.start;
        Some(e)
    }

    /// Moves clips by `delta` frames and `shift` tracks (all or nothing). Clips keep their spacing; a clip only
    /// goes to a track of its family.
    pub fn move_clips(&mut self, ids: &[Id], delta: i64, shift: i32) -> Result<(), Refused> {
        let moving: Vec<(usize, Clip)> = ids.iter().filter_map(|id| self.find(*id)).map(|(t, i)| (t, self.tracks[t].clips[i].clone())).collect();
        if moving.is_empty() {
            return Ok(());
        }
        let min_start = moving.iter().map(|(_, c)| c.start).min().unwrap_or(0);
        let delta = delta.max(-min_start);
        let mut placed: Vec<(usize, Clip)> = Vec::new();
        for (t, c) in &moving {
            let nt = *t as i64 + shift as i64;
            if nt < 0 || nt >= self.tracks.len() as i64 {
                return Err(NO_ROOM);
            }
            let nt = nt as usize;
            let kind = self.tracks[nt].kind;
            if kind.family() != self.tracks[*t].kind.family() || !kind.accepts(self.media_of(c).map(|m| m.kind)) {
                return Err("That track does not take this kind of clip");
            }
            let mut c = c.clone();
            c.start += delta;
            if !self.fits(nt, c.start, c.len, ids) || placed.iter().any(|(pt, p)| *pt == nt && p.start < c.end() && c.start < p.end()) {
                return Err(NO_ROOM);
            }
            placed.push((nt, c));
        }
        for tr in &mut self.tracks {
            tr.clips.retain(|c| !ids.contains(&c.id));
        }
        for (t, c) in placed {
            self.tracks[t].clips.push(c);
        }
        self.sort();
        self.drop_lost_transitions();
        Ok(())
    }

    pub fn delete(&mut self, ids: &[Id]) {
        for tr in &mut self.tracks {
            tr.clips.retain(|c| !ids.contains(&c.id));
        }
        self.drop_lost_transitions();
    }

    /// Deletes the clips and closes the gaps they leave on their tracks.
    pub fn ripple_delete(&mut self, ids: &[Id]) {
        for tr in &mut self.tracks {
            let gone: Vec<(i64, i64)> = tr.clips.iter().filter(|c| ids.contains(&c.id)).map(|c| (c.start, c.len)).collect();
            if gone.is_empty() {
                continue;
            }
            tr.clips.retain(|c| !ids.contains(&c.id));
            for c in &mut tr.clips {
                let shift: i64 = gone.iter().filter(|(s, _)| *s < c.start).map(|(_, l)| l).sum();
                c.start -= shift;
            }
        }
        self.drop_lost_transitions();
    }

    /// Closes the empty space under `at` on a track (the gap the clip after it leaves).
    pub fn close_gap(&mut self, track: usize, at: i64) -> bool {
        let Some(tr) = self.tracks.get_mut(track) else { return false };
        let before = tr.clips.iter().filter(|c| c.end() <= at).map(Clip::end).max().unwrap_or(0);
        let Some(next) = tr.clips.iter().filter(|c| c.start >= at).map(|c| c.start).min() else { return false };
        let gap = next - before;
        if gap <= 0 {
            return false;
        }
        for c in &mut tr.clips {
            if c.start >= next {
                c.start -= gap;
            }
        }
        true
    }

    /// New speed for a clip: it keeps the same stretch of media, so it gets shorter or longer (up to the next clip).
    pub fn set_speed(&mut self, id: Id, speed: f64) -> Option<()> {
        let speed = speed.clamp(0.1, 10.0);
        let (_, after) = self.room(id)?;
        let fps = self.fps();
        let c = self.clip_mut(id)?;
        let src = c.src_len(fps);
        c.speed = speed;
        c.len = ((src / speed * fps).round() as i64).clamp(1, (after - c.start).max(1));
        Some(())
    }

    /// The clip right before this one on its track, touching it (a transition needs one).
    pub fn prev_adjacent(&self, id: Id) -> Option<Id> {
        let (t, i) = self.find(id)?;
        let c = &self.tracks[t].clips[i];
        (i > 0).then(|| &self.tracks[t].clips[i - 1]).filter(|p| p.end() == c.start).map(|p| p.id)
    }

    /// Sets (or removes) the transition into a clip. Its length is kept to what both clips can give.
    pub fn set_transition(&mut self, id: Id, t: Option<Transition>) -> Result<(), Refused> {
        let Some(mut t) = t else {
            if let Some(c) = self.clip_mut(id) {
                c.trans = None;
            }
            return Ok(());
        };
        let prev = self.prev_adjacent(id).ok_or("A transition needs a clip right before this one")?;
        let a = self.clip(prev).map_or(1, |c| c.len);
        let b = self.clip(id).map_or(1, |c| c.len);
        t.len = t.len.clamp(2, a.min(b).max(2));
        if let Some(c) = self.clip_mut(id) {
            c.trans = Some(t);
        }
        Ok(())
    }

    /// A transition stays only while its clip still touches the one before.
    fn drop_lost_transitions(&mut self) {
        let lost: Vec<Id> = self
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .filter(|c| c.trans.is_some())
            .map(|c| c.id)
            .filter(|id| self.prev_adjacent(*id).is_none())
            .collect();
        for id in lost {
            if let Some(c) = self.clip_mut(id) {
                c.trans = None;
            }
        }
    }

    /// Frames that clip edges stick to: 0, the playhead and the edges of the other clips.
    pub fn snap_points(&self, exclude: &[Id], playhead: i64) -> Vec<i64> {
        let mut v = vec![0, playhead];
        for t in &self.tracks {
            for c in t.clips.iter().filter(|c| !exclude.contains(&c.id)) {
                v.push(c.start);
                v.push(c.end());
            }
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Where a clip shows, with its transitions (handles before and after included).
    pub fn span(&self, track: usize, index: usize) -> Span {
        let tr = &self.tracks[track];
        let c = &tr.clips[index];
        let mut s = Span { from: c.start, to: c.end(), ..Span::default() };
        if let Some(t) = c.trans.filter(|_| index > 0 && tr.clips[index - 1].end() == c.start) {
            let (h1, h2) = halves(t.len);
            match t.kind {
                TransKind::Crossfade => {
                    s.from = c.start - h1;
                    s.alpha_in = Some((c.start - h1, t.len));
                }
                TransKind::Dip => s.black_in = Some((c.start, h2)),
            }
        }
        if let Some(n) = tr.clips.get(index + 1).filter(|n| n.start == c.end())
            && let Some(t) = n.trans
        {
            let (h1, h2) = halves(t.len);
            match t.kind {
                TransKind::Crossfade => {
                    s.to = c.end() + h2;
                    s.xfade_out = Some((c.end() - h1, t.len));
                }
                TransKind::Dip => s.black_out = Some((c.end() - h1, h1)),
            }
        }
        s
    }

    /// The pictures at a frame, bottom first (texts included, with no source time).
    pub fn layers(&self, frame: i64) -> Vec<Layer> {
        let fps = self.fps();
        let mut out = Vec::new();
        for t in (0..self.tracks.len()).rev() {
            let tr = &self.tracks[t];
            if !tr.kind.visual() || tr.hidden {
                continue;
            }
            for i in 0..tr.clips.len() {
                let s = self.span(t, i);
                if frame < s.from || frame >= s.to {
                    continue;
                }
                let c = &tr.clips[i];
                // 0 → 1 across a window of frames
                let ramp = |(a, n): (i64, i64)| (((frame - a) as f64 + 0.5) / n.max(1) as f64).clamp(0.0, 1.0);
                let alpha = s.alpha_in.map_or(1.0, ramp);
                let black_in = s.black_in.map_or(0.0, |w| 1.0 - ramp(w));
                let black_out = s.black_out.map_or(0.0, ramp);
                let mut src = c.src_at(frame, fps);
                if let Some(m) = self.media_of(c) {
                    if m.kind == MediaKind::Image {
                        src = 0.0;
                    } else if m.duration > 0.0 {
                        src = src.clamp(0.0, (m.duration - 1.0 / fps.max(m.fps)).max(0.0));
                    }
                }
                out.push(Layer { track: t, clip: c.id, src, alpha, black: black_in.max(black_out) });
            }
        }
        out
    }

    /// The project between two frames, as if it started at `from` (for playing the sound from the playhead).
    pub fn sub_range(&self, from: i64, to: i64) -> Project {
        let fps = self.fps();
        let mut p = self.clone();
        for tr in &mut p.tracks {
            tr.clips.retain(|c| c.end() > from && c.start < to);
            for c in &mut tr.clips {
                if c.start < from {
                    let cut = from - c.start;
                    c.src_in += cut as f64 / fps * c.speed;
                    c.len -= cut;
                    c.start = from;
                    c.trans = None;
                    c.fade_in = 0.0;
                }
                if c.end() > to {
                    c.len = to - c.start;
                    c.fade_out = 0.0;
                }
                c.start -= from;
            }
        }
        p.tracks.iter_mut().for_each(|t| t.clips.retain(|c| c.len > 0));
        p
    }

    /// A new frame rate: every position and length is converted (clips keep their times, to the nearest frame).
    pub fn set_fps(&mut self, fps: u32) {
        let (old, new) = (self.fps.max(1) as f64, fps.max(1) as f64);
        if self.fps == fps {
            return;
        }
        for t in &mut self.tracks {
            for c in &mut t.clips {
                let end = (c.end() as f64 * new / old).round() as i64;
                c.start = (c.start as f64 * new / old).round() as i64;
                c.len = (end - c.start).max(1);
                if let Some(tx) = &mut c.trans {
                    tx.len = ((tx.len as f64 * new / old).round() as i64).max(2);
                }
            }
            // rounding may make neighbours touch differently: never overlap
            t.clips.sort_by_key(|c| c.start);
            for i in 1..t.clips.len() {
                let prev_end = t.clips[i - 1].end();
                if t.clips[i].start < prev_end {
                    t.clips[i - 1].len = (t.clips[i].start - t.clips[i - 1].start).max(1);
                }
            }
        }
        self.fps = fps;
    }

    pub fn ratio(&self) -> f64 {
        self.aspect[0].max(1) as f64 / self.aspect[1].max(1) as f64
    }
}

/// Every change, kept to go back and forth (whole copies: a project is small).
#[derive(Default)]
pub struct History {
    past: Vec<Project>,
    future: Vec<Project>,
}

const HISTORY: usize = 200;

impl History {
    /// Call before a change, with the project as it was.
    pub fn push(&mut self, before: &Project) {
        if self.past.last() == Some(before) {
            return;
        }
        self.past.push(before.clone());
        if self.past.len() > HISTORY {
            self.past.remove(0);
        }
        self.future.clear();
    }
    pub fn undo(&mut self, now: &mut Project) -> bool {
        match self.past.pop() {
            Some(p) => {
                self.future.push(std::mem::replace(now, p));
                true
            }
            None => false,
        }
    }
    pub fn redo(&mut self, now: &mut Project) -> bool {
        match self.future.pop() {
            Some(p) => {
                self.past.push(std::mem::replace(now, p));
                true
            }
            None => false,
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }
}

/// "00:01:05:12" (hours, minutes, seconds, frames).
pub fn timecode(frame: i64, fps: u32) -> String {
    let fps = fps.max(1) as i64;
    let f = frame.max(0);
    let (s, ff) = (f / fps, f % fps);
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60, ff)
}

/// "1:05" or "1:02:05" for a length in seconds.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as i64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A project with a 10 s video (id 100), a picture (101) and a 20 s song (102).
    pub fn sample() -> Project {
        let mut p = Project::default();
        p.media.push(Media { id: 100, kind: MediaKind::Video, duration: 10.0, width: 1920, height: 1080, fps: 30.0, has_audio: true, ..Media::default() });
        p.media.push(Media { id: 101, kind: MediaKind::Image, width: 800, height: 800, ..Media::default() });
        p.media.push(Media { id: 102, kind: MediaKind::Audio, duration: 20.0, has_audio: true, ..Media::default() });
        p.next_id = 200;
        p
    }

    fn main(p: &Project) -> usize {
        p.first_track(TrackKind::Video).unwrap()
    }

    #[test]
    fn append_and_place() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        let b = p.append(101).unwrap();
        assert_eq!(p.clip(a).unwrap().len, 300);
        assert_eq!(p.clip(b).unwrap().start, 300);
        assert_eq!(p.clip(b).unwrap().len, 90);
        let s = p.append(102).unwrap();
        assert_eq!(p.track_of(s).unwrap().kind, TrackKind::Audio);
        assert_eq!(p.duration(), 600);
        // overlap refused, wrong kind refused
        assert_eq!(p.add_media(100, main(&p), 100), Err(NO_ROOM));
        assert!(p.add_media(102, main(&p), 1000).is_err());
    }

    #[test]
    fn split_keeps_the_picture_in_place() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        p.clip_mut(a).unwrap().speed = 2.0;
        p.clip_mut(a).unwrap().len = 150;
        p.clip_mut(a).unwrap().fade_out = 1.0;
        let made = p.split(60, None);
        assert_eq!(made.len(), 1);
        let (l, r) = (p.clip(a).unwrap().clone(), p.clip(made[0]).unwrap().clone());
        assert_eq!((l.start, l.len, r.start, r.len), (0, 60, 60, 90));
        // 60 frames at 2x = 4 s of media
        assert!((r.src_in - 4.0).abs() < 1e-9);
        assert_eq!((l.fade_out, r.fade_out), (0.0, 1.0));
        // the same picture before and after the cut
        assert!((l.src_at(59, 30.0) + 2.0 / 30.0 - r.src_at(60, 30.0)).abs() < 1e-9);
        // nothing to cut on an edge
        assert!(p.split(60, None).is_empty());
        assert!(p.split(10, Some(&[made[0]])).is_empty());
    }

    #[test]
    fn trims_stop_at_neighbours_and_media() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        let b = p.append(100).unwrap();
        // the end cannot pass the media's 10 s nor the next clip
        assert_eq!(p.trim_end(a, 400), Some(300));
        assert_eq!(p.trim_end(a, 200), Some(200));
        assert_eq!(p.trim_end(a, 500), Some(300));
        // the start of b: forward takes media away, back cannot go before the media's start nor before a's end
        assert_eq!(p.trim_start(b, 330), Some(330));
        assert!((p.clip(b).unwrap().src_in - 1.0).abs() < 1e-9);
        assert_eq!(p.trim_start(b, 250), Some(300));
        assert_eq!(p.clip(b).unwrap().src_in, 0.0);
        // a clip is never less than a frame
        assert_eq!(p.trim_end(a, -5), Some(1));
        // a picture stretches as far as wanted
        let i = p.add_media(101, main(&p), 1000).unwrap();
        assert_eq!(p.trim_end(i, 5000), Some(5000));
        assert_eq!(p.trim_start(i, 700), Some(700));
    }

    #[test]
    fn ripple_delete_closes_gaps() {
        let mut p = sample();
        let a = p.append(101).unwrap();
        let b = p.append(101).unwrap();
        let c = p.append(101).unwrap();
        p.ripple_delete(&[a]);
        assert_eq!(p.clip(b).unwrap().start, 0);
        assert_eq!(p.clip(c).unwrap().start, 90);
        p.delete(&[b]);
        assert_eq!(p.clip(c).unwrap().start, 90);
        assert!(p.close_gap(main(&p), 10));
        assert_eq!(p.clip(c).unwrap().start, 0);
        // several at once
        let d = p.append(101).unwrap();
        let e = p.append(101).unwrap();
        p.ripple_delete(&[c, d]);
        assert_eq!(p.clip(e).unwrap().start, 0);
    }

    #[test]
    fn moving_clips() {
        let mut p = sample();
        let a = p.append(101).unwrap();
        let b = p.append(101).unwrap();
        assert_eq!(p.move_clips(&[a], 30, 0), Err(NO_ROOM));
        // both together keep their spacing; never before 0
        p.move_clips(&[a, b], -50, 0).unwrap();
        assert_eq!(p.clip(a).unwrap().start, 0);
        p.move_clips(&[a, b], 10, 0).unwrap();
        assert_eq!((p.clip(a).unwrap().start, p.clip(b).unwrap().start), (10, 100));
        // up to the overlay track; not onto text or sound
        p.move_clips(&[b], 0, -1).unwrap();
        assert_eq!(p.track_of(b).unwrap().kind, TrackKind::Overlay);
        assert!(p.move_clips(&[b], 0, -1).is_err());
        assert!(p.move_clips(&[a], 0, 1).is_err());
    }

    #[test]
    fn snapping() {
        let mut p = sample();
        let a = p.append(101).unwrap();
        let _b = p.add_media(101, main(&p), 200).unwrap();
        let pts = p.snap_points(&[a], 150);
        assert_eq!(pts, vec![0, 150, 200, 290]);
        assert_eq!(snap(196, &pts, 5), Some(200));
        assert_eq!(snap(146, &pts, 5), Some(150));
        assert_eq!(snap(175, &pts, 5), None);
        // nearest wins
        assert_eq!(snap(203, &[200, 205], 10), Some(205));
    }

    #[test]
    fn undo_redo() {
        let mut p = sample();
        let mut h = History::default();
        h.push(&p);
        let a = p.append(100).unwrap();
        h.push(&p);
        p.split(100, None);
        assert_eq!(p.tracks[main(&p)].clips.len(), 2);
        assert!(h.undo(&mut p));
        assert_eq!(p.tracks[main(&p)].clips.len(), 1);
        assert!(h.undo(&mut p));
        assert!(p.clip(a).is_none());
        assert!(!h.undo(&mut p));
        assert!(h.redo(&mut p) && h.redo(&mut p));
        assert_eq!(p.tracks[main(&p)].clips.len(), 2);
        assert!(!h.redo(&mut p));
        // a new change forgets what could be redone
        h.undo(&mut p);
        h.push(&p);
        p.delete(&[a]);
        assert!(!h.can_redo());
    }

    #[test]
    fn speed_changes_length() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        p.set_speed(a, 2.0);
        assert_eq!(p.clip(a).unwrap().len, 150);
        p.set_speed(a, 1.0);
        assert_eq!(p.clip(a).unwrap().len, 300);
        // up to the next clip only
        let _b = p.add_media(101, main(&p), 400).unwrap();
        p.set_speed(a, 0.5);
        assert_eq!(p.clip(a).unwrap().len, 400);
    }

    #[test]
    fn transitions_and_layers() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        let b = p.append(100).unwrap();
        assert!(p.set_transition(a, Some(Transition { kind: TransKind::Crossfade, len: 10 })).is_err());
        p.set_transition(b, Some(Transition { kind: TransKind::Crossfade, len: 10 })).unwrap();
        let t = main(&p);
        assert_eq!(p.span(t, 0), Span { from: 0, to: 305, xfade_out: Some((295, 10)), ..Span::default() });
        assert_eq!(p.span(t, 1), Span { from: 295, to: 600, alpha_in: Some((295, 10)), ..Span::default() });
        // in the middle of the crossfade: both, the new one half see-through
        let l = p.layers(300);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].clip, a);
        assert!((l[1].alpha - 0.55).abs() < 1e-9);
        // the handle of b reads before its in point, clamped to the media
        assert_eq!(l[1].src, 0.0);
        // dip: black at the cut
        p.set_transition(b, Some(Transition { kind: TransKind::Dip, len: 10 })).unwrap();
        let l = p.layers(299);
        assert_eq!(l.len(), 1);
        assert!(l[0].black > 0.8);
        // a moved clip loses its transition
        p.move_clips(&[b], 30, 0).unwrap();
        assert!(p.clip(b).unwrap().trans.is_none());
    }

    #[test]
    fn texts_find_room() {
        let mut p = sample();
        let a = p.add_text(0, TextStyle::default());
        let b = p.add_text(30, TextStyle::default());
        assert_ne!(p.find(a).unwrap().0, p.find(b).unwrap().0);
        assert_eq!(p.tracks.iter().filter(|t| t.kind == TrackKind::Text).count(), 2);
        let layers = p.layers(40);
        assert_eq!(layers.len(), 2);
    }

    #[test]
    fn sub_range_starts_at_zero() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        let _b = p.append(100).unwrap();
        let s = p.sub_range(150, 450);
        let t = main(&s);
        assert_eq!(s.tracks[t].clips.len(), 2);
        let c = s.clip(a).unwrap();
        assert_eq!((c.start, c.len), (0, 150));
        assert!((c.src_in - 5.0).abs() < 1e-9);
        assert_eq!(s.tracks[t].clips[1].len, 150);
        assert_eq!(s.duration(), 300);
    }

    #[test]
    fn frame_rate_change_keeps_times() {
        let mut p = sample();
        let a = p.append(100).unwrap();
        let b = p.append(101).unwrap();
        p.set_fps(60);
        assert_eq!((p.clip(a).unwrap().len, p.clip(b).unwrap().start, p.clip(b).unwrap().len), (600, 600, 180));
        p.set_fps(25);
        assert_eq!((p.clip(a).unwrap().len, p.clip(b).unwrap().start), (250, 250));
    }

    #[test]
    fn times() {
        assert_eq!(timecode(30 * 65 + 12, 30), "00:01:05:12");
        assert_eq!(clock(65.4), "1:05");
        assert_eq!(clock(3725.0), "1:02:05");
    }
}
