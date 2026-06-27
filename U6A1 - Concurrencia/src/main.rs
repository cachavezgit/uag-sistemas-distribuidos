// ─────────────────────────────────────────────────────────
// main.rs — Punto de entrada del juego del gato P2P
//
// Uso:
//   Jugador 1: cargo run -- --jugador 1 --escucha 8001 --rival 127.0.0.1:8002 --clave <clave>
//   Jugador 2: cargo run -- --jugador 2 --escucha 8002 --rival 127.0.0.1:8001 --clave <clave>
//
// Ambos jugadores deben usar la misma --clave para que el cifrado
// Vigenère sea simétrico y los mensajes puedan descifrarse correctamente.
// ─────────────────────────────────────────────────────────

mod crypto;
mod auth;
mod game;
mod network;
mod player;
mod rpc;
mod transfer;
mod ui;

use std::io;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use game::{Game, GameResult};
use network::{connect_to_peer, iniciar_log, send_file_chunks, send_move, start_server, SharedState, TransferProgress};
use player::PlayerHandle;
use rpc::TicTacToeClient;
use ratatui_explorer::FileExplorer;
use ui::{TransferState, VideoState};

const TICK_RATE: Duration = Duration::from_millis(16);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (my_player, listen_port, rival_addr, clave) = parse_args(&args);

    iniciar_log();

    let usuario = match auth::autenticar() {
        Some(u) => u,
        None => {
            eprintln!("[Auth] Credenciales incorrectas. Acceso denegado.");
            process::exit(1);
        }
    };
    println!("[Auth] Bienvenido, {}. Iniciando nodo...\n", usuario);

    let rt = tokio::runtime::Runtime::new().expect("No se pudo crear el runtime de tokio");
    let resultado = rt.block_on(iniciar_nodo(my_player, listen_port, rival_addr, usuario.clone(), clave));

    auth::cerrar_sesion(&usuario);

    if let Err(e) = resultado {
        eprintln!("[Error] {}", e);
        process::exit(1);
    }
}

// ─────────────────────────────────────────────────────────
// Lógica async del nodo: servidor RPC + cliente + UI
// ─────────────────────────────────────────────────────────
async fn iniciar_nodo(
    my_player: u8,
    listen_port: u16,
    rival_addr: String,
    usuario: String,
    clave: String,
) -> anyhow::Result<()> {
    // Canal para chunks de archivo recibidos por el servidor RPC
    let (chunk_tx, chunk_rx) = mpsc::channel::<transfer::FileChunk>(256);
    // Canal para reportar progreso de envío a la TUI
    let (progress_tx, progress_rx) = mpsc::channel::<TransferProgress>(64);

    let state = SharedState::new(clave.clone(), chunk_tx);

    start_server(listen_port, state.clone()).await?;

    if my_player == 1 {
        println!("[Info] Esperando que el Jugador 2 levante su servidor...");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    let rpc_client = connect_to_peer(&rival_addr).await?;

    let mut game = Game::new(my_player, usuario);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_loop(
        &mut terminal,
        &mut game,
        &state,
        &rpc_client,
        &clave,
        chunk_rx,
        progress_tx,
        progress_rx,
    )
    .await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result.map_err(|e| anyhow::anyhow!(e))
}

// ─────────────────────────────────────────────────────────
// Loop principal: eventos de teclado, RPC de juego y transferencia
// ─────────────────────────────────────────────────────────
async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    game: &mut Game,
    state: &SharedState,
    rpc_client: &TicTacToeClient,
    clave: &str,
    mut chunk_rx: mpsc::Receiver<transfer::FileChunk>,
    progress_tx: mpsc::Sender<TransferProgress>,
    mut progress_rx: mpsc::Receiver<TransferProgress>,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let mut transfer = TransferState {
        explorer: None,
        progress: None,
        last_event: None,
        video_state: VideoState::default(),
    };

    // Buffer de chunks recibidos, agrupados por nombre de archivo
    let mut recv_buffer: Vec<transfer::FileChunk> = Vec::new();

    // true si el explorador abierto/la transferencia en curso es de video (tecla V),
    // false si es de memes (tecla M) — distingue a quién enrutar el progreso recibido.
    let mut video_mode = false;
    // Reproductor activo (Some mientras haya un video en reproducción)
    let mut player_handle: Option<PlayerHandle> = None;

    loop {
        terminal.draw(|frame| ui::render(frame, game, &transfer))?;

        let timeout = TICK_RATE
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        // ── Eventos de teclado ──
        if event::poll(timeout)? {
            let ev = event::read()?;

            if transfer.explorer.is_some() {
                // Reenviar el evento al explorador antes de inspeccionar teclas
                if let Some(ref mut explorer) = transfer.explorer {
                    let _ = explorer.handle(&ev);
                }

                if let Event::Key(key) = &ev {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc => {
                                transfer.explorer = None;
                            }
                            KeyCode::Enter => {
                                // Extraer path si el elemento seleccionado es un archivo
                                let maybe_path = transfer.explorer.as_ref()
                                    .filter(|e| e.current().is_file())
                                    .map(|e| e.current().path().to_string_lossy().to_string());

                                if let Some(path) = maybe_path {
                                    transfer.explorer = None;

                                    if video_mode && !network::is_video_file(&path) {
                                        transfer.video_state = VideoState::Error(
                                            "Selecciona un archivo de video (.mp4 .mkv .avi .mov .webm)"
                                                .to_string(),
                                        );
                                    } else {
                                        if video_mode {
                                            transfer.video_state =
                                                VideoState::Transmitiendo { chunk_actual: 0, total: 0 };
                                        }
                                        let key_clone = clave.to_string();
                                        let client_clone = rpc_client.clone();
                                        let ptx = progress_tx.clone();
                                        tokio::spawn(async move {
                                            match transfer::fragment_and_encrypt(&path, &key_clone) {
                                                Ok(chunks) => {
                                                    if let Err(e) = send_file_chunks(&client_clone, chunks, ptx).await {
                                                        eprintln!("[Transfer] Error: {}", e);
                                                    }
                                                }
                                                Err(e) => {
                                                    let _ = ptx.send(TransferProgress::Error(e.to_string())).await;
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            } else if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            if let Some(mut handle) = player_handle.take() {
                                let _ = handle.stop();
                                transfer.video_state = VideoState::Inactivo;
                            } else {
                                break;
                            }
                        }

                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            if game.result != GameResult::Ongoing {
                                game.reset();
                            }
                        }

                        KeyCode::Char('m') | KeyCode::Char('M') => {
                            if let Ok(explorer) = FileExplorer::new() {
                                transfer.explorer = Some(explorer);
                                video_mode = false;
                            }
                        }

                        KeyCode::Char('v') | KeyCode::Char('V') => {
                            if let Ok(explorer) = FileExplorer::new() {
                                transfer.explorer = Some(explorer);
                                transfer.video_state = VideoState::Explorando;
                                video_mode = true;
                            }
                        }

                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            let digit = c as usize - '1' as usize;
                            if digit < 9 {
                                handle_local_move(game, state, rpc_client, digit, clave).await;
                            }
                        }

                        _ => {}
                    }
                }
            }
        }

        // ── Movimiento recibido del rival vía RPC ──
        if let Some(casilla) = state.poll_incoming_move() {
            game.apply_move(casilla);
        }

        // ── Actualizar progreso de envío ──
        while let Ok(prog) = progress_rx.try_recv() {
            match &prog {
                TransferProgress::Done { file_name } => {
                    if video_mode {
                        transfer.video_state = VideoState::Inactivo;
                        transfer.last_event = Some(format!("Video enviado: {}", file_name));
                        video_mode = false;
                    } else {
                        transfer.last_event = Some(format!("Enviado: {}", file_name));
                        transfer.progress = None;
                    }
                }
                TransferProgress::Error(msg) => {
                    if video_mode {
                        transfer.video_state = VideoState::Error(msg.clone());
                        video_mode = false;
                    } else {
                        transfer.last_event = Some(format!("Error: {}", msg));
                        transfer.progress = None;
                    }
                }
                TransferProgress::Sending { current, total, .. } => {
                    let (current, total) = (*current, *total);
                    if video_mode {
                        transfer.video_state = VideoState::Transmitiendo {
                            chunk_actual: current as usize,
                            total: total as usize,
                        };
                    } else {
                        transfer.progress = Some(prog);
                    }
                }
                TransferProgress::VideoReady(path) => {
                    match PlayerHandle::play_file(path) {
                        Ok(handle) => {
                            transfer.video_state = VideoState::Reproduciendo;
                            transfer.last_event =
                                Some(format!("Reproduciendo con {}: {}", handle.player.path(), path.display()));
                            player_handle = Some(handle);
                        }
                        Err(e) => {
                            transfer.video_state = VideoState::Error(e.to_string());
                        }
                    }
                }
                TransferProgress::VideoError(msg) => {
                    transfer.video_state = VideoState::Error(msg.clone());
                }
            }
        }

        // ── Chunks recibidos del rival: acumular y reconstruir ──
        while let Ok(chunk) = chunk_rx.try_recv() {
            let total = chunk.total_chunks;
            let idx = chunk.chunk_index;
            recv_buffer.push(chunk);

            if idx == total - 1 {
                // Ordenar por índice antes de reconstruir
                recv_buffer.sort_by_key(|c| c.chunk_index);
                let nombre = recv_buffer[0].file_name.clone();
                let es_video = network::is_video_file(&nombre);
                if es_video {
                    transfer.video_state = VideoState::Reconstruyendo;
                }

                match transfer::decrypt_and_reconstruct(&recv_buffer, clave, "./recibidos") {
                    Ok(ruta) => {
                        if es_video {
                            let _ = progress_tx
                                .send(TransferProgress::VideoReady(PathBuf::from(&ruta)))
                                .await;
                        } else {
                            transfer.last_event =
                                Some(format!("Recibido: {} → {}", nombre, ruta));
                        }
                    }
                    Err(e) => {
                        if es_video {
                            let _ = progress_tx
                                .send(TransferProgress::VideoError(e.to_string()))
                                .await;
                        } else {
                            transfer.last_event = Some(format!("Error al recibir: {}", e));
                        }
                    }
                }
                recv_buffer.clear();
            }
        }

        // ── El reproductor terminó por su cuenta (ffplay -autoexit al acabar el video) ──
        if let Some(handle) = player_handle.as_mut() {
            if !handle.is_running() {
                player_handle = None;
                transfer.video_state = VideoState::Inactivo;
            }
        }

        // ── Rival desconectado ──
        if state.is_rival_disconnected() {
            terminal.draw(|frame| ui::render(frame, game, &transfer))?;
            tokio::time::sleep(Duration::from_secs(2)).await;
            break;
        }

        if last_tick.elapsed() >= TICK_RATE {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────
// Procesa movimiento local: valida, aplica e invoca RPC
// ─────────────────────────────────────────────────────────
async fn handle_local_move(
    game: &mut Game,
    _state: &SharedState,
    rpc_client: &TicTacToeClient,
    casilla: usize,
    clave: &str,
) {
    if !game.is_my_turn() || game.result != GameResult::Ongoing {
        return;
    }
    if !game.is_cell_available(casilla) {
        return;
    }

    game.apply_move(casilla);
    send_move(rpc_client, casilla, clave).await;
}

// ─────────────────────────────────────────────────────────
// Parsea argumentos de línea de comandos
// ─────────────────────────────────────────────────────────
fn parse_args(args: &[String]) -> (u8, u16, String, String) {
    let mut listen_port: u16 = 8001;
    let mut rival_addr = String::from("127.0.0.1:8002");
    let mut my_player: u8 = 1;
    let mut clave = String::from("clave_defecto");

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--escucha" => {
                if let Some(p) = args.get(i + 1) {
                    listen_port = p.parse().unwrap_or(8001);
                    i += 1;
                }
            }
            "--rival" => {
                if let Some(addr) = args.get(i + 1) {
                    rival_addr = addr.clone();
                    i += 1;
                }
            }
            "--jugador" => {
                if let Some(p) = args.get(i + 1) {
                    my_player = p.parse().unwrap_or(1);
                    i += 1;
                }
            }
            "--clave" => {
                if let Some(c) = args.get(i + 1) {
                    clave = c.clone();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    println!("╔══════════════════════════════════════════╗");
    println!("║   Juego del Gato — P2P con RPC (tarpc)   ║");
    println!("║   Jugador {}  |  Puerto: {}              ║", my_player, listen_port);
    println!("║   Rival en: {}                  ║", rival_addr);
    println!("║   Cifrado Vigenère activo                ║");
    println!("╚══════════════════════════════════════════╝\n");

    (my_player, listen_port, rival_addr, clave)
}
