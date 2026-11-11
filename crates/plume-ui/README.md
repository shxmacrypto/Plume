# plume-ui

The interface kit shared by the native Plume apps (Plume FEx, Plume Mail). It knows nothing about files or
mail: an app gives it a state to paint and receives events.

- `window`: a frameless window that keeps the real Windows frame (shadow, rounded corners, snapping, the snap
  layouts of Windows 11), events, timers, work handed back from other threads (`Sender`), work done after the
  current event (`Cx::after`, for anything that opens a menu or a dialog).
- `canvas`: Direct2D drawing in device-independent pixels; one-line text with an ellipsis, wrapped text, text
  measuring, vector icons, images.
- `edit`: the text field. A real Windows `EDIT` control placed over the window while the user types (one line,
  password, or several lines), so IME, selection, undo and the clipboard are Windows' own.
- `theme`, `icons`, `geom`, `gfx`: Plume's colours and metrics, its line icons, rectangles and colours, the
  Direct2D and DirectWrite factories.
- `dialogs`: Windows' own "save as", "choose a folder" and "choose files" dialogs.
- `Hits<Id>`: every control drawn in a frame registers its rectangle; the last one drawn is on top.

With the `test-hook` feature (test builds only), the dialogs do not open and answer from the environment.
