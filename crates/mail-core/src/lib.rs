//! Le cœur de Plume Mail : parler aux serveurs de courrier (IMAP pour lire, SMTP pour envoyer), lire et écrire des
//! messages, garder la boîte sur ce PC. Aucune interface ici.

pub mod engine;
pub mod html;
pub mod imap;
pub mod message;
pub mod net;
pub mod oauth;
pub mod smtp;
pub mod store;
