// ─────────────────────────────────────────────────────────
// bin/client.rs — Cliente de chat P2P (entry point)
//
// Etapa 4: levanta el PeerService propio ANTES de registrarse (así el
// servidor puede conectarse de vuelta para el push, ver proto.rs),
// se registra, y corre la TUI con directorio en tiempo real y chat
// individual P2P real (send_message).
// ─────────────────────────────────────────────────────────

#[path = "client/app.rs"]
mod app;
#[path = "client/peer.rs"]
mod peer;
#[path = "client/ui.rs"]
mod ui;

use std::io;
use std::process;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use ratatui_explorer::FileExplorer;
use tarpc::{client, context, tokio_serde::formats::Json};
use tokio::sync::mpsc;

use app::{AppMode, AppState, ClientEvent, Focus};
use gato_p2p::proto::{NodeInfo, RegistryServiceClient};

const SERVER_ADDR: &str = "127.0.0.1:9000";
const TICK_RATE: Duration = Duration::from_millis(16);

struct Args {
    nombre: String,
    emoji: String,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut nombre = String::from("Anonimo");
    let mut emoji = String::from("🙂");

    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--nombre" => {
                if let Some(v) = raw.get(i + 1) {
                    nombre = v.clone();
                    i += 1;
                }
            }
            "--emoji" => {
                if let Some(v) = raw.get(i + 1) {
                    emoji = v.clone();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    Args { nombre, emoji }
}

fn main() {
    let args = parse_args();

    // Compuerta de autenticación: paso extra sobre lo pedido en el spec,
    // reutilizado de U6 (usuarios.json + lock de sesión). La identidad
    // del chat sigue siendo --nombre/--emoji, independiente del login.
    let usuario_autenticado = match gato_p2p::auth::autenticar() {
        Some(u) => u,
        None => {
            eprintln!("[Auth] Credenciales incorrectas o sesión ya activa. Acceso denegado.");
            process::exit(1);
        }
    };

    let rt = tokio::runtime::Runtime::new().expect("No se pudo crear el runtime de tokio");
    let resultado = rt.block_on(iniciar(args));

    gato_p2p::auth::cerrar_sesion(&usuario_autenticado);

    if let Err(e) = resultado {
        eprintln!("[Error] {}", e);
        process::exit(1);
    }
}

async fn iniciar(args: Args) -> anyhow::Result<()> {
    let (event_tx, event_rx) = mpsc::channel::<ClientEvent>(256);

    // El listener PeerService propio debe estar arriba ANTES de registrarse:
    // el servidor de descubrimiento se conecta de vuelta a este puerto para
    // empujar notify_directory apenas nos registremos.
    let my_port = peer::start_listener(event_tx.clone()).await?;

    let transport = tarpc::serde_transport::tcp::connect(SERVER_ADDR, Json::default).await?;
    let registry = RegistryServiceClient::new(client::Config::default(), transport).spawn();

    let my_info = NodeInfo {
        username: args.nombre.clone(),
        emoji: args.emoji.clone(),
        ip: "127.0.0.1".to_string(),
        port: my_port,
    };

    let directorio = registry
        .register(context::current(), my_info.clone())
        .await?
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut app = AppState::new(my_info, directorio);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut app, event_rx, event_tx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    registry
        .unregister(context::current(), app.my_info.username.clone())
        .await?
        .map_err(|e| anyhow::anyhow!(e))?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
    mut event_rx: mpsc::Receiver<ClientEvent>,
    event_tx: mpsc::Sender<ClientEvent>,
) -> anyhow::Result<()> {
    let mut last_tick = Instant::now();

    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, app))?;

        let timeout = TICK_RATE.checked_sub(last_tick.elapsed()).unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            let ev = event::read()?;
            if app.file_explorer.is_some() {
                handle_explorer_event(app, &ev, &event_tx);
            } else if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key.code);
                }
            }
        }

        while let Ok(event) = event_rx.try_recv() {
            handle_client_event(app, event);
        }

        app.sweep_finished_players();

        if last_tick.elapsed() >= TICK_RATE {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

/// Aplica un evento recibido del PeerService propio (mensajes P2P, push
/// del directorio, etc.) al estado de la TUI.
fn handle_client_event(app: &mut AppState, event: ClientEvent) {
    match event {
        ClientEvent::DirectMessage { from, content } => {
            app.record_message(from.clone(), from, content);
        }
        ClientEvent::GroupMessage { from, group, content } => {
            app.record_message(group, from, content);
        }
        ClientEvent::FileChunkReceived { from, chunk } => {
            app.receive_file_chunk(from, chunk);
        }
        ClientEvent::SystemMessage { target, content } => {
            app.record_message(target, "Sistema".to_string(), content);
        }
        ClientEvent::DirectoryUpdated(nodes) => {
            app.directory = nodes;
        }
        ClientEvent::GameInvite { from } => app.receive_game_invite(from),
        ClientEvent::GameAccept { from } => app.start_game_as_inviter(from),
        ClientEvent::GameMove { from, position } => app.apply_remote_move(from, position),
        // El resto de variantes se maneja a partir de sus commits
        // correspondientes (grupos, video).
        _ => {}
    }
}

fn handle_key(app: &mut AppState, code: KeyCode) {
    if app.mode == AppMode::Game {
        handle_game_key(app, code);
        return;
    }

    if code == KeyCode::F(2) {
        open_file_explorer(app);
        return;
    }

    match app.focus {
        Focus::Contacts => match code {
            KeyCode::Up => app.select_prev(),
            KeyCode::Down => app.select_next(),
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Char('a') | KeyCode::Char('A') if app.pending_game_invite.is_some() => {
                accept_game_invite(app);
            }
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Esc => app.should_quit = true,
            _ => {}
        },
        Focus::Input => match code {
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Esc => app.input_buffer.clear(),
            KeyCode::Enter => submit_message(app),
            KeyCode::Backspace => {
                app.input_buffer.pop();
            }
            KeyCode::Char(c) => app.input_buffer.push(c),
            _ => {}
        },
    }
}

/// Teclado durante `AppMode::Game`: `[1-9]` juega una casilla, `[Esc]` abandona.
fn handle_game_key(app: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Esc => app.abandon_game(),
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let position = c as u8 - b'1'; // '1'-'9' → 0-8
            if let Some((ip, port, from)) = app.play_local_move(position) {
                tokio::spawn(async move {
                    let _ = peer::game_move_to(&ip, port, from, position).await;
                });
            }
        }
        _ => {}
    }
}

/// `[A]` — acepta la invitación de gato pendiente (si hay alguna) y avisa
/// al peer que la mandó vía `game_accept`.
fn accept_game_invite(app: &mut AppState) {
    let Some(from) = app.pending_game_invite.clone() else { return };
    let Some(node) = app.find_node(&from) else { return };
    let ip = node.ip.clone();
    let port = node.port;
    let my_username = app.my_info.username.clone();

    app.start_game_as_acceptor(from);

    tokio::spawn(async move {
        let _ = peer::game_accept_to(&ip, port, my_username).await;
    });
}

/// `/gato` — invita al contacto seleccionado a jugar.
fn start_game_invite(app: &mut AppState) {
    let Some(target) = app.selected_contact.clone() else { return };
    if app.is_group(&target) {
        app.record_message(
            target,
            "Sistema".to_string(),
            "No se puede jugar al gato con un grupo.".to_string(),
        );
        return;
    }
    let Some(node) = app.find_node(&target) else { return };
    let ip = node.ip.clone();
    let port = node.port;
    let from = app.my_info.username.clone();

    app.mark_game_invite_sent(target.clone());

    tokio::spawn(async move {
        let _ = peer::game_invite_to(&ip, port, from).await;
    });
}

/// `[F2]` — abre el explorador de archivos para adjuntar uno al contacto
/// seleccionado. El envío grupal (fan-out) llega en el commit de grupos.
fn open_file_explorer(app: &mut AppState) {
    let Some(target) = app.selected_contact.clone() else { return };
    if app.is_group(&target) {
        app.record_message(
            target,
            "Sistema".to_string(),
            "Enviar archivos a un grupo llega en un commit posterior.".to_string(),
        );
        return;
    }
    if let Ok(explorer) = FileExplorer::new() {
        app.file_explorer = Some(explorer);
        app.mode = AppMode::FileExplorer;
    }
}

/// Enruta los eventos de teclado/mouse al explorador mientras está abierto,
/// e intercepta `Esc` (cancelar) y `Enter` (confirmar selección de archivo).
fn handle_explorer_event(app: &mut AppState, ev: &Event, event_tx: &mpsc::Sender<ClientEvent>) {
    if let Some(explorer) = app.file_explorer.as_mut() {
        let _ = explorer.handle(ev);
    }

    let Event::Key(key) = ev else { return };
    if key.kind != KeyEventKind::Press {
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.file_explorer = None;
            app.mode = AppMode::Chat;
        }
        KeyCode::Enter => {
            let maybe_path = app
                .file_explorer
                .as_ref()
                .filter(|e| e.current().is_file())
                .map(|e| e.current().path().to_string_lossy().to_string());

            if let Some(path) = maybe_path {
                app.file_explorer = None;
                app.mode = AppMode::Chat;
                start_file_send(app, path, event_tx.clone());
            }
        }
        _ => {}
    }
}

/// Fragmenta+cifra el archivo (reutilizando `transfer.rs`/`crypto.rs` con
/// la clave fija del proyecto) y lo envía por P2P en background; el
/// resultado (éxito o error) se reporta como `SystemMessage` en el chat
/// del contacto seleccionado.
fn start_file_send(app: &mut AppState, path: String, event_tx: mpsc::Sender<ClientEvent>) {
    let Some(target) = app.selected_contact.clone() else { return };
    let Some(node) = app.find_node(&target) else { return };

    let ip = node.ip.clone();
    let port = node.port;
    let from = app.my_info.username.clone();
    let file_name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    app.record_message(target.clone(), "Sistema".to_string(), format!("📎 Enviando {}...", file_name));

    tokio::spawn(async move {
        let result = match gato_p2p::transfer::fragment_and_encrypt(&path, gato_p2p::CLAVE_VIGENERE) {
            Ok(chunks) => peer::send_file_to(&ip, port, from, chunks).await,
            Err(e) => Err(e),
        };
        let content = match result {
            Ok(()) => format!("✅ {} enviado", file_name),
            Err(e) => format!("❌ Error enviando {}: {}", file_name, e),
        };
        let _ = event_tx.send(ClientEvent::SystemMessage { target, content }).await;
    });
}

/// Procesa el buffer de entrada (traduce `/e `), lo agrega al historial
/// local y lo envía por P2P real al contacto seleccionado. El envío
/// grupal (fan-out a cada miembro) se conecta en el commit de grupos.
fn submit_message(app: &mut AppState) {
    if app.input_buffer.trim().is_empty() {
        return;
    }
    let raw = std::mem::take(&mut app.input_buffer);

    if raw.trim() == "/gato" {
        start_game_invite(app);
        return;
    }

    let content = gato_p2p::emoji::procesar(&raw);

    let Some(target) = app.selected_contact.clone() else { return };
    app.record_message(target.clone(), app.my_info.username.clone(), content.clone());

    if app.is_group(&target) {
        return; // fan-out a miembros: commit de grupos
    }

    if let Some(node) = app.find_node(&target) {
        let ip = node.ip.clone();
        let port = node.port;
        let from = app.my_info.username.clone();
        tokio::spawn(async move {
            let _ = peer::send_message_to(&ip, port, from, content).await;
        });
    }
}
