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
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::prelude::*;
use tarpc::{
    client, context,
    server::{self, Channel},
    tokio_serde::formats::Json,
};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::crypto;
use crate::rpc::{ChunkAck, MoveResult, TicTacToe, TicTacToeClient};
use crate::transfer::FileChunk;

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
    /// Canal para encolar los chunks de archivo recibidos
    pub chunk_tx: mpsc::Sender<FileChunk>,
    /// true mientras el usuario local canceló con [Q] la recepción/reproducción
    /// de un video en curso. El servidor RPC lo consulta en `send_chunk` para
    /// rechazar (ChunkAck.ok = false) los chunks restantes y así avisarle al
    /// emisor que debe detener la transmisión, en vez de seguir mandando un
    /// video que ya nadie va a reproducir.
    pub video_cancel: Arc<Mutex<bool>>,
}

impl SharedState {
    pub fn new(clave: String, chunk_tx: mpsc::Sender<FileChunk>) -> Self {
        SharedState {
            incoming_move: Arc::new(Mutex::new(None)),
            rival_disconnected: Arc::new(Mutex::new(false)),
            clave: Arc::new(clave),
            chunk_tx,
            video_cancel: Arc::new(Mutex::new(false)),
        }
    }

    /// Consume y retorna el movimiento recibido del rival
    pub fn poll_incoming_move(&self) -> Option<usize> {
        self.incoming_move.lock().unwrap().take()
    }

    pub fn is_rival_disconnected(&self) -> bool {
        *self.rival_disconnected.lock().unwrap()
    }

    /// Marca/desmarca la cancelación de video para que `send_chunk` rechace
    /// (o vuelva a aceptar) los chunks de video entrantes.
    pub fn set_video_cancel(&self, value: bool) {
        *self.video_cancel.lock().unwrap() = value;
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

    /// Recibe un chunk de archivo del peer rival y lo encola para reconstrucción.
    async fn send_chunk(self, _: context::Context, chunk: FileChunk) -> ChunkAck {
        let idx = chunk.chunk_index;

        // El receptor canceló la reproducción del video con [Q]: rechazar este
        // chunk (y los siguientes) para que el emisor detenga la transmisión
        // en vez de seguir mandando un video que ya nadie va a ver.
        if is_video_file(&chunk.file_name) && *self.state.video_cancel.lock().unwrap() {
            return ChunkAck { chunk_index: idx, ok: false };
        }

        if let Err(e) = self.state.chunk_tx.send(chunk).await {
            eprintln!("[RPC] Error encolando chunk {}: {}", idx, e);
            return ChunkAck { chunk_index: idx, ok: false };
        }
        ChunkAck { chunk_index: idx, ok: true }
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
// Progreso de una transferencia de archivo en curso
// ─────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum TransferProgress {
    Sending { current: u32, total: u32, file_name: String },
    Done { file_name: String },
    Error(String),
}

/// Extensiones de video reconocidas para distinguir un archivo de video
/// de un archivo cualquiera (p.ej. los memes) al terminar su reconstrucción.
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "mpg", "mpeg"];

/// true si `file_name` tiene una extensión de video conocida.
pub fn is_video_file(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

// ─────────────────────────────────────────────────────────
// Envía todos los chunks de un archivo al peer rival
// Espera el ChunkAck de cada chunk antes de enviar el siguiente.
// Reporta progreso por el canal progress_tx para actualizar la TUI.
// ─────────────────────────────────────────────────────────
pub async fn send_file_chunks(
    client: &TicTacToeClient,
    chunks: Vec<FileChunk>,
    progress_tx: mpsc::Sender<TransferProgress>,
) -> anyhow::Result<()> {
    let total = chunks.len() as u32;
    let file_name = chunks.first().map(|c| c.file_name.clone()).unwrap_or_default();

    for chunk in chunks {
        let current = chunk.chunk_index + 1;
        let _ = progress_tx
            .send(TransferProgress::Sending {
                current,
                total,
                file_name: file_name.clone(),
            })
            .await;

        match client.send_chunk(context::current(), chunk).await {
            Ok(ack) if ack.ok => {}
            Ok(ack) => {
                let msg = if is_video_file(&file_name) {
                    "Transmisión detenida: el receptor canceló la reproducción".to_string()
                } else {
                    format!("Chunk {} rechazado por el receptor", ack.chunk_index)
                };
                let _ = progress_tx.send(TransferProgress::Error(msg.clone())).await;
                return Err(anyhow::anyhow!(msg));
            }
            Err(e) => {
                let msg = format!("Error de red enviando chunk {}: {}", current - 1, e);
                let _ = progress_tx.send(TransferProgress::Error(msg.clone())).await;
                return Err(anyhow::anyhow!(msg));
            }
        }
    }

    let _ = progress_tx
        .send(TransferProgress::Done { file_name })
        .await;

    Ok(())
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

// ─────────────────────────────────────────────────────────
// Descifra un FileChunk individual para streaming en tiempo real
// ─────────────────────────────────────────────────────────
/// Descifra un FileChunk individual y retorna los bytes crudos reconstruidos.
/// Usado por main.rs para alimentar el pipe de ffplay chunk por chunk, sin
/// esperar a tener el archivo completo (mismo pipeline que `transfer.rs`:
/// Vigenère decrypt → Base64 decode, pero sobre un solo chunk).
pub fn decrypt_chunk_bytes(chunk: &FileChunk, clave: &str) -> anyhow::Result<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let cifrado_str = String::from_utf8(chunk.data.clone())
        .map_err(|e| anyhow::anyhow!("Chunk no es UTF-8 válido: {}", e))?;
    let base64_str = crypto::descifrar(&cifrado_str, clave);
    let bytes = STANDARD
        .decode(&base64_str)
        .map_err(|e| anyhow::anyhow!("Error Base64 decode: {}", e))?;

    Ok(bytes)
}
