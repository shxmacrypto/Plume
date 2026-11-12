//! Plume Create's window: a start page (open a photo, start a design from a size or a template) and the editor
//! (tools on the left, the canvas, layer properties and the layers on the right).

pub mod bars;
pub mod canvas;
pub mod draw;
pub mod events;
pub mod home;
pub mod icons;
pub mod panel;
pub mod popups;
pub mod widgets;

use crate::{
    engine::{
        doc::{Adjust, Doc, Id, ImageL, Kind, Layer, Look, Paint, Rgba, ShapeKind, ShapeL, TextAlign, TextL, Xf},
        history::History,
        img::{IRect, Img},
        ops::{self, AlignTo},
        project,
        render::Renderer,
        select::Selection,
        templates,
    },
    i18n::{fill, tr},
    settings::Settings,
    text::DwText,
    wic::{self, Format},
};
use plume_ui::{Area, Canvas, Color, Cursor, Cx, Event, Handler, Hits, PaintCx, Pt, Rect, Theme, Window, m, pt, rect};
use std::{
    any::Any,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

pub const ACCENT_DARK: u32 = 0xee5fb5;
pub const ACCENT_LIGHT: u32 = 0xc0267f;

pub const RAIL_W: f32 = 60.0;
pub const PANEL_W: f32 = 312.0;
pub const OPTS_H: f32 = 48.0;
pub const STATUS_H: f32 = 34.0;
pub const RULER: f32 = 20.0;

/// Colours of the window, from plume-ui's theme with the app's accent.
#[derive(Clone)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color,
    pub side: Color,
    pub card: Color,
    pub line: Color,
    pub t1: Color,
    pub t2: Color,
    pub t3: Color,
    pub t4: Color,
    pub fill: Color,
    pub fill2: Color,
    pub hover: Color,
    pub solid: Color,
    pub solid2: Color,
    pub ring: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub ink: Color,
    pub close_hover: Color,
    /// Around the canvas.
    pub stage: Color,
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        let accent = Color::hex(if dark { ACCENT_DARK } else { ACCENT_LIGHT });
        let t = Theme::new(dark, Some(accent));
        Pal {
            dark,
            bg: t.bg,
            side: t.bg2,
            card: t.card,
            line: t.sep,
            t1: t.t1,
            t2: t.t2,
            t3: t.t3,
            t4: t.t4,
            fill: t.fill,
            fill2: t.fill2,
            hover: t.hover,
            solid: t.solid,
            solid2: t.solid2,
            ring: t.ring,
            accent,
            on_accent: Color::hex(0xffffff),
            ink: accent,
            close_hover: t.close_hover,
            stage: if dark { t.bg2.mix(Color::hex(0x000000), 0.25) } else { t.bg2 },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Move,
    Select,
    Crop,
    Brush,
    Eraser,
    Fill,
    Picker,
    Text,
    Shape,
    Clone,
    Cutout,
    Hand,
}

impl Tool {
    pub const ALL: [Tool; 12] = [
        Tool::Move,
        Tool::Select,
        Tool::Crop,
        Tool::Brush,
        Tool::Eraser,
        Tool::Fill,
        Tool::Picker,
        Tool::Text,
        Tool::Shape,
        Tool::Clone,
        Tool::Cutout,
        Tool::Hand,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => "Move and transform",
            Tool::Select => "Select",
            Tool::Crop => "Crop and straighten",
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Fill",
            Tool::Picker => "Eyedropper",
            Tool::Text => "Text",
            Tool::Shape => "Shapes",
            Tool::Clone => "Clone and heal",
            Tool::Cutout => "Remove background",
            Tool::Hand => "Hand",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Tool::Move => "V",
            Tool::Select => "M",
            Tool::Crop => "C",
            Tool::Brush => "B",
            Tool::Eraser => "E",
            Tool::Fill => "G",
            Tool::Picker => "I",
            Tool::Text => "T",
            Tool::Shape => "U",
            Tool::Clone => "S",
            Tool::Cutout => "K",
            Tool::Hand => "H",
        }
    }
    pub fn icon(self) -> &'static plume_ui::icons::Icon {
        match self {
            Tool::Move => &icons::POINTER,
            Tool::Select => &icons::MARQUEE,
            Tool::Crop => &icons::CROP,
            Tool::Brush => &icons::BRUSH,
            Tool::Eraser => &icons::ERASER,
            Tool::Fill => &icons::BUCKET,
            Tool::Picker => &icons::PICKER,
            Tool::Text => &icons::TEXT,
            Tool::Shape => &icons::SHAPES,
            Tool::Clone => &icons::STAMP,
            Tool::Cutout => &icons::CUTOUT,
            Tool::Hand => &icons::HAND,
        }
    }
    /// Tools that paint pixels with a round brush.
    pub fn brushy(self) -> bool {
        matches!(self, Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Cutout)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelMode {
    Rect,
    Ellipse,
    Lasso,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Layer,
    Adjust,
    Looks,
}

/// Values a slider changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sl {
    BrushSize,
    BrushHard,
    BrushOpacity,
    FillTol,
    CutoutTol,
    Opacity,
    Rotation,
    Adj(u8),
    LookAmount,
    TextSize,
    TextSpacing,
    TextLine,
    OutlineW,
    ShadowBlur,
    ShadowY,
    StrokeW,
    Radius,
    GradAngle,
    Straighten,
    ExportQuality,
}

/// Colours a swatch opens the picker for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ct {
    Brush,
    Text,
    Fill,
    Fill2,
    Stroke,
    Outline,
    Shadow,
}

/// On/off switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tg {
    Italic,
    Outline,
    Shadow,
    FillOn,
    StrokeOn,
    Gradient,
    MaskOn,
    Grid,
    Rulers,
    Snap,
}

/// Segmented choices: which and the index chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sg {
    SelMode,
    NewShape,
    ShapeKind,
    Weight,
    Align,
    Tab,
    Format,
    Theme,
    Lang,
    CropRatio,
    ExportSize,
    CloneMode,
    CutMode,
}

/// Text fields edited with the window's edit box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    LayerText(Id),
    LayerName(Id),
    Hex,
    Value(Sl),
    FontSearch,
    CustomW,
    CustomH,
    ExportW,
    ExportH,
    CanvasW,
    CanvasH,
}

/// Commands from buttons, menus and keys.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Home,
    Open,
    Place,
    Save,
    SaveAs,
    Export,
    ExportNow,
    Undo,
    Redo,
    AddText,
    AddShape,
    Duplicate,
    Delete,
    Group,
    Ungroup,
    MaskAdd,
    MaskDelete,
    MaskInvert,
    RemoveBg,
    Align(AlignTo),
    Distribute(bool),
    FlipH,
    FlipV,
    CropApply,
    CropCancel,
    Deselect,
    SelectAll,
    InvertSel,
    FillSel,
    ClearSel,
    SelToLayer,
    SelToMask,
    CropToSel,
    ResetAdjust,
    Shortcuts,
    Settings,
    Theme(&'static str),
    Lang(&'static str),
    Fit,
    Actual,
    ZoomIn,
    ZoomOut,
    CanvasSize,
    StartCustom,
    CloseDialog,
    SwapColours,
    ToTop,
    ToBottom,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    AppMenu,
    Act(Act),
    Preset(usize),
    Template(usize),
    Recent(usize),
    Tool(Tool),
    Slider(Sl),
    Swatch(Ct),
    Toggle(Tg),
    Seg(Sg, usize),
    Field(Field),
    Look(Look),
    BlendMenu,
    FontMenu,
    LayerRow(Id),
    LayerEye(Id),
    LayerLock(Id),
    LayerOpen(Id),
    LayersArea,
    PanelArea,
    Canvas,
    Handle(u8),
    CropHandle(u8),
    MenuItem(usize),
    FontItem(usize),
    PickSv,
    PickHue,
    PickAlpha,
    PaletteColour(Rgba),
    Popup,
    Dialog,
    Scrim,
    Rail,
    Bar,
}

/// A small list that drops from a button.
#[derive(Clone, Debug)]
pub struct Menu {
    pub items: Vec<(String, bool, Act)>,
    pub anchor: Rect,
}

#[derive(Clone, Debug)]
pub enum Popup {
    Menu(Menu),
    /// Blend modes of the selected layer.
    Blend(Rect),
    Fonts {
        anchor: Rect,
        filter: String,
        scroll: f32,
    },
    Colour {
        target: Ct,
        anchor: Rect,
        h: f32,
        s: f32,
        v: f32,
        a: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Export { format: Format, w: u32, h: u32, quality: u32 },
    Custom { w: u32, h: u32 },
    Canvas { w: u32, h: u32 },
    Shortcuts,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropState {
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub angle: f32,
    /// 0 free, 1 original, 2 1:1, 3 4:5, 4 9:16, 5 16:9.
    pub ratio: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Window DIP per document pixel.
    pub zoom: f32,
    /// Where the document's top left corner is, in the window.
    pub ox: f32,
    pub oy: f32,
}

/// The canvas as last drawn: what it shows and the bitmap holding it.
pub struct Frame {
    pub bmp: ID2D1Bitmap,
    pub key: (u64, u32, IRect, bool),
    pub rt: usize,
}

pub enum Drag {
    Pan { start: Pt, ox: f32, oy: f32 },
    Move { start: (f32, f32), before: Doc, orig: Vec<(Id, Layer)>, moved: bool },
    Scale { handle: u8, before: Doc, orig: Vec<(Id, Layer)>, frame: Xf },
    Rotate { start: (f32, f32), before: Doc, orig: Vec<(Id, Layer)>, centre: (f32, f32) },
    Marquee { start: (f32, f32), now: (f32, f32) },
    Lasso { pts: Vec<(f32, f32)> },
    NewShape { start: (f32, f32), id: Id, before: Doc },
    Stroke { id: Id, before: Doc, stroke: Box<crate::engine::paint::Stroke>, mask: bool },
    Crop { handle: u8, start: (f32, f32), orig: CropState },
    Slider { sl: Sl, r: Rect },
    Layer { id: Id, start: Pt, over: Option<(Option<Id>, usize, f32)>, moved: bool },
    PickSv(Rect),
    PickHue(Rect),
    PickAlpha(Rect),
}

/// Small previews of the looks: for the document version and the layer they were made for.
pub type LookPreviews = (u64, Id, Vec<(Look, ID2D1Bitmap)>);

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

pub fn theme_dark(set: &Settings) -> bool {
    match set.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => crate::system_dark(),
    }
}

pub struct App {
    pub win: Window,
    pub pal: Pal,
    pub set: Settings,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub mouse: Pt,
    pub size: (f32, f32),
    pub scale: f32,
    pub active: bool,
    /// The editor (false: the start page).
    pub editing: bool,
    pub doc: Doc,
    /// Bumped at every change of the document (redrawing, thumbnails).
    pub ver: u64,
    pub hist: History,
    /// The project file, once saved or opened.
    pub path: Option<PathBuf>,
    /// The name shown and proposed when saving or exporting.
    pub name: String,
    pub dirty: bool,
    pub sel: Vec<Id>,
    pub selection: Option<Selection>,
    pub tool: Tool,
    pub sel_mode: SelMode,
    pub new_shape: ShapeKind,
    pub heal: bool,
    pub cut_restore: bool,
    pub fill_tol: f32,
    pub cut_tol: f32,
    pub colour: Rgba,
    pub colour2: Rgba,
    pub clone_src: Option<(f32, f32)>,
    pub view: View,
    pub fit_on_next: bool,
    pub renderer: Renderer,
    pub frame: Option<Frame>,
    pub drag: Option<Drag>,
    pub crop: Option<CropState>,
    pub guides: ops::Snap,
    pub popup: Option<Popup>,
    pub dialog: Option<Dialog>,
    pub field: Option<Field>,
    /// Before the first change made while typing in a field (one undo step per field).
    pub field_before: Option<Doc>,
    pub tab: Tab,
    pub panel_scroll: f32,
    pub panel_h: f32,
    pub layers_scroll: f32,
    pub toast: Option<(String, Instant)>,
    pub fonts: Vec<String>,
    pub thumbs: HashMap<Id, (u64, ID2D1Bitmap)>,
    pub looks: Option<LookPreviews>,
    pub tpl_bitmaps: Vec<Option<ID2D1Bitmap>>,
    pub logo: Option<ID2D1Bitmap>,
    pub rt_id: usize,
    pub ants: f32,
    pub log: Vec<String>,
    pub frames: u64,
    pub last_render_ms: f32,
    /// Where the mouse went down (drags measure from it).
    pub drag_origin: Pt,
    /// Clone: from the brush to its source, in the document.
    pub clone_offset: Option<(f32, f32)>,
    pub font_formats: HashMap<String, windows::Win32::Graphics::DirectWrite::IDWriteTextFormat>,
}

impl App {
    pub fn new(win: Window, set: Settings) -> App {
        let pal = Pal::new(theme_dark(&set));
        let text: Box<dyn crate::engine::render::TextRaster> = match DwText::new() {
            Some(t) => Box::new(t),
            None => Box::new(crate::engine::render::NoText),
        };
        let fonts = DwText::new().map(|t| t.families()).unwrap_or_default();
        App {
            win,
            pal,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            mouse: pt(0.0, 0.0),
            size: (1280.0, 820.0),
            scale: 1.0,
            active: true,
            editing: false,
            doc: Doc::new(1080, 1080),
            ver: 1,
            hist: History::default(),
            path: None,
            name: tr("Untitled").into(),
            dirty: false,
            sel: vec![],
            selection: None,
            tool: Tool::Move,
            sel_mode: SelMode::Rect,
            new_shape: ShapeKind::Rect,
            heal: false,
            cut_restore: false,
            fill_tol: 0.15,
            cut_tol: 0.5,
            colour: Rgba::hex(0xdb2b9a),
            colour2: Rgba::WHITE,
            clone_src: None,
            view: View { zoom: 0.5, ox: 0.0, oy: 0.0 },
            fit_on_next: true,
            renderer: Renderer::new(text),
            frame: None,
            drag: None,
            crop: None,
            guides: ops::Snap::default(),
            popup: None,
            dialog: None,
            field: None,
            field_before: None,
            tab: Tab::Layer,
            panel_scroll: 0.0,
            panel_h: 0.0,
            layers_scroll: 0.0,
            toast: None,
            fonts,
            thumbs: HashMap::new(),
            looks: None,
            tpl_bitmaps: vec![],
            logo: None,
            rt_id: 0,
            ants: 0.0,
            log: vec![],
            frames: 0,
            last_render_ms: 0.0,
            drag_origin: pt(0.0, 0.0),
            clone_offset: None,
            font_formats: HashMap::new(),
            set,
        }
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 60 {
            self.log.remove(0);
        }
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        let t: String = text.into();
        self.note(format!("toast: {t}"));
        self.toast = Some((t, Instant::now()));
        cx.repaint();
    }

    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h) && self.popup.is_none() || (self.popup.is_some() && self.hover.as_ref() == Some(h))
    }

    // ----- the document -----

    /// The document changed (no new undo step).
    pub fn touch(&mut self) {
        self.ver += 1;
        self.dirty = true;
    }

    /// Changes the document as one undo step.
    pub fn change(&mut self, label: &str, f: impl FnOnce(&mut Doc)) {
        let before = self.doc.clone();
        f(&mut self.doc);
        self.hist.push(before, label);
        self.touch();
        self.clean_sel();
    }

    /// Keeps a copy of the document before a change made in several moves (a drag, typing): see `commit`.
    pub fn commit(&mut self, before: Doc, label: &str) {
        self.hist.push(before, label);
        self.touch();
    }

    fn clean_sel(&mut self) {
        let doc = &self.doc;
        self.sel.retain(|id| doc.find(*id).is_some());
    }

    pub fn primary(&self) -> Option<&Layer> {
        self.sel.first().and_then(|id| self.doc.find(*id))
    }

    pub fn undo(&mut self, cx: &mut Cx) {
        self.end_field();
        if let Some(l) = self.hist.undo(&mut self.doc) {
            self.ver += 1;
            self.dirty = true;
            self.clean_sel();
            self.note(format!("undo {l}"));
            cx.repaint();
        }
    }

    pub fn redo(&mut self, cx: &mut Cx) {
        self.end_field();
        if let Some(l) = self.hist.redo(&mut self.doc) {
            self.ver += 1;
            self.dirty = true;
            self.clean_sel();
            self.note(format!("redo {l}"));
            cx.repaint();
        }
    }

    /// Opens a document in the editor.
    pub fn start(&mut self, doc: Doc, name: String, path: Option<PathBuf>) {
        self.doc = doc;
        self.name = name;
        self.path = path;
        self.dirty = false;
        self.hist.clear();
        self.sel.clear();
        self.selection = None;
        self.crop = None;
        self.clone_src = None;
        self.editing = true;
        self.fit_on_next = true;
        self.frame = None;
        self.thumbs.clear();
        self.looks = None;
        self.ver += 1;
        self.tab = Tab::Layer;
        self.panel_scroll = 0.0;
        self.layers_scroll = 0.0;
        // the top layer that can be edited is selected
        if let Some(l) = self.doc.layers.iter().rev().find(|l| !l.locked) {
            self.sel = vec![l.id];
        }
        let title = format!("{} — Plume Create", self.name);
        self.win.set_title(&title);
    }

    pub fn new_design(&mut self, w: u32, h: u32, name: &str) {
        let doc = templates::blank(w, h, tr("Background"));
        self.start(doc, name.to_string(), None);
        self.tool = Tool::Move;
    }

    pub fn start_template(&mut self, key: &str) -> bool {
        let r = &self.renderer;
        let measure = |t: &TextL, w: f32| r.measure(t, w);
        let Some(doc) = templates::template(key, crate::i18n::french(), &measure) else { return false };
        let name = templates::TEMPLATES.iter().find(|t| t.key == key).map(|t| tr(t.name)).unwrap_or("Design");
        self.start(doc, name.to_string(), None);
        self.tool = Tool::Move;
        true
    }

    /// Opens a photo (a new document its size) or a project.
    pub fn open_path(&mut self, p: &Path, cx: &mut Cx) -> bool {
        let ext = wic::ext(p);
        if ext == project::EXT {
            match project::load(p) {
                Ok(doc) => {
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    self.start(doc, name, Some(p.to_path_buf()));
                    self.set.add_recent(p.to_path_buf());
                    self.set.save();
                    cx.repaint();
                    true
                }
                Err(e) => {
                    self.say(fill(tr("Could not open this project: {a}"), &[&e]), cx);
                    false
                }
            }
        } else {
            match wic::open(p) {
                Ok(img) => {
                    let (w, h) = (img.w, img.h);
                    let mut doc = Doc::new(w, h);
                    let id = doc.id();
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    doc.layers.push(Layer::new(
                        id,
                        name.clone(),
                        Xf::new(w as f32 / 2.0, h as f32 / 2.0, w as f32, h as f32),
                        Kind::Image(ImageL { px: Arc::new(img), key: 0 }),
                    ));
                    self.start(doc, name, None);
                    self.set.add_recent(p.to_path_buf());
                    self.set.save();
                    cx.repaint();
                    true
                }
                Err(e) => {
                    let msg = if e == "no codec" { tr("Windows has no codec for this format").to_string() } else { e };
                    self.say(fill(tr("Could not open this picture: {a}"), &[&msg]), cx);
                    false
                }
            }
        }
    }

    /// Puts a photo as a new layer, fitted in the canvas.
    pub fn place_path(&mut self, p: &Path, cx: &mut Cx) -> bool {
        match wic::open(p) {
            Ok(img) => {
                let (dw, dh) = (self.doc.w as f32, self.doc.h as f32);
                let k = (dw * 0.8 / img.w as f32).min(dh * 0.8 / img.h as f32).min(1.0);
                let (w, h) = (img.w as f32 * k, img.h as f32 * k);
                let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let mut nid = 0;
                self.change("place", |d| {
                    nid = d.id();
                    d.layers.push(Layer::new(nid, name, Xf::new(dw / 2.0, dh / 2.0, w, h), Kind::Image(ImageL { px: Arc::new(img), key: 0 })));
                });
                self.sel = vec![nid];
                cx.repaint();
                true
            }
            Err(e) => {
                self.say(fill(tr("Could not open this picture: {a}"), &[&e]), cx);
                false
            }
        }
    }

    pub fn save(&mut self, as_new: bool, cx: &mut Cx) {
        self.end_field();
        let path = match (&self.path, as_new) {
            (Some(p), false) => Some(p.clone()),
            _ => plume_ui::dialogs::save_as(
                self.win.hwnd,
                tr("Save the project"),
                &format!("{}.{}", self.name, project::EXT),
                project::EXT,
                &[(tr("Plume Create project"), "*.plumecreate")],
            ),
        };
        let Some(path) = path else { return };
        let path = if wic::ext(&path) == project::EXT { path } else { path.with_extension(project::EXT) };
        match project::save(&self.doc, &path) {
            Ok(()) => {
                self.name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.path = Some(path.clone());
                self.dirty = false;
                self.set.add_recent(path);
                self.set.save();
                self.win.set_title(&format!("{} — Plume Create", self.name));
                self.say(tr("Project saved"), cx);
            }
            Err(e) => self.say(fill(tr("Could not save: {a}"), &[&e]), cx),
        }
    }

    pub fn export_dialog(&mut self) {
        self.end_field();
        self.dialog = Some(Dialog::Export { format: self.set.export_format, w: self.doc.w, h: self.doc.h, quality: self.set.export_quality });
    }

    /// Draws the document at `w` × `h` and writes it (never over a file). Returns where it went.
    pub fn export_to(&mut self, path: &Path, format: Format, w: u32, quality: u32) -> Result<PathBuf, String> {
        let k = w as f32 / self.doc.w as f32;
        let t = Instant::now();
        let img = self.renderer.image(&self.doc, k);
        self.note(format!("export render {}x{} in {} ms", img.w, img.h, t.elapsed().as_millis()));
        let path = if wic::ext(path) == format.ext() || (format == Format::Jpeg && wic::ext(path) == "jpeg") {
            path.to_path_buf()
        } else {
            path.with_extension(format.ext())
        };
        wic::export(&img, &path, format, quality)
    }

    pub fn export_now(&mut self, cx: &mut Cx) {
        let Some(Dialog::Export { format, w, quality, .. }) = self.dialog.clone() else { return };
        self.set.export_format = format;
        self.set.export_quality = quality;
        self.set.save();
        let kinds = [(
            format.label(),
            match format {
                Format::Png => "*.png",
                Format::Jpeg => "*.jpg",
                Format::Webp => "*.webp",
            },
        )];
        let Some(path) = plume_ui::dialogs::save_as(self.win.hwnd, tr("Export"), &format!("{}.{}", self.name, format.ext()), format.ext(), &kinds) else {
            return;
        };
        self.dialog = None;
        match self.export_to(&path, format, w, quality) {
            Ok(p) => {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.say(fill(tr("Exported: {a}"), &[&name]), cx);
            }
            Err(e) => self.say(fill(tr("Could not export: {a}"), &[&e]), cx),
        }
    }

    // ----- layers -----

    /// The image layer painting goes to: the selected one, else a new transparent layer the size of the canvas.
    pub fn paint_layer(&mut self) -> Id {
        if let Some(l) = self.primary()
            && matches!(l.kind, Kind::Image(_))
            && !l.locked
        {
            return l.id;
        }
        let (w, h) = (self.doc.w, self.doc.h);
        let mut nid = 0;
        let above = self.sel.first().copied();
        self.change("new paint layer", |d| {
            nid = d.id();
            let l = Layer::new(
                nid,
                tr("Painting"),
                Xf::new(w as f32 / 2.0, h as f32 / 2.0, w as f32, h as f32),
                Kind::Image(ImageL { px: Arc::new(Img::new(w, h)), key: 0 }),
            );
            insert_above(d, above, l);
        });
        self.sel = vec![nid];
        nid
    }

    pub fn add_text_at(&mut self, x: f32, y: f32, text: &str) -> Id {
        let size = (self.doc.h.min(self.doc.w) as f32 / 12.0).round().max(12.0);
        let t = TextL { text: text.into(), size, color: self.colour, weight: 700, align: TextAlign::Left, ..TextL::default() };
        let w = (self.doc.w as f32 * 0.7).max(size * 4.0);
        let h = self.renderer.measure(&t, w);
        let mut nid = 0;
        let above = self.sel.first().copied();
        let name = text.lines().next().unwrap_or("").chars().take(24).collect::<String>();
        self.change("add text", |d| {
            nid = d.id();
            insert_above(d, above, Layer::new(nid, name, Xf::new(x + w / 2.0, y + h / 2.0, w, h), Kind::Text(t)));
        });
        self.sel = vec![nid];
        nid
    }

    pub fn add_shape(&mut self, kind: ShapeKind, x: f32, y: f32, w: f32, h: f32) -> Id {
        let sh = match kind {
            ShapeKind::Line | ShapeKind::Arrow => {
                ShapeL { kind, fill: None, stroke: Some(self.colour), stroke_w: (self.doc.w.min(self.doc.h) as f32 / 120.0).max(2.0).round(), radius: 0.0 }
            }
            _ => ShapeL {
                kind,
                fill: Some(Paint::Solid(self.colour)),
                stroke: None,
                stroke_w: (self.doc.w.min(self.doc.h) as f32 / 160.0).max(2.0).round(),
                radius: 0.0,
            },
        };
        let name = match kind {
            ShapeKind::Rect => tr("Rectangle"),
            ShapeKind::Ellipse => tr("Ellipse"),
            ShapeKind::Line => tr("Line"),
            ShapeKind::Arrow => tr("Arrow"),
        };
        let mut nid = 0;
        let above = self.sel.first().copied();
        self.change("add shape", |d| {
            nid = d.id();
            insert_above(d, above, Layer::new(nid, name, Xf::new(x + w / 2.0, y + h / 2.0, w, h), Kind::Shape(sh)));
        });
        self.sel = vec![nid];
        nid
    }

    /// The height of a text layer follows its words (the top edge stays).
    pub fn refit_text(&mut self, id: Id) {
        let Some(l) = self.doc.find(id) else { return };
        let Kind::Text(t) = &l.kind else { return };
        let h = self.renderer.measure(t, l.xf.w).max(1.0);
        let m = l.xf.to_doc();
        let (cx, cy) = m.apply(l.xf.w / 2.0, h / 2.0);
        if let Some(l) = self.doc.find_mut(id) {
            l.xf.h = h;
            l.xf.cx = cx;
            l.xf.cy = cy;
        }
    }

    pub fn act(&mut self, a: Act, cx: &mut Cx) {
        self.note(format!("act {a:?}"));
        cx.repaint();
        match a {
            Act::Home => {
                self.end_field();
                self.editing = false;
                self.popup = None;
                self.dialog = None;
                self.win.set_title("Plume Create");
            }
            Act::Open => {
                let files = plume_ui::dialogs::pick_files(self.win.hwnd, tr("Open a photo or a project"));
                if let Some(f) = files.first() {
                    self.open_path(f, cx);
                }
            }
            Act::Place => {
                let files = plume_ui::dialogs::pick_files(self.win.hwnd, tr("Place a photo"));
                if let Some(f) = files.first() {
                    self.place_path(f, cx);
                }
            }
            Act::Save => self.save(false, cx),
            Act::SaveAs => self.save(true, cx),
            Act::Export => self.export_dialog(),
            Act::ExportNow => self.export_now(cx),
            Act::Undo => self.undo(cx),
            Act::Redo => self.redo(cx),
            Act::AddText => {
                let (x, y) = (self.doc.w as f32 * 0.15, self.doc.h as f32 * 0.4);
                let id = self.add_text_at(x, y, tr("Your text"));
                self.tab = Tab::Layer;
                self.open_field(Field::LayerText(id), cx);
            }
            Act::AddShape => {
                let s = self.doc.w.min(self.doc.h) as f32 * 0.4;
                let (x, y) = ((self.doc.w as f32 - s) / 2.0, (self.doc.h as f32 - s) / 2.0);
                let k = if matches!(self.new_shape, ShapeKind::Line | ShapeKind::Arrow) { ShapeKind::Rect } else { self.new_shape };
                self.add_shape(k, x, y, s, s);
            }
            Act::Duplicate => {
                let ids = self.sel.clone();
                let off = (self.doc.w.min(self.doc.h) as f32 / 40.0).round();
                let mut news = vec![];
                self.change("duplicate", |d| {
                    for id in ids {
                        if let Some(n) = ops::duplicate(d, id, off) {
                            news.push(n);
                        }
                    }
                });
                if !news.is_empty() {
                    self.sel = news;
                }
            }
            Act::Delete => {
                if self.selection.is_some() && self.primary().is_some_and(|l| matches!(l.kind, Kind::Image(_))) {
                    self.act(Act::ClearSel, cx);
                    return;
                }
                let ids: Vec<Id> = self.sel.iter().copied().filter(|id| self.doc.find(*id).is_some_and(|l| !l.locked)).collect();
                if ids.is_empty() {
                    return;
                }
                self.change("delete", |d| {
                    for id in &ids {
                        d.remove(*id);
                    }
                });
                self.sel.clear();
            }
            Act::Group => {
                let ids = self.sel.clone();
                if ids.is_empty() {
                    return;
                }
                let mut gid = None;
                self.change("group", |d| gid = ops::group(d, &ids, tr("Group")));
                match gid {
                    Some(g) => self.sel = vec![g],
                    None => {
                        let _ = self.hist.undo(&mut self.doc);
                        self.say(tr("Layers in different groups cannot be grouped"), cx);
                    }
                }
            }
            Act::Ungroup => {
                let Some(id) = self.sel.first().copied() else { return };
                if !self.doc.find(id).is_some_and(|l| l.is_group()) {
                    return;
                }
                let mut out = vec![];
                self.change("ungroup", |d| out = ops::ungroup(d, id));
                self.sel = out;
            }
            Act::MaskAdd => {
                let Some(l) = self.primary() else { return };
                if l.is_group() || l.mask.is_some() {
                    return;
                }
                let (mw, mh) = mask_size(l);
                let id = l.id;
                let from_sel = self.selection.as_ref().map(|s| (s.clone(), l.xf));
                self.change("add mask", |d| {
                    if let Some(l) = d.find_mut(id) {
                        let g = match from_sel {
                            Some((s, xf)) => mask_from_selection(&s, &xf, mw, mh),
                            None => crate::engine::img::Gray::new(mw, mh, 255),
                        };
                        l.mask = Some(crate::engine::doc::Mask { px: Arc::new(g), key: 0, on: true });
                    }
                });
            }
            Act::MaskDelete => {
                let Some(id) = self.sel.first().copied() else { return };
                self.change("delete mask", |d| {
                    if let Some(l) = d.find_mut(id) {
                        l.mask = None;
                    }
                });
            }
            Act::MaskInvert => {
                let Some(id) = self.sel.first().copied() else { return };
                self.change("invert mask", |d| {
                    if let Some(l) = d.find_mut(id)
                        && let Some(m) = &mut l.mask
                    {
                        let g = Arc::make_mut(&mut m.px);
                        for v in &mut g.data {
                            *v = 255 - *v;
                        }
                    }
                });
            }
            Act::RemoveBg => self.remove_background(cx),
            Act::Align(to) => {
                let ids = self.sel.clone();
                self.change("align", |d| ops::align(d, &ids, to));
            }
            Act::Distribute(h) => {
                let ids = self.sel.clone();
                self.change("distribute", |d| ops::distribute(d, &ids, h));
            }
            Act::FlipH | Act::FlipV => {
                let ids = self.sel.clone();
                let hz = a == Act::FlipH;
                self.change("flip", |d| {
                    for id in ids {
                        if let Some(l) = d.find_mut(id) {
                            flip(l, hz);
                        }
                    }
                });
            }
            Act::CropApply => self.apply_crop(cx),
            Act::CropCancel => {
                self.crop = None;
                self.tool = Tool::Move;
            }
            Act::Deselect => self.selection = None,
            Act::SelectAll => {
                self.selection =
                    Some(Selection::new(crate::engine::select::SelShape::Rect(0.0, 0.0, self.doc.w as f32, self.doc.h as f32), self.doc.w, self.doc.h));
            }
            Act::InvertSel => {
                if let Some(s) = &mut self.selection {
                    s.invert = !s.invert;
                }
            }
            Act::FillSel | Act::ClearSel => self.fill_selection(a == Act::FillSel, cx),
            Act::SelToLayer => self.selection_to_layer(cx),
            Act::SelToMask => {
                if self.primary().is_some_and(|l| l.mask.is_some()) {
                    self.act(Act::MaskDelete, cx);
                }
                self.act(Act::MaskAdd, cx);
            }
            Act::CropToSel => {
                let Some(s) = &self.selection else { return };
                let (x0, y0, x1, y1) = s.bounds();
                if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                    return;
                }
                self.change("crop", |d| ops::crop(d, (x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0, 0.0));
                self.selection = None;
                self.fit_on_next = true;
            }
            Act::ResetAdjust => {
                let ids = self.sel.clone();
                self.change("reset adjustments", |d| {
                    for id in ids {
                        if let Some(l) = d.find_mut(id) {
                            let (look, amount) = (l.adj.look, l.adj.look_amount);
                            l.adj = Adjust { look, look_amount: amount, ..Adjust::default() };
                        }
                    }
                });
            }
            Act::Shortcuts => self.dialog = Some(Dialog::Shortcuts),
            Act::Settings => self.dialog = Some(Dialog::Settings),
            Act::Theme(t) => {
                self.set.theme = t.into();
                self.set.save();
                self.apply_theme();
            }
            Act::Lang(l) => {
                self.set.lang = l.into();
                self.set.save();
                crate::i18n::set(l);
                self.tpl_bitmaps.clear();
            }
            Act::Fit => self.fit(),
            Act::Actual => self.zoom_to(1.0, None),
            Act::ZoomIn => self.zoom_to(self.view.zoom * 1.25, None),
            Act::ZoomOut => self.zoom_to(self.view.zoom / 1.25, None),
            Act::CanvasSize => self.dialog = Some(Dialog::Canvas { w: self.doc.w, h: self.doc.h }),
            Act::StartCustom => match self.dialog.clone() {
                Some(Dialog::Custom { w, h }) => {
                    self.set.custom_w = w;
                    self.set.custom_h = h;
                    self.set.save();
                    self.dialog = None;
                    self.new_design(w, h, tr("Design"));
                }
                Some(Dialog::Canvas { w, h }) => {
                    self.dialog = None;
                    self.change("canvas size", |d| ops::resize_canvas(d, w, h));
                    self.fit_on_next = true;
                }
                _ => {}
            },
            Act::CloseDialog => {
                self.end_field();
                self.dialog = None;
            }
            Act::SwapColours => std::mem::swap(&mut self.colour, &mut self.colour2),
            Act::ToTop | Act::ToBottom => {
                let Some(id) = self.sel.first().copied() else { return };
                let top = a == Act::ToTop;
                self.change("order", |d| {
                    if let Some((parent, _)) = d.parent_of(id) {
                        let n = d.list_mut(parent).map_or(0, |l| l.len());
                        ops::move_to(d, id, parent, if top { n } else { 0 });
                    }
                });
            }
        }
    }

    pub fn set_look(&mut self, look: Look) {
        let ids = self.sel.clone();
        if ids.is_empty() {
            return;
        }
        self.change("look", |d| {
            for id in ids {
                if let Some(l) = d.find_mut(id) {
                    l.adj.look = look;
                    if l.adj.look_amount <= 0.0 {
                        l.adj.look_amount = 1.0;
                    }
                }
            }
        });
    }

    pub fn remove_background(&mut self, cx: &mut Cx) {
        let Some(l) = self.primary() else {
            self.say(tr("Select a photo layer first"), cx);
            return;
        };
        let Some(img) = l.image().cloned() else {
            self.say(tr("Select a photo layer first"), cx);
            return;
        };
        let id = l.id;
        let t = Instant::now();
        let mask = crate::engine::cutout::remove_background(&img, self.cut_tol);
        self.note(format!("cutout {}x{} in {} ms", img.w, img.h, t.elapsed().as_millis()));
        self.change("remove background", |d| {
            if let Some(l) = d.find_mut(id) {
                l.mask = Some(crate::engine::doc::Mask { px: Arc::new(mask), key: 0, on: true });
            }
        });
        self.tool = Tool::Cutout;
        self.say(tr("Background removed: touch up with the brush (Erase / Restore)"), cx);
    }

    fn fill_selection(&mut self, fill_it: bool, cx: &mut Cx) {
        let Some(sel) = self.selection.clone() else { return };
        let id = if fill_it { self.paint_layer() } else { self.primary().filter(|l| matches!(l.kind, Kind::Image(_))).map(|l| l.id).unwrap_or(0) };
        let Some(l) = self.doc.find(id) else {
            self.say(tr("Select a photo layer first"), cx);
            return;
        };
        let Some(img) = l.image().cloned() else { return };
        let to_doc = px_to_doc(l, &img);
        let out = crate::engine::paint::fill_selection(&img, &sel, to_doc, fill_it.then_some(self.colour));
        self.change(if fill_it { "fill selection" } else { "clear selection" }, |d| {
            if let Some(l) = d.find_mut(id)
                && let Kind::Image(im) = &mut l.kind
            {
                im.px = Arc::new(out);
            }
        });
    }

    fn selection_to_layer(&mut self, cx: &mut Cx) {
        let Some(sel) = self.selection.clone() else { return };
        let Some(l) = self.primary().filter(|l| matches!(l.kind, Kind::Image(_))).cloned() else {
            self.say(tr("Select a photo layer first"), cx);
            return;
        };
        let img = l.image().cloned().unwrap();
        let to_doc = px_to_doc(&l, &img);
        // the selected part only: the rest made transparent
        let mut copy = (*img).clone();
        for y in 0..copy.h {
            for x in 0..copy.w {
                let (dx, dy) = to_doc.apply(x as f32 + 0.5, y as f32 + 0.5);
                let k = sel.at(dx, dy);
                if k < 1.0 {
                    let mut p = copy.get(x, y);
                    p[3] = (p[3] as f32 * k).round() as u8;
                    copy.set(x, y, p);
                }
            }
        }
        let mut nid = 0;
        self.change("layer from selection", |d| {
            nid = d.id();
            let mut n = l.clone();
            n.id = nid;
            n.name = format!("{} ({})", l.name, tr("copy"));
            n.mask = None;
            n.kind = Kind::Image(ImageL { px: Arc::new(copy), key: 0 });
            insert_above(d, Some(l.id), n);
        });
        self.sel = vec![nid];
        self.selection = None;
    }

    pub fn apply_crop(&mut self, cx: &mut Cx) {
        let Some(c) = self.crop.take() else { return };
        self.change("crop", |d| ops::crop(d, c.cx, c.cy, c.w, c.h, c.angle));
        self.selection = None;
        self.fit_on_next = true;
        self.tool = Tool::Move;
        self.say(fill(tr("Cropped to {a} × {b}"), &[&self.doc.w.to_string(), &self.doc.h.to_string()]), cx);
    }

    pub fn apply_theme(&mut self) {
        let dark = theme_dark(&self.set);
        self.pal = Pal::new(dark);
        self.win.set_dark(dark);
        self.thumbs.clear();
        self.looks = None;
    }

    // ----- view -----

    /// The canvas area of the window (between the bars and panels).
    pub fn stage(&self) -> Rect {
        let (w, h) = self.size;
        let top = m::TITLE_H + OPTS_H;
        let mut r = rect(RAIL_W, top, (w - RAIL_W - PANEL_W).max(10.0), (h - top - STATUS_H).max(10.0));
        if self.set.rulers {
            r.x += RULER;
            r.y += RULER;
            r.w -= RULER;
            r.h -= RULER;
        }
        r
    }

    pub fn fit(&mut self) {
        let s = self.stage();
        let z = ((s.w - 64.0) / self.doc.w as f32).min((s.h - 64.0) / self.doc.h as f32).max(0.01);
        self.view.zoom = z;
        self.view.ox = s.x + (s.w - self.doc.w as f32 * z) / 2.0;
        self.view.oy = s.y + (s.h - self.doc.h as f32 * z) / 2.0;
    }

    /// Zooms keeping a window point (default: the stage's centre) on the same spot of the document.
    pub fn zoom_to(&mut self, z: f32, at: Option<Pt>) {
        let z = z.clamp(0.02, 32.0);
        let s = self.stage();
        let p = at.unwrap_or(pt(s.x + s.w / 2.0, s.y + s.h / 2.0));
        let (dx, dy) = self.to_doc(p);
        self.view.zoom = z;
        self.view.ox = p.x - dx * z;
        self.view.oy = p.y - dy * z;
    }

    /// Window point → document point (through the crop's turn when straightening).
    pub fn to_doc(&self, p: Pt) -> (f32, f32) {
        let x = (p.x - self.view.ox) / self.view.zoom;
        let y = (p.y - self.view.oy) / self.view.zoom;
        match &self.crop {
            Some(c) if c.angle != 0.0 => {
                let (s, co) = c.angle.to_radians().sin_cos();
                let (dx, dy) = (x - c.cx, y - c.cy);
                (c.cx + dx * co - dy * s, c.cy + dx * s + dy * co)
            }
            _ => (x, y),
        }
    }

    /// Document point → window point (no turn).
    pub fn to_win(&self, x: f32, y: f32) -> Pt {
        pt(self.view.ox + x * self.view.zoom, self.view.oy + y * self.view.zoom)
    }

    // ----- fields -----

    pub fn open_field(&mut self, f: Field, cx: &mut Cx) {
        self.end_field();
        self.field = Some(f);
        self.field_before = None;
        cx.repaint();
    }

    /// Closes the edit box (its last value already applied).
    pub fn end_field(&mut self) {
        if self.field.take().is_some() {
            self.win.edit_hide();
        }
        if let Some(before) = self.field_before.take() {
            self.commit(before, "typing");
        }
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.frames += 1;
        self.size = (cx.w, cx.h);
        self.scale = c.scale();
        self.active = cx.active;
        self.hits.clear();
        let rt = c.target();
        let id = draw::target_id(&rt);
        if id != self.rt_id {
            self.rt_id = id;
            self.frame = None;
            self.thumbs.clear();
            self.looks = None;
            self.tpl_bitmaps.clear();
            self.logo = None;
        }
        c.clear(self.pal.bg);
        if self.editing {
            if self.fit_on_next {
                self.fit_on_next = false;
                self.fit();
            }
            self.paint_editor(c, cx);
        } else {
            self.paint_home(c, cx);
        }
        self.paint_dialog(c, cx);
        self.paint_popup(c, cx);
        self.paint_toast(c, cx);
        self.paint_caption(c, cx.w, cx.maximized);
        if self.selection.is_some() || self.drag.as_ref().is_some_and(|d| matches!(d, Drag::Marquee { .. } | Drag::Lasso { .. })) {
            cx.animate = true;
            self.ants = (self.ants + 0.4) % 8.0;
        }
        self.place_field(c);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        self.handle(ev, cx);
    }

    fn area(&self, p: Pt) -> Area {
        if p.y >= m::TITLE_H || self.dialog.is_some() {
            return Area::Client;
        }
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Bar) | None => Area::Caption,
            Some(_) => Area::Control,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Slider(_)) => Cursor::SizeWE,
            Some(Hit::Handle(h)) if *h < 8 => Cursor::Hand,
            Some(Hit::Canvas) => Cursor::Arrow,
            Some(Hit::Bar) | None => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// Puts a new layer just above another (in the same list), or on top.
pub fn insert_above(d: &mut Doc, above: Option<Id>, l: Layer) {
    if let Some(a) = above
        && let Some((parent, i)) = d.parent_of(a)
        && let Some(list) = d.list_mut(parent)
    {
        list.insert(i + 1, l);
        return;
    }
    d.layers.push(l);
}

/// Picture pixels → document.
pub fn px_to_doc(l: &Layer, img: &Img) -> crate::engine::doc::Affine {
    let k = crate::engine::doc::Affine { a: l.xf.w / img.w as f32, b: 0.0, c: 0.0, d: l.xf.h / img.h as f32, e: 0.0, f: 0.0 };
    k.then(&l.xf.to_doc())
}

/// The size a new mask gets: the picture's pixels, or the box's.
pub fn mask_size(l: &Layer) -> (u32, u32) {
    match l.image() {
        Some(i) => (i.w, i.h),
        None => ((l.xf.w.round() as u32).clamp(1, 4096), (l.xf.h.round() as u32).clamp(1, 4096)),
    }
}

/// A mask for a layer's box from a selection of the document.
pub fn mask_from_selection(s: &Selection, xf: &Xf, mw: u32, mh: u32) -> crate::engine::img::Gray {
    let m = crate::engine::doc::Affine { a: xf.w / mw as f32, b: 0.0, c: 0.0, d: xf.h / mh as f32, e: 0.0, f: 0.0 }.then(&xf.to_doc());
    let mut g = crate::engine::img::Gray::new(mw, mh, 0);
    for y in 0..mh {
        for x in 0..mw {
            let (dx, dy) = m.apply(x as f32 + 0.5, y as f32 + 0.5);
            g.set(x, y, (s.at(dx, dy) * 255.0 + 0.5) as u8);
        }
    }
    g
}

pub fn flip(l: &mut Layer, horizontal: bool) {
    let f = l.frame();
    match &mut l.kind {
        Kind::Group(ch) => {
            for c in ch {
                // mirror the child's place across the group's middle, then the child itself
                if horizontal {
                    c.translate(2.0 * (f.cx - c.frame().cx), 0.0);
                } else {
                    c.translate(0.0, 2.0 * (f.cy - c.frame().cy));
                }
                flip(c, horizontal);
            }
        }
        _ => {
            if horizontal {
                l.xf.flip_x = !l.xf.flip_x;
            } else {
                l.xf.flip_y = !l.xf.flip_y;
            }
            l.xf.rot = -l.xf.rot;
        }
    }
}
