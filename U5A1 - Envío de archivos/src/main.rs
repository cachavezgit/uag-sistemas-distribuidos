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
mod rpc;
mod transfer;
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

use game::{Game, GameResult};
use network::{connect_to_peer, iniciar_log, send_move, start_server, SharedState};
use rpc::TicTacToeClient;

const TICK_RATE: Duration = Duration::from_millis(16);

fn main() {
    // ── Parsear argumentos ──
    let args: Vec<String> = std::env::args().collect();
    let (my_player, listen_port, rival_addr, clave) = parse_args(&args);

    // ── Crear crypto.log al arrancar para que `tail -f` funcione de inmediato ──
    iniciar_log();

    // ── Autenticación: síncrona y bloqueante, antes del runtime async ──
    // Ningún socket ni tarea tokio se crea si esta compuerta no pasa.
    let usuario = match auth::autenticar() {
        Some(u) => u,
        None => {
            eprintln!("[Auth] Credenciales incorrectas. Acceso denegado.");
            process::exit(1);
        }
    };
    println!("[Auth] Bienvenido, {}. Iniciando nodo...\n", usuario);

    // ── Lanzar runtime async sólo tras autenticación exitosa ──
    let rt = tokio::runtime::Runtime::new().expect("No se pudo crear el runtime de tokio");
    let resultado = rt.block_on(iniciar_nodo(my_player, listen_port, rival_addr, usuario.clone(), clave));

    // ── Liberar sesión antes de salir (en cualquier caso) ──
    auth::cerrar_sesion(&usuario);

    if let Err(e) = resultado {
        eprintln!("[Error] {}", e);
        process::exit(1);
    }
}

// ─────────────────────────────────────────────────────────
// Lógica async del nodo: servidor RPC + cliente + UI
// Se invoca únicamente si la autenticación fue exitosa.
// ─────────────────────────────────────────────────────────
async fn iniciar_nodo(my_player: u8, listen_port: u16, rival_addr: String, usuario: String, clave: String) -> anyhow::Result<()> {
    // ── Estado compartido entre servidor RPC y UI ──
    // La clave Vigenère se almacena en SharedState para que el servidor
    // RPC pueda descifrar los payloads entrantes del rival.
    let state = SharedState::new(clave.clone());

    // ── Levantar servidor RPC propio ──
    start_server(listen_port, state.clone()).await?;

    // ── Conectar al peer rival ──
    // Ambos jugadores levantan su servidor primero, luego conectan.
    // J1 espera un momento para que J2 también levante el suyo.
    if my_player == 1 {
        println!("[Info] Esperando que el Jugador 2 levante su servidor...");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    let rpc_client = connect_to_peer(&rival_addr).await?;

    // ── Inicializar juego ──
    let mut game = Game::new(my_player, usuario);

    // ── Inicializar terminal Ratatui ──
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // ── Loop principal ──
    let result = run_loop(&mut terminal, &mut game, &state, &rpc_client, &clave).await;

    // ── Restaurar terminal ──
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result.map_err(|e| anyhow::anyhow!(e))
}

// ─────────────────────────────────────────────────────────
// Loop principal: maneja eventos de teclado y llamadas RPC
// ─────────────────────────────────────────────────────────
async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    game: &mut Game,
    state: &SharedState,
    rpc_client: &TicTacToeClient,
    clave: &str,
) -> io::Result<()> {
    let mut last_tick = Instant::now();

    loop {
        // ── Renderizar frame ──
        terminal.draw(|frame| ui::render(frame, game))?;

        let timeout = TICK_RATE
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        // ── Eventos de teclado ──
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,

                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            if game.result != GameResult::Ongoing {
                                game.reset();
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

        // ── Rival desconectado ──
        if state.is_rival_disconnected() {
            terminal.draw(|frame| ui::render(frame, game))?;
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

    // Aplicar localmente
    game.apply_move(casilla);

    // Cifrar y enviar al peer remoto vía RPC
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
