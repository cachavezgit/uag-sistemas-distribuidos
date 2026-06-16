// ─────────────────────────────────────────────────────────
// network.rs — Servidor y cliente RPC con tarpc
//
// Cada peer levanta su propio servidor tarpc (para recibir
// llamadas del rival) y crea un cliente tarpc (para invocar
// métodos en el peer remoto). Esto implementa RPC verdadero:
// el peer remoto ejecuta make_move() como si fuera local.
// ─────────────────────────────────────────────────────────

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::prelude::*;
use tarpc::{
    client, context,
    server::{self, Channel},
    tokio_serde::formats::Json,
};
use tokio::time::sleep;

use crate::crypto;
use crate::rpc::{MoveResult, TicTacToe, TicTacToeClient};

// Escribe una línea al archivo crypto.log sin tocar stdout,
// evitando interferir con la TUI de Ratatui que usa stdout.
fn log_crypto(msg: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("crypto.log") {
        let _ = writeln!(f, "{}", msg);
    }
}

// Crea crypto.log vacío al arrancar para que `tail -f` funcione
// desde el inicio sin esperar al primer movimiento.
pub fn iniciar_log() {
    let _ = OpenOptions::new().create(true).append(true).open("crypto.log");
}

// ─────────────────────────────────────────────────────────
// Estado compartido entre el servidor RPC y el loop de UI
// ─────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct SharedState {
    /// Último movimiento recibido del rival (None si no hay)
    pub incoming_move: Arc<Mutex<Option<usize>>>,
    /// Si el rival se desconectó o el juego terminó
    pub rival_disconnected: Arc<Mutex<bool>>,
    /// Clave Vigenère para descifrar los payloads entrantes
    pub clave: Arc<String>,
}

impl SharedState {
    pub fn new(clave: String) -> Self {
        SharedState {
            incoming_move: Arc::new(Mutex::new(None)),
            rival_disconnected: Arc::new(Mutex::new(false)),
            clave: Arc::new(clave),
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
    /// El peer rival invoca este método remotamente para notificar su movimiento.
    /// Recibe el payload cifrado con Vigenère, lo muestra en terminal para
    /// evidenciar su opacidad, lo descifra y almacena la casilla resultante.
    async fn make_move(self, _: context::Context, payload: String) -> MoveResult {
        // Registrar el payload tal como viajó por la red (cifrado)
        log_crypto(&format!("[RECV] cifrado:    \"{}\"", payload));

        // Descifrar usando la clave compartida almacenada en SharedState
        let descifrado = crypto::descifrar(&payload, &self.state.clave);
        log_crypto(&format!("[RECV] descifrado: \"{}\"", descifrado));

        // Parsear el índice de casilla desde el texto descifrado
        let casilla = match descifrado.trim().parse::<usize>() {
            Ok(c) => c,
            Err(_) => {
                log_crypto("[RECV] Error: payload descifrado no es un número válido");
                return MoveResult::InvalidCell;
            }
        };

        if casilla >= 9 {
            return MoveResult::InvalidCell;
        }

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
// Serializa la casilla, la cifra con Vigenère y la envía.
// ─────────────────────────────────────────────────────────
pub async fn send_move(client: &TicTacToeClient, casilla: usize, clave: &str) -> bool {
    // Serializar el índice de casilla como texto plano
    let texto = casilla.to_string();

    // Cifrar con Vigenère antes de entregar al stub RPC
    let payload = crypto::cifrar(&texto, clave);
    log_crypto(&format!("[SEND] casilla {}  →  cifrado: \"{}\"", casilla, payload));

    match client.make_move(context::current(), payload).await {
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
