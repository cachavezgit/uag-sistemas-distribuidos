// ─────────────────────────────────────────────────────────
// lib.rs — Re-exports públicos del crate gato-p2p
//
// Los binarios server/client comparten esta librería.
// ─────────────────────────────────────────────────────────

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

/// Hora en formato `HH:MM:SS` (UTC, sin ajuste de zona horaria), para
/// prefijar logs (`server.rs`).
pub fn timestamp_log() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs_of_day = secs % 86400;
    format!("{:02}:{:02}:{:02}", secs_of_day / 3600, (secs_of_day % 3600) / 60, secs_of_day % 60)
}
