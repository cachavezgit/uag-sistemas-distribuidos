// ─────────────────────────────────────────────────────────
// bin/client.rs — Cliente de chat P2P (entry point)
//
// Etapa 3 (TUI skeleton): autentica, se registra en el servidor de
// descubrimiento y lanza la TUI con el layout WhatsApp. La navegación
// y el envío de mensajes son locales todavía (echo en el historial);
// el listener PeerService propio, el push en tiempo real y el envío
// P2P real se conectan en el commit 4.
// ─────────────────────────────────────────────────────────

#[path = "client/app.rs"]
mod app;
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
use tarpc::{client, context, tokio_serde::formats::Json};

use app::{AppState, Focus};
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
    let transport = tarpc::serde_transport::tcp::connect(SERVER_ADDR, Json::default).await?;
    let registry = RegistryServiceClient::new(client::Config::default(), transport).spawn();

    let my_info = NodeInfo {
        username: args.nombre.clone(),
        emoji: args.emoji.clone(),
        ip: "127.0.0.1".to_string(),
        port: 0, // el listener PeerService propio se agrega en el commit 4
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

    let result = run_loop(&mut terminal, &mut app).await;

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
) -> anyhow::Result<()> {
    let mut last_tick = Instant::now();

    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, app))?;

        let timeout = TICK_RATE.checked_sub(last_tick.elapsed()).unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key.code);
                }
            }
        }

        if last_tick.elapsed() >= TICK_RATE {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn handle_key(app: &mut AppState, code: KeyCode) {
    match app.focus {
        Focus::Contacts => match code {
            KeyCode::Up => app.select_prev(),
            KeyCode::Down => app.select_next(),
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Esc => app.should_quit = true,
            _ => {}
        },
        Focus::Input => match code {
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Esc => app.input_buffer.clear(),
            KeyCode::Enter => {
                if !app.input_buffer.trim().is_empty() {
                    let content = std::mem::take(&mut app.input_buffer);
                    app.push_local_message(content);
                }
            }
            KeyCode::Backspace => {
                app.input_buffer.pop();
            }
            KeyCode::Char(c) => app.input_buffer.push(c),
            _ => {}
        },
    }
}
