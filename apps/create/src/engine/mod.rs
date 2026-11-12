//! Plume Create's picture engine: plain Rust, no window needed (only text asks Windows to draw letters, through
//! [`render::TextRaster`]). The interface and the export use the same compositor.

pub mod adjust;
pub mod blend;
pub mod cutout;
pub mod doc;
pub mod history;
pub mod img;
pub mod ops;
pub mod paint;
pub mod png;
pub mod project;
pub mod render;
pub mod select;
pub mod shape;
pub mod templates;
pub mod textfx;
