// ─────────────────────────────────────────────────────────
// lib.rs — Re-exports públicos del crate gato-p2p
//
// Los binarios server/client comparten esta librería.
// ─────────────────────────────────────────────────────────

pub mod audio;
pub mod auth;
pub mod crypto;
pub mod emoji;
pub mod game;
pub mod player;
pub mod proto;
pub mod transfer;
#[cfg(feature = "camera")]
pub mod video;

/// Clave Vigenère compartida por todos los nodos del chat.
/// Fija por diseño: un chat multi-usuario no puede negociar una clave
/// distinta por cada par de peers como hacía el flag `--clave` del U6.
pub const CLAVE_VIGENERE: &str = "SISTEMAS";

/// Hora local en formato `HH:MM:SS`, para prefijar logs (`server.rs`).
pub fn timestamp_log() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}
