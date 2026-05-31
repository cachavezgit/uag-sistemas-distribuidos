use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

// IP y puerto donde escucha el servidor central
const SERVIDOR_IP: &str = "127.0.0.1";
const SERVIDOR_PUERTO: u16 = 9000;

// Base de datos en memoria: lista de (ip, puerto) de clientes registrados
type ListaNodos = Arc<Mutex<Vec<(String, u16)>>>;

fn main() {
    let nodos: ListaNodos = Arc::new(Mutex::new(Vec::new()));

    let addr = format!("{}:{}", SERVIDOR_IP, SERVIDOR_PUERTO);
    let listener = TcpListener::bind(&addr).expect("No se pudo iniciar el servidor");

    println!("╔══════════════════════════════════════════╗");
    println!("║     Servidor Central — Topología Estrella ║");
    println!("║     Escuchando en {}:{}          ║", SERVIDOR_IP, SERVIDOR_PUERTO);
    println!("╚══════════════════════════════════════════╝\n");

    // Loop infinito: acepta conexiones de clientes
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let nodos_clone = Arc::clone(&nodos);
                thread::spawn(move || {
                    manejar_cliente(stream, nodos_clone);
                });
            }
            Err(e) => eprintln!("[Servidor] Error al aceptar conexión: {}", e),
        }
    }
}

// ─────────────────────────────────────────────────────────
// Maneja cada cliente conectado al servidor
//
// Protocolo:
//   Primer paquete  → "REGISTRO:<puerto_escucha>"
//   Paquetes siguientes → mensajes a reenviar a todos
// ─────────────────────────────────────────────────────────
fn manejar_cliente(stream: TcpStream, nodos: ListaNodos) {
    let peer_ip = stream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "desconocido".into());

    let reader = BufReader::new(stream);
    let mut puerto_cliente: Option<u16> = None;

    for linea in reader.lines() {
        match linea {
            Ok(msg) if !msg.is_empty() => {
                // ── Primer mensaje: registro del nodo ──
                if let Some(puerto_str) = msg.strip_prefix("REGISTRO:") {
                    if let Ok(puerto) = puerto_str.trim().parse::<u16>() {
                        puerto_cliente = Some(puerto);
                        let mut lista = nodos.lock().unwrap();
                        // Evitar duplicados
                        if !lista.iter().any(|(ip, p)| ip == &peer_ip && *p == puerto) {
                            lista.push((peer_ip.clone(), puerto));
                            println!(
                                "[+] Nodo registrado: {}:{}  (total: {})",
                                peer_ip,
                                puerto,
                                lista.len()
                            );
                        }
                    }
                    continue;
                }

                // ── Mensajes normales: broadcast a todos ──
                println!("[MSG] {}:{} → \"{}\"",
                    peer_ip,
                    puerto_cliente.unwrap_or(0),
                    msg
                );

                let lista = nodos.lock().unwrap().clone();
                let remitente_puerto = puerto_cliente.unwrap_or(0);

                for (ip, puerto) in &lista {
                    // No reenviar al mismo nodo que lo mandó
                    if ip == &peer_ip && *puerto == remitente_puerto {
                        continue;
                    }
                    match TcpStream::connect(format!("{}:{}", ip, puerto)) {
                        Ok(mut dest) => {
                            let paquete = format!("[{}:{}] {}\n", peer_ip, remitente_puerto, msg);
                            if let Err(e) = dest.write_all(paquete.as_bytes()) {
                                eprintln!("  ✗ Error enviando a {}:{} — {}", ip, puerto, e);
                            }
                        }
                        Err(e) => {
                            eprintln!("  ✗ No se pudo conectar a {}:{} — {}", ip, puerto, e);
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    // Al desconectarse, eliminar de la lista
    if let Some(puerto) = puerto_cliente {
        let mut lista = nodos.lock().unwrap();
        lista.retain(|(ip, p)| !(ip == &peer_ip && *p == puerto));
        println!("[-] Nodo desconectado: {}:{}  (total: {})", peer_ip, puerto, lista.len());
    }
}
