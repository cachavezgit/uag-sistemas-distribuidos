// ─────────────────────────────────────────────────────────
// network.rs — Servidor y cliente RPC con tarpc
//
// Cada peer levanta su propio servidor tarpc (para recibir
// llamadas del rival) y crea un cliente tarpc (para invocar
// métodos en el peer remoto). Esto implementa RPC verdadero:
// el peer remoto ejecuta make_move() como si fuera local.
// ─────────────────────────────────────────────────────────

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::prelude::*;
use tarpc::{
    client, context,
    server::{self, Channel},
    tokio_serde::formats::Json,
};
use tokio::time::sleep;

use crate::rpc::{MoveResult, TicTacToe, TicTacToeClient};

// ─────────────────────────────────────────────────────────
// Estado compartido entre el servidor RPC y el loop de UI
// ─────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct SharedState {
    /// Último movimiento recibido del rival (None si no hay)
    pub incoming_move: Arc<Mutex<Option<usize>>>,
    /// Si el rival se desconectó o el juego terminó
    pub rival_disconnected: Arc<Mutex<bool>>,
}

impl SharedState {
    pub fn new() -> Self {
        SharedState {
            incoming_move: Arc::new(Mutex::new(None)),
            rival_disconnected: Arc::new(Mutex::new(false)),
        }
    }

    /// Consume y retorna el movimiento recibido del rival
    pub fn poll_incoming_move(&self) -> Option<usize> {
        self.incoming_move.lock().unwrap().take()
    }

    pub fn is_rival_disconnected(&self) -> bool {
        *self.rival_disconnected.lock().unwrap()
    }
}

// ─────────────────────────────────────────────────────────
// Implementación del servidor RPC
//
// Este struct recibe las llamadas remotas del peer rival.
// tarpc genera el trait TicTacToe que aquí implementamos.
// ─────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct TicTacToeServer {
    pub state: SharedState,
}

impl TicTacToe for TicTacToeServer {
    /// El peer rival invoca este método remotamente para
    /// notificar su movimiento. Aquí lo almacenamos para
    /// que el loop de UI lo procese en el siguiente frame.
    async fn make_move(self, _: context::Context, casilla: usize) -> MoveResult {
        if casilla >= 9 {
            return MoveResult::InvalidCell;
        }
        // Almacenar el movimiento para procesarlo en el loop principal
        *self.state.incoming_move.lock().unwrap() = Some(casilla);
        MoveResult::Ok
    }

    /// Responde al ping del rival para confirmar que está listo
    async fn ping(self, _: context::Context) -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────────
// Levanta el servidor RPC en el puerto indicado
// ─────────────────────────────────────────────────────────
pub async fn start_server(port: u16, state: SharedState) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    println!("[RPC] Servidor escuchando en {}", addr);

    // Crear listener TCP con transporte tarpc (JSON sobre TCP)
    let mut listener = tarpc::serde_transport::tcp::listen(&addr, Json::default).await?;
    listener.config_mut().max_frame_length(usize::MAX);

    // Aceptar conexiones en background
    tokio::spawn(async move {
        listener
            .filter_map(|r| future::ready(r.ok()))
            .map(server::BaseChannel::with_defaults)
            .for_each(|channel| {
                let server = TicTacToeServer { state: state.clone() };
                channel.execute(server.serve()).for_each(|response| async move {
                    tokio::spawn(response);
                })
            })
            .await;
    });

    Ok(())
}

// ─────────────────────────────────────────────────────────
// Crea el cliente RPC que se conecta al peer rival
// Reintenta hasta que el servidor del rival esté listo
// ─────────────────────────────────────────────────────────
pub async fn connect_to_peer(addr: &str) -> anyhow::Result<TicTacToeClient> {
    println!("[RPC] Conectando al peer rival en {}...", addr);

    let mut intentos = 0;
    let transport = loop {
        match tarpc::serde_transport::tcp::connect(addr, Json::default).await {
            Ok(t) => break t,
            Err(e) => {
                intentos += 1;
                if intentos >= 15 {
                    return Err(anyhow::anyhow!("No se pudo conectar al rival: {}", e));
                }
                println!("[RPC] Reintentando conexión ({}/15)...", intentos);
                sleep(Duration::from_secs(1)).await;
            }
        }
    };

    // Crear cliente tarpc — a partir de aquí, make_move() se
    // invoca como una función local pero ejecuta en el peer remoto
    let client = TicTacToeClient::new(client::Config::default(), transport).spawn();

    // Verificar que el rival responde
    client.ping(context::current()).await?;
    println!("[RPC] ✓ Conectado al peer rival");

    Ok(client)
}

// ─────────────────────────────────────────────────────────
// Envía un movimiento al peer rival vía RPC
// ─────────────────────────────────────────────────────────
pub async fn send_move(client: &TicTacToeClient, casilla: usize) -> bool {
    match client.make_move(context::current(), casilla).await {
        Ok(MoveResult::Ok) => true,
        Ok(other) => {
            eprintln!("[RPC] Movimiento rechazado: {:?}", other);
            false
        }
        Err(e) => {
            eprintln!("[RPC] Error enviando movimiento: {}", e);
            false
        }
    }
}
