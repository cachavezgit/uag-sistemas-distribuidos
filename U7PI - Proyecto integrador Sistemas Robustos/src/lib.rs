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

/// Clave Vigenère compartida por todos los nodos del chat.
/// Fija por diseño: un chat multi-usuario no puede negociar una clave
/// distinta por cada par de peers como hacía el flag `--clave` del U6.
pub const CLAVE_VIGENERE: &str = "SISTEMAS";
