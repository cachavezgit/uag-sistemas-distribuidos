use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

const SERVIDOR_IP: &str = "127.0.0.1";
const SERVIDOR_PUERTO: u16 = 9000;

type ListaNodos = Arc<Mutex<Vec<(String, u16)>>>;

fn main() {
    let nodos: ListaNodos = Arc::new(Mutex::new(Vec::new()));

    let addr = format!("{}:{}", SERVIDOR_IP, SERVIDOR_PUERTO);
    let listener = TcpListener::bind(&addr).expect("No se pudo iniciar el servidor");

    println!("╔══════════════════════════════════════════╗");
    println!("║     Servidor Central — Topología Estrella ║");
    println!("║     Escuchando en {}:{}          ║", SERVIDOR_IP, SERVIDOR_PUERTO);
    println!("╚══════════════════════════════════════════╝\n");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let nodos_clone = Arc::clone(&nodos);
                thread::spawn(move || manejar_cliente(stream, nodos_clone));
            }
            Err(e) => eprintln!("[Servidor] Error al aceptar conexión: {}", e),
        }
    }
}

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

                // ── Registro del nodo ──
                if let Some(puerto_str) = msg.strip_prefix("REGISTRO:") {
                    if let Ok(puerto) = puerto_str.trim().parse::<u16>() {
                        puerto_cliente = Some(puerto);
                        let mut lista = nodos.lock().unwrap();
                        if !lista.iter().any(|(ip, p)| ip == &peer_ip && *p == puerto) {
                            lista.push((peer_ip.clone(), puerto));
                            println!("[+] Registrado: {}:{}  (total: {})", peer_ip, puerto, lista.len());
                        }
                    }
                    continue;
                }

                // ── Mensaje con destino: "DESTINO:<destino>|<paquete>" ──
                if let Some(resto) = msg.strip_prefix("DESTINO:") {
                    let partes: Vec<&str> = resto.splitn(2, '|').collect();
                    if partes.len() < 2 { continue; }

                    let destino = partes[0];
                    let paquete = partes[1];
                    let remitente_puerto = puerto_cliente.unwrap_or(0);

                    println!("[MSG] {}:{} → @{} → \"{}\"", peer_ip, remitente_puerto, destino, paquete);

                    let lista = nodos.lock().unwrap().clone();

                    if destino.eq_ignore_ascii_case("all") {
                        // Broadcast a todos excepto el remitente
                        for (ip, puerto) in &lista {
                            if ip == &peer_ip && *puerto == remitente_puerto { continue; }
                            reenviar(ip, *puerto, paquete);
                        }
                    } else {
                        // Envío directo al puerto indicado
                        if let Ok(dest_puerto) = destino.parse::<u16>() {
                            for (ip, puerto) in &lista {
                                if *puerto == dest_puerto {
                                    reenviar(ip, *puerto, paquete);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    // Eliminar al desconectarse
    if let Some(puerto) = puerto_cliente {
        let mut lista = nodos.lock().unwrap();
        lista.retain(|(ip, p)| !(ip == &peer_ip && *p == puerto));
        println!("[-] Desconectado: {}:{}  (total: {})", peer_ip, puerto, lista.len());
    }
}

fn reenviar(ip: &str, puerto: u16, mensaje: &str) {
    match TcpStream::connect(format!("{}:{}", ip, puerto)) {
        Ok(mut dest) => {
            if let Err(e) = writeln!(dest, "{}", mensaje) {
                eprintln!("  ✗ Error enviando a {}:{} — {}", ip, puerto, e);
            }
        }
        Err(e) => eprintln!("  ✗ No se pudo conectar a {}:{} — {}", ip, puerto, e),
    }
}
