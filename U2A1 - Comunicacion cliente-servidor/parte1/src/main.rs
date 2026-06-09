use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

// ─────────────────────────────────────────────
// Configuración de los 3 nodos predefinidos
// ─────────────────────────────────────────────
const NODOS: [(&str, u16); 3] = [
    ("127.0.0.1", 8001),
    ("127.0.0.1", 8002),
    ("127.0.0.1", 8003),
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Uso: {} <numero_nodo>  (1, 2 o 3)", args[0]);
        std::process::exit(1);
    }

    let nodo_idx: usize = args[1].parse::<usize>().expect("Número inválido") - 1;
    if nodo_idx >= NODOS.len() {
        eprintln!("El número de nodo debe ser 1, 2 o 3");
        std::process::exit(1);
    }

    let (mi_ip, mi_puerto) = NODOS[nodo_idx];
    let nombre = format!("Nodo {}", nodo_idx + 1);

    let mensajes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mensajes_servidor = Arc::clone(&mensajes);

    println!("╔══════════════════════════════════════════╗");
    println!("║      Chat P2P con Hilos — {}         ║", nombre);
    println!("║  Escuchando en {}:{}             ║", mi_ip, mi_puerto);
    println!("╚══════════════════════════════════════════╝");
    println!("Formato: @<nodo> <mensaje>   → envío directo  (ej: @2 hola)");
    println!("         @all <mensaje>      → envío a todos  (ej: @all hola)");
    println!("         salir               → terminar\n");

    // ──────────────────────────────────────────
    // HILO DISPLAY
    // ──────────────────────────────────────────
    let mensajes_display = Arc::clone(&mensajes);
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_millis(100));
            let mut msgs = mensajes_display.lock().unwrap();
            for m in msgs.drain(..) {
                println!("{}", m);
            }
        }
    });

    // ──────────────────────────────────────────
    // HILO SERVIDOR — escucha permanente (TCP)
    // ──────────────────────────────────────────
    let addr = format!("{}:{}", mi_ip, mi_puerto);
    let listener = TcpListener::bind(&addr).expect("No se pudo enlazar el puerto");

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let mensajes_clone = Arc::clone(&mensajes_servidor);
                    thread::spawn(move || {
                        manejar_conexion(stream, mensajes_clone);
                    });
                }
                Err(e) => eprintln!("[Servidor] Error al aceptar conexión: {}", e),
            }
        }
    });

    // ──────────────────────────────────────────
    // INTERFAZ DE USUARIO — hilo principal
    // ──────────────────────────────────────────
    let stdin = io::stdin();
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

        // ── Parsear formato: @<destino> <mensaje> ──
        if !texto.starts_with('@') {
            println!("  ⚠ Formato inválido. Usa @<nodo> o @all seguido del mensaje.");
            continue;
        }

        let partes: Vec<&str> = texto[1..].splitn(2, ' ').collect();
        if partes.len() < 2 {
            println!("  ⚠ Falta el mensaje. Ejemplo: @2 hola");
            continue;
        }

        let destino = partes[0];
        let mensaje = partes[1];
        let paquete = format!("[{}] {}", nombre, mensaje);

        if destino.eq_ignore_ascii_case("all") {
            // Enviar a todos los otros nodos
            let mut enviados = 0;
            for (i, (ip, puerto)) in NODOS.iter().enumerate() {
                if i == nodo_idx { continue; }
                match enviar_mensaje(ip, *puerto, &paquete) {
                    Ok(_) => enviados += 1,
                    Err(_) => eprintln!("  ✗ Nodo {} no disponible", i + 1),
                }
            }
            println!("  ✓ Enviado a {} nodo(s)", enviados);
        } else {
            // Envío directo a un nodo específico
            match destino.parse::<usize>() {
                Ok(dest_num) if dest_num >= 1 && dest_num <= NODOS.len() => {
                    let dest_idx = dest_num - 1;
                    if dest_idx == nodo_idx {
                        println!("  ⚠ No puedes enviarte un mensaje a ti mismo.");
                        continue;
                    }
                    let (ip, puerto) = NODOS[dest_idx];
                    match enviar_mensaje(ip, puerto, &paquete) {
                        Ok(_) => println!("  ✓ Enviado a Nodo {}", dest_num),
                        Err(_) => eprintln!("  ✗ Nodo {} no disponible", dest_num),
                    }
                }
                _ => println!("  ⚠ Destino inválido. Usa @1, @2, @3 o @all"),
            }
        }
    }
}

// ──────────────────────────────────────────────
// Maneja una conexión entrante en el servidor
// ──────────────────────────────────────────────
fn manejar_conexion(stream: TcpStream, mensajes: Arc<Mutex<Vec<String>>>) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "desconocido".into());

    let reader = BufReader::new(stream);
    for linea in reader.lines() {
        match linea {
            Ok(msg) if !msg.is_empty() => {
                let mut msgs = mensajes.lock().unwrap();
                msgs.push(format!("  << {}", msg));
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("[Servidor] Error leyendo de {}: {}", peer, e);
                break;
            }
        }
    }
}

// ──────────────────────────────────────────────
// Envía un mensaje TCP a ip:puerto
// ──────────────────────────────────────────────
fn enviar_mensaje(ip: &str, puerto: u16, mensaje: &str) -> io::Result<()> {
    let addr = format!("{}:{}", ip, puerto);
    let mut stream = TcpStream::connect(&addr)?;
    writeln!(stream, "{}", mensaje)?;
    stream.flush()?;
    Ok(())
}
