use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

const SERVIDOR_PUERTO: u16 = 9000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Uso: {} <mi_puerto> <mi_ip> <servidor_ip>  (ej: 8001 192.168.1.10 192.168.1.5)", args[0]);
        std::process::exit(1);
    }

    let mi_puerto: u16 = args[1].parse().expect("Puerto inválido");
    let mi_ip = args[2].clone();
    let servidor_ip = args[3].clone();
    let nombre = format!("Cliente:{}", mi_puerto);

    let mensajes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    println!("╔══════════════════════════════════════════╗");
    println!("║  Chat P2P via Servidor Central            ║");
    println!("║  {}  →  Servidor {}:{}    ║", nombre, servidor_ip, SERVIDOR_PUERTO);
    println!("╚══════════════════════════════════════════╝");
    println!("Escribe tu mensaje y presiona Enter.");
    println!("Escribe 'salir' para terminar.\n");

    // ──────────────────────────────────────────
    // HILO SERVIDOR LOCAL — recibe mensajes
    // que el servidor central reenvía
    // ──────────────────────────────────────────
    let mensajes_srv = Arc::clone(&mensajes);
    let addr_local = format!("{}:{}", mi_ip, mi_puerto);
    let listener = TcpListener::bind(&addr_local)
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
    let mut conexion_servidor = conectar_servidor(&servidor_ip)
        .expect("No se pudo conectar al servidor central. ¿Está corriendo?");

    // Enviar paquete de registro con nuestro puerto
    writeln!(conexion_servidor, "REGISTRO:{}", mi_puerto)
        .expect("Error al registrarse");
    conexion_servidor.flush().unwrap();
    println!("[✓] Registrado en el servidor central\n");

    // ──────────────────────────────────────────
    // HILO DISPLAY — imprime mensajes recibidos
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

        if texto.is_empty() {
            continue;
        }

        // Enviar al servidor central (él hace el broadcast)
        let paquete = format!("[{}] {}", nombre, texto);
        if let Err(_) = writeln!(conexion_servidor, "{}", paquete) {
            // Reconectar si se perdió la conexión
            println!("[!] Reconectando al servidor...");
            match conectar_servidor(&servidor_ip) {
                Ok(nueva) => {
                    conexion_servidor = nueva;
                    writeln!(conexion_servidor, "REGISTRO:{}", mi_puerto).ok();
                    conexion_servidor.flush().ok();
                    writeln!(conexion_servidor, "{}", paquete).ok();
                    conexion_servidor.flush().ok();
                }
                Err(e) => {
                    eprintln!("  ✗ No se pudo reconectar: {}", e);
                    continue;
                }
            }
        } else {
            conexion_servidor.flush().ok();
            println!("  ✓ Enviado al servidor");
        }
    }
}

fn conectar_servidor(ip: &str) -> std::io::Result<TcpStream> {
    TcpStream::connect(format!("{}:{}", ip, SERVIDOR_PUERTO))
}
