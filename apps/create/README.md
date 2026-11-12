# Plume Create

Photo editing and graphic design in one native app: open a photo to retouch it, or start a design from a size or a
template. Rust, Direct2D for the window, DirectWrite for text, Windows' own codecs (WIC) for opening pictures.
English and French, dark and light. Nothing goes online.

## What it does

**Two ways in**
- **Edit a photo**: PNG, JPEG, WebP, BMP, GIF (first frame), TIFF, and HEIC / AVIF when Windows has the codec. The
  photo is turned the way the camera says; pictures over 8192 px on their long side are brought down to that.
- **Start a design** from a size: Instagram post (1080 × 1080), portrait (1080 × 1350), story (1080 × 1920), TikTok
  cover (1080 × 1920), YouTube thumbnail (1280 × 720), poster A4 (2480 × 3508), banner (1500 × 500) or any size.
- Five starter templates, made only from Plume Create's own shapes and text (Windows' fonts, no outside pictures):
  bold story, quote post, video thumbnail, event poster, clean banner. Their words come in English or French.

**Layers**: picture, text, shape (rectangle with rounded corners, ellipse, line, arrow), group. Each one can be
hidden, locked, faded, given a blend mode (normal, multiply, screen, overlay, darken, lighten, colour dodge and burn,
hard and soft light, difference, exclusion, hue, saturation, colour, luminosity), a mask, and moved in the list by
dragging (onto another place or into a group). Shift / Ctrl pick several.

**Tools** (key in brackets)
- Move and transform (V): handles to scale (corners keep proportions for pictures and text; Shift changes that),
  a handle to turn (Shift: steps of 15°), flip, align and distribute, arrows to nudge. Smart guides: the layer
  snaps to the canvas edges and centre, the other layers' edges and centres, and the grid (Alt: no snapping).
- Select (M, L): rectangle, ellipse, lasso. Then: fill, clear, invert, copy to a new layer, make a mask, crop to it.
  Brush, eraser and fill stay inside the selection.
- Crop and straighten (C): free or fixed ratio (original, 1:1, 4:5, 9:16, 16:9), straighten ±45° (the frame shrinks
  so no empty corner comes in). Layers keep their pixels: nothing is lost outside the frame.
- Brush (B) and eraser (E): size, hardness, opacity; the opacity of a stroke is never exceeded however often the
  brush passes. Painting on a text or a shape makes a new painting layer.
- Fill (G): an area of a picture (tolerance), or a shape's / text's colour in one click.
- Eyedropper (I), Text (T), Shapes (U), Hand (H or Space).
- Clone and heal (S): Alt+click where to copy from; heal keeps the light and colour of the place painted.
- Remove background (K), see below; its brush hides (Erase) or shows (Restore) through the mask.

**Text**: any font installed on Windows (searchable list showing each font in itself), size, five weights, italic,
alignment, colour, letter spacing, line height, outline, soft shadow. The box wraps the words; its side handles
change the width, its corners the size.

**Adjustments** (never change the layer: change them or reset them at any time): exposure, brightness, contrast,
saturation, vibrance, temperature, tint, levels (black point, white point, midtones), sharpen, blur, vignette.

**Looks** in one click, with a strength: Vivid, Warm, Cool, Mono, Noir, Vintage, Faded, Film, Dusk, Matte, Bright,
each previewed on the document.

**Background remover**: not AI. It learns the background's colours from the edges of the picture, spreads from the
edges over pixels close to them without crossing strong outlines, cleans thin lines and specks, and writes a soft
mask (the picture itself is kept). It works well for a subject in front of a plain or simple background; busy
backgrounds need the refine brush. No pure-Rust model small and fast enough on a CPU was found to do better offline.

**Design helpers**: rulers, a grid (50 px), smart guides and snapping, align and distribute, six colour palettes,
the colours used lately, a colour picker (hue, saturation, brightness, opacity, hex).

**Files**
- Project: `.plumecreate`, one JSON file holding the layers and their pictures (PNG inside, each picture once).
- Export: PNG (keeps transparency), JPEG (quality; transparent parts become white), WebP (written without loss by
  the pure-Rust image-webp encoder; a quality under 95 rounds the colours a little for a smaller file), at any size.
  An export never replaces a file: "name (2).png" and so on.
- Undo and redo for everything (80 steps, old ones dropped past 1.5 GB of pictures).

## How it works

- `src/engine/`: plain Rust, no window. `render.rs` is the compositor used for the screen (only the part in view,
  at the zoom shown) and for export (any size): each layer is drawn in the output's pixels, blurred or sharpened if
  asked, toned and coloured, masked, then blended. Pictures drawn much smaller use halved copies; shapes are drawn
  from their outline's distance (crisp at any zoom); text comes from DirectWrite as coverage, outline and shadow
  are added in Rust (distance map, blur). Painting keeps the base picture and the stroke's coverage.
- `src/text.rs` (DirectWrite on a WIC bitmap), `src/wic.rs` (open, export), `src/ui/` (the window, on plume-ui).

## Tests

- `cargo test -p plume-create`: the engine (layers, blend modes, adjustments, looks, shapes, text effects, crop,
  align and snapping, painting, selections, background remover, PNG, project file, undo), export through WIC and
  image-webp, text through DirectWrite.
- `python apps/create/tests/ui/test_create.py <exe>` and `test_tools.py <exe>` drive a test build
  (`cargo build --profile fast -p plume-create --features test-hook`) through its socket: a generated photo is
  cropped, adjusted, given a look and words, exported and checked pixel by pixel; a story is made from a template and
  checked the same way; every tool is used once. The window never takes the focus; screenshots go to
  `target/shots-create/`. Needs Python with Pillow.

## Data and command line

- `plume-create.exe [file]` opens a photo or a project (a running window takes it, single instance).
- Settings: `%LOCALAPPDATA%\Plume\Create\settings.json` (theme, language, recent files, export choices, brush).
- Env: `PLUME_CREATE_DIR` (data folder), `PLUME_CREATE_NOACTIVATE`, `PLUME_CREATE_TEST_PORT` (test-hook builds),
  `PLUME_SUITE_LANG` / `PLUME_SUITE_THEME` (read).

## Not there yet

- Drag and drop from Explorer onto the window, pasting a picture from the clipboard.
- Curves with points (levels are there), adjustment layers, layer effects on pictures and shapes (stroke, shadow),
  text on a path, more than one text style inside a text layer.
- Painting on a mask with the brush and eraser (it is done with the Remove background tool's brush).
- Animated GIF frames beyond the first, PSD files, printing.
