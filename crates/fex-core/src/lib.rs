//! Cœur de Plume FEx, sans interface : lecture rapide des dossiers, tri « comme Windows »,
//! puis (à venir) l'index de tout le disque et la recherche façon Everything.
//!
//! Tout ce module se teste sans fenêtre : `cargo test -p fex-core`.

pub mod index;
pub mod listing;
pub mod query;
pub mod sort;
pub mod usn;
pub mod wide;

pub use listing::{Entry, list_dir};
pub use sort::{SortKey, sort_by, sort_entries};
