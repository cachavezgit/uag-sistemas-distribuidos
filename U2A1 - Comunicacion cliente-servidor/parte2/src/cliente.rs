use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

const SERVIDOR_IP: &str = "127.0.0.1";
const SERVIDOR_PUERTO: u16 = 9000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Uso: {} <puerto_escucha>  (ej: 8001, 8002, 8003...)", args[0]);
        std::process::exit(1);
    }

    let mi_puerto: u16 = args[1].parse().expect("Puerto inválido");
    let nombre = format!("Cliente:{}", mi_puerto);

    let mensajes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    println!("╔══════════════════════════════════════════╗");
    println!("║  Chat P2P via Servidor Central            ║");
    println!("║  {}  →  Servidor {}:{}    ║", nombre, SERVIDOR_IP, SERVIDOR_PUERTO);
    println!("╚══════════════════════════════════════════╝");
    println!("Formato: @<puerto> <mensaje>  → envío directo  (ej: @8002 hola)");
    println!("         @all <mensaje>       → envío a todos  (ej: @all hola)");
    println!("         salir                → terminar\n");

    // ──────────────────────────────────────────
    // HILO SERVIDOR LOCAL — recibe mensajes
    // ──────────────────────────────────────────
    let mensajes_srv = Arc::clone(&mensajes);
    let listener = TcpListener::bind(format!("127.0.0.1:{}", mi_puerto))
        .expect("No se pudo abrir el puerto de escucha local");

    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                let msgs = Arc::clone(&mensajes_srv);
                thread::spawn(move || {
                    let reader = BufReader::new(stream);
                    for linea in reader.lines() {
                        if let Ok(msg) = linea {
                            if !msg.is_empty() {
                                msgs.lock().unwrap().push(format!("  << {}", msg));
                            }
                        }
                    }
                });
            }
        }
    });

    // ──────────────────────────────────────────
    // REGISTRARSE en el servidor central
    // ──────────────────────────────────────────
    let mut conexion_servidor = conectar_servidor()
        .expect("No se pudo conectar al servidor. ¿Está corriendo?");

    writeln!(conexion_servidor, "REGISTRO:{}", mi_puerto).unwrap();
    conexion_servidor.flush().unwrap();
    println!("[✓] Registrado en el servidor central\n");

    // ──────────────────────────────────────────
    // HILO DISPLAY
    // ──────────────────────────────────────────
    let mensajes_display = Arc::clone(&mensajes);
    thread::spawn(move || loop {
        thread::sleep(std::time::Duration::from_millis(100));
        let mut msgs = mensajes_display.lock().unwrap();
        for m in msgs.drain(..) {
            println!("{}", m);
        }
    });

    // ──────────────────────────────────────────
    // INTERFAZ DE USUARIO — hilo principal
    // ──────────────────────────────────────────
    let stdin = std::io::stdin();
    for linea in stdin.lock().lines() {
        let texto = linea.expect("Error al leer stdin");
        let texto = texto.trim().to_string();

        if texto.eq_ignore_ascii_case("salir") {
            println!("Cerrando {}...", nombre);
            break;
        }

        if texto.is_empty() { continue; }

        if !texto.starts_with('@') {
            println!("  ⚠ Formato inválido. Usa @<puerto> o @all seguido del mensaje.");
            continue;
        }

        let partes: Vec<&str> = texto[1..].splitn(2, ' ').collect();
        if partes.len() < 2 {
            println!("  ⚠ Falta el mensaje. Ejemplo: @8002 hola  o  @all hola");
            continue;
        }

        let destino = partes[0];
        let mensaje = partes[1];

        // Paquete con formato: "DESTINO:<destino>|[nombre] mensaje"
        let paquete = format!("DESTINO:{}|[{}] {}", destino, nombre, mensaje);

        if let Err(_) = writeln!(conexion_servidor, "{}", paquete) {
            println!("[!] Reconectando al servidor...");
            match conectar_servidor() {
                Ok(nueva) => {
                    conexion_servidor = nueva;
                    writeln!(conexion_servidor, "REGISTRO:{}", mi_puerto).ok();
                    conexion_servidor.flush().ok();
                    writeln!(conexion_servidor, "{}", paquete).ok();
                    conexion_servidor.flush().ok();
                }
                Err(e) => { eprintln!("  ✗ No se pudo reconectar: {}", e); continue; }
            }
        } else {
            conexion_servidor.flush().ok();
            if destino.eq_ignore_ascii_case("all") {
                println!("  ✓ Enviado a todos via servidor");
            } else {
                println!("  ✓ Enviado a Cliente:{} via servidor", destino);
            }
        }
    }
}

fn conectar_servidor() -> std::io::Result<TcpStream> {
    TcpStream::connect(format!("{}:{}", SERVIDOR_IP, SERVIDOR_PUERTO))
}
