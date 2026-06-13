// ─────────────────────────────────────────────────────────
// rpc.rs — Definición del servicio RPC con tarpc
//
// El atributo #[tarpc::service] genera automáticamente:
//   - Un trait `TicTacToe` para implementar en el servidor
//   - Un `TicTacToeClient` para invocar desde el cliente
//
// Esto es el equivalente en Rust a Java RMI:
//   - La interfaz aquí = la interfaz Remote en Java
//   - TicTacToeClient  = el stub generado por rmic
//   - TicTacToeServer  = el skeleton en el servidor
// ─────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// Resultado de un movimiento enviado al peer remoto
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveResult {
    Ok,           // Movimiento aplicado correctamente
    InvalidCell,  // Casilla ocupada o fuera de rango
    NotYourTurn,  // El peer remoto intentó mover en turno ajeno
    GameOver,     // El juego ya terminó
}

/// Definición del servicio RPC del juego del gato
///
/// Cada método aquí es una llamada remota que un peer
/// puede invocar en el otro como si fuera una función local.
#[tarpc::service]
pub trait TicTacToe {
    /// Envía un movimiento al peer remoto.
    /// casilla: índice 0-8 del tablero
    /// Retorna el resultado de aplicar el movimiento.
    async fn make_move(casilla: usize) -> MoveResult;

    /// Consulta si el peer remoto está listo para jugar.
    async fn ping() -> bool;
}
