# Plume logos

One family: the same rounded square, the same gloss, the same feather.

| App | File | Idea |
|---|---|---|
| Plume Web (the browser) | `../logo.svg`, `../logo256.png`, `../plume.ico` | The original logo: the white feather on blue. It does not change. |
| Plume Suite (the installer, the whole product) | `plume-suite.svg` | The same feather, in the Plume gradient, on graphite with a gradient ring. |
| Plume FEx (file explorer) | `plume-fex.svg` | A folder holding the feather, on amber. |
| Plume Mail | `plume-mail.svg` | An envelope sealed with the feather, on rose. |

`family.png` shows the four side by side at every icon size, on dark and on light.

## Rebuild the PNG and ICO files

The SVG files are the source. After editing one:

```bash
python assets/brand/make-icons.py
```

It needs Microsoft Edge (to draw the SVG, headless, with a throwaway profile) and Pillow. It writes
`<name>-256.png` and `<name>.ico` (16 to 256 px) next to each SVG, and `family.png`.
