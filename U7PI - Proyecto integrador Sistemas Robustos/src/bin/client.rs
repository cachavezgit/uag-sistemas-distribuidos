// ─────────────────────────────────────────────────────────
// bin/client.rs — Cliente de chat P2P (entry point)
//
// Levanta el PeerService propio ANTES de registrarse (así el servidor
// puede conectarse de vuelta para el push, ver proto.rs), se registra,
// y corre la TUI: chat individual y grupal, transferencia de archivos,
// gato embebido y videollamada.
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
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use ratatui_explorer::FileExplorer;
use tarpc::{client, context, tokio_serde::formats::Json};
use tokio::sync::mpsc;

use app::{AppMode, AppState, ClientEvent, Focus};
use gato_p2p::proto::{NodeInfo, RegistryServiceClient};

const TICK_RATE: Duration = Duration::from_millis(16);

struct Args {
    nombre: String,
    emoji: String,
    /// Dirección IP:puerto del servidor de descubrimiento.
    /// Por defecto 127.0.0.1:9000; en red local usar la IP LAN del servidor,
    /// p. ej. --servidor 192.168.1.10:9000
    servidor: String,
    /// Índice de cámara a usar para videollamada (default 0 = cámara principal).
    /// En Linux con cámara virtual: --camara 10 para /dev/video10.
    camera_index: u32,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut nombre = String::from("Anonimo");
    let mut emoji = String::from("🙂");
    let mut servidor = String::from("192.168.132.23:9000");
    let mut camera_index: u32 = 0;

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
            "--servidor" => {
                if let Some(v) = raw.get(i + 1) {
                    servidor = v.clone();
                    i += 1;
                }
            }
            "--camara" => {
                if let Some(v) = raw.get(i + 1) {
                    if let Ok(n) = v.parse::<u32>() {
                        camera_index = n;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    Args { nombre, emoji, servidor, camera_index }
}

/// Aviso no fatal si `mpv` no está instalado: la videollamada y la
/// reproducción de video autoplay caen a `ffplay` si existe (ver
/// `player::Player::detect`), o simplemente fallarán con un mensaje claro.
fn advertir_si_falta_mpv() {
    let tiene_mpv = std::process::Command::new("which")
        .arg("mpv")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !tiene_mpv {
        eprintln!("[Aviso] No se encontró 'mpv' en el sistema (probá `which mpv`).");
        eprintln!("        La videollamada y la reproducción de video usarán 'ffplay' si está disponible.");
        eprintln!("        Instalar con: brew install mpv (macOS) / sudo apt install mpv (Linux)\n");
    }
}

fn main() {
    let args = parse_args();

    advertir_si_falta_mpv();

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

    // Detectar la IP local correcta para la red donde está el servidor:
    // se crea un socket UDP "conectado" a la dirección del servidor (no manda
    // ningún paquete real) y se lee la dirección local que el SO le asignó
    // según su tabla de ruteo. Así funciona tanto en localhost como en LAN.
    let my_ip = {
        let probe = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        probe.connect(&args.servidor).await?;
        probe.local_addr()?.ip().to_string()
    };

    // El listener PeerService propio debe estar arriba ANTES de registrarse:
    // el servidor de descubrimiento se conecta de vuelta a este puerto para
    // empujar notify_directory apenas nos registremos.
    let my_port = peer::start_listener(event_tx.clone()).await?;

    let transport = tarpc::serde_transport::tcp::connect(&args.servidor, Json::default).await?;
    let registry = RegistryServiceClient::new(client::Config::default(), transport).spawn();

    let my_info = NodeInfo {
        username: args.nombre.clone(),
        emoji: args.emoji.clone(),
        ip: my_ip,
        port: my_port,
    };

    let directorio = registry
        .register(context::current(), my_info.clone())
        .await?
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut app = AppState::new(my_info, directorio, args.camera_index);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut app, event_rx, event_tx, registry.clone()).await;

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
    registry: RegistryServiceClient,
) -> anyhow::Result<()> {
    let mut last_tick = Instant::now();

    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, app))?;

        let timeout = TICK_RATE.checked_sub(last_tick.elapsed()).unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            let ev = event::read()?;

            // Salida global: funciona sin importar el foco, el modo (chat,
            // gato, videollamada) o si hay un prompt/explorador abierto —
            // a diferencia de `q`/`Esc`, que solo salen con foco en
            // Contactos (ver `handle_key`).
            let es_ctrl_c = matches!(
                &ev,
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
            );

            if es_ctrl_c {
                app.should_quit = true;
            } else if app.file_explorer.is_some() {
                handle_explorer_event(app, &ev, &event_tx);
            } else if app.group_creation.is_some() {
                if let Event::Key(key) = ev {
                    if key.kind == KeyEventKind::Press {
                        handle_group_creation_key(app, key.code, &registry);
                    }
                }
            } else if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key.code, &registry);
                }
            }
        }

        while let Ok(event) = event_rx.try_recv() {
            let es_frame_de_video = matches!(event, ClientEvent::VideoFrame { .. });
            handle_client_event(app, event);
            if es_frame_de_video {
                // Escribir al stdin del reproductor puede bloquear si el pipe
                // se llena; procesar a lo sumo un frame por tick evita que
                // una ráfaga de frames en cola tranque el resto del loop
                // (teclado, otros eventos) — mismo fix que necesitó el
                // streaming de archivos de video en el U6.
                break;
            }
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
            // Si no había ningún contacto seleccionado (p.ej. el directorio
            // estaba vacío al registrarse), auto-seleccionar el primero
            // disponible cuando llegue el primer push con peers.
            if app.selected_contact.is_none() {
                if let Some(first) = app.contact_keys().first() {
                    app.selected_contact = Some(first.clone());
                }
            }
        }
        ClientEvent::GameInvite { from } => app.receive_game_invite(from),
        ClientEvent::GameAccept { from } => app.start_game_as_inviter(from),
        ClientEvent::GameMove { from, position } => app.apply_remote_move(from, position),
        ClientEvent::GroupsUpdated(groups) => app.groups = groups,
        ClientEvent::VideoFrame { from, jpeg } => app.receive_video_frame(from, jpeg),
        ClientEvent::VideoCallRequest { from } => app.receive_video_call_request(from),
        ClientEvent::VideoCallEnded => app.end_video_call(),
        ClientEvent::VideoCallAccepted { from } => {
            if let Some(node) = app.find_node(&from) {
                let ip = node.ip.clone();
                let port = node.port;
                let my_username = app.my_info.username.clone();
                let stop = app.start_video_call(from);
                spawn_video_pipeline(ip, port, my_username, stop, app.camera_index);
            }
        }
    }
}

fn handle_key(app: &mut AppState, code: KeyCode, registry: &RegistryServiceClient) {
    if app.mode == AppMode::Game {
        handle_game_key(app, code);
        return;
    }

    if code == KeyCode::F(2) {
        open_file_explorer(app);
        return;
    }

    if code == KeyCode::F(4) {
        start_video_call_request(app, registry);
        return;
    }

    if app.video_active && code == KeyCode::Esc {
        // Notificar al peer antes de limpiar el estado local
        if let Some(session) = app.video_session.as_ref() {
            let peer_name = session.peer.clone();
            if let Some(node) = app.find_node(&peer_name) {
                let ip = node.ip.clone();
                let port = node.port;
                let from = app.my_info.username.clone();
                tokio::spawn(async move {
                    let _ = peer::notify_video_call_ended_to(&ip, port, from).await;
                });
            }
        }
        app.end_video_call();
        return;
    }

    // ↑/↓ navegan contactos desde cualquier panel
    match code {
        KeyCode::Up => { app.select_prev(); return; }
        KeyCode::Down => { app.select_next(); return; }
        _ => {}
    }

    match app.focus {
        Focus::Contacts => match code {
            // Tab y Enter enfocan el panel de entrada
            KeyCode::Tab | KeyCode::Enter => app.toggle_focus(),
            KeyCode::Char('a') | KeyCode::Char('A') if app.pending_game_invite.is_some() => {
                accept_game_invite(app);
            }
            KeyCode::Char('a') | KeyCode::Char('A') if app.pending_video_call_invite.is_some() => {
                accept_video_call(app, registry);
            }
            KeyCode::Char('g') | KeyCode::Char('G') => app.start_group_creation(),
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Esc => app.should_quit = true,
            // Cualquier carácter no asignado arriba: cambiar foco y empezar a escribir
            KeyCode::Char(c) => {
                app.toggle_focus();
                app.input_buffer.push(c);
            }
            _ => {}
        },
        Focus::Input => match code {
            // Tab mantiene el buffer; Esc cancela y vuelve a Contactos
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Esc => {
                app.input_buffer.clear();
                app.toggle_focus();
            }
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

/// `[F4]` — solicita una videollamada al contacto seleccionado vía el
/// servidor de descubrimiento (`request_video_call`). No arranca la
/// cámara todavía: eso pasa recién cuando llega `VideoCallAccepted`.
fn start_video_call_request(app: &mut AppState, registry: &RegistryServiceClient) {
    let Some(target) = app.selected_contact.clone() else { return };
    if app.is_group(&target) {
        app.record_message(
            target,
            "Sistema".to_string(),
            "No se puede hacer videollamada a un grupo.".to_string(),
        );
        return;
    }
    if app.find_node(&target).is_none() {
        return;
    }

    app.record_message(target.clone(), "Sistema".to_string(), format!("📹 Llamando a {}...", target));

    let my_username = app.my_info.username.clone();
    let registry = registry.clone();
    tokio::spawn(async move {
        let _ = registry.request_video_call(context::current(), my_username, target).await;
    });
}

/// `[A]` — acepta la videollamada pendiente: arranca cámara+envío propios
/// y avisa al peer que la invitó vía `accept_video_call`.
fn accept_video_call(app: &mut AppState, registry: &RegistryServiceClient) {
    let Some(from) = app.pending_video_call_invite.take() else { return };
    let Some(node) = app.find_node(&from) else { return };
    let ip = node.ip.clone();
    let port = node.port;
    let my_username = app.my_info.username.clone();

    let stop = app.start_video_call(from.clone());
    spawn_video_pipeline(ip, port, my_username.clone(), stop, app.camera_index);

    let registry = registry.clone();
    tokio::spawn(async move {
        let _ = registry.accept_video_call(context::current(), my_username, from).await;
    });
}

/// Arranca la captura de cámara (`video::start_capture`, hilo dedicado) y
/// una tarea que consume esos frames y los manda por RPC sobre una única
/// conexión (`peer::stream_video_to`).
/// Sin el feature `camera` (compilación por defecto en Linux/ARM), esta
/// función es un no-op: el nodo puede recibir y reproducir video vía mpv
/// pero no captura ni envía frames propios.
fn spawn_video_pipeline(ip: String, port: u16, my_username: String, stop: std::sync::Arc<std::sync::atomic::AtomicBool>, camera_index: u32) {
    #[cfg(feature = "camera")]
    {
        let (frame_tx, frame_rx) = mpsc::channel::<Vec<u8>>(8);
        gato_p2p::video::start_capture(frame_tx, stop, camera_index);
        tokio::spawn(async move {
            let _ = peer::stream_video_to(&ip, port, my_username, frame_rx).await;
        });
    }
    #[cfg(not(feature = "camera"))]
    {
        let _ = (ip, port, my_username, stop, camera_index);
    }
}

/// Teclado durante el prompt inline de `[G]` — primero el nombre del
/// grupo, luego el checklist de contactos con `[Espacio]` para marcar.
fn handle_group_creation_key(app: &mut AppState, code: KeyCode, registry: &RegistryServiceClient) {
    use app::GroupCreationStep;

    let Some(step) = app.group_creation.as_ref().map(|gc| &gc.step) else { return };

    match step {
        GroupCreationStep::NamingGroup => match code {
            KeyCode::Esc => app.group_creation = None,
            KeyCode::Enter => {
                if let Some(gc) = app.group_creation.as_mut() {
                    if !gc.name.trim().is_empty() {
                        gc.step = GroupCreationStep::SelectingMembers;
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(gc) = app.group_creation.as_mut() {
                    gc.name.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(gc) = app.group_creation.as_mut() {
                    gc.name.push(c);
                }
            }
            _ => {}
        },
        GroupCreationStep::SelectingMembers => match code {
            KeyCode::Esc => app.group_creation = None,
            KeyCode::Up => app.group_creation_move(-1),
            KeyCode::Down => app.group_creation_move(1),
            KeyCode::Char(' ') => app.group_creation_toggle_current(),
            KeyCode::Enter => {
                if let Some(group) = app.finish_group_creation() {
                    let registry = registry.clone();
                    tokio::spawn(async move {
                        let _ = registry.create_group(context::current(), group).await;
                    });
                }
            }
            _ => {}
        },
    }
}

/// `[F2]` — abre el explorador de archivos para adjuntar uno al contacto
/// seleccionado. El envío grupal de archivos queda fuera de alcance.
fn open_file_explorer(app: &mut AppState) {
    let Some(target) = app.selected_contact.clone() else { return };
    if app.is_group(&target) {
        app.record_message(
            target,
            "Sistema".to_string(),
            "Enviar archivos a un grupo no está soportado.".to_string(),
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
/// local y lo envía por P2P real al contacto (o a cada miembro del grupo)
/// seleccionado.
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
        send_group_message(app, &target, content);
        return;
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

/// Fan-out: le manda `content` por P2P a cada miembro del grupo `group_name`
/// (excepto a mí mismo), una conexión efímera por destinatario.
fn send_group_message(app: &AppState, group_name: &str, content: String) {
    let Some(group) = app.groups.iter().find(|g| g.name == group_name).cloned() else { return };
    let my_username = app.my_info.username.clone();

    for member in group.members.into_iter().filter(|m| *m != my_username) {
        let Some(node) = app.find_node(&member) else { continue };
        let ip = node.ip.clone();
        let port = node.port;
        let from = my_username.clone();
        let group_name = group_name.to_string();
        let content = content.clone();
        tokio::spawn(async move {
            let _ = peer::send_group_message_to(&ip, port, from, group_name, content).await;
        });
    }
}
