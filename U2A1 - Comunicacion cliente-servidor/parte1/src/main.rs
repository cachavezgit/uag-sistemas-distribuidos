use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

// ─────────────────────────────────────────────
// Configuración de los 3 nodos predefinidos
// Cada nodo conoce a los otros dos.
// ─────────────────────────────────────────────
const NODOS: [(&str, u16); 3] = [
    ("127.0.0.1", 8001),
    ("127.0.0.1", 8002),
    ("127.0.0.1", 8003),
];

fn main() {
    // El número de nodo (1, 2 o 3) se pasa como argumento
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

    // Buffer de mensajes compartido entre hilos (para mostrarlo en pantalla)
    let mensajes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mensajes_servidor = Arc::clone(&mensajes);

    println!("╔══════════════════════════════════════════╗");
    println!("║      Chat P2P con Hilos — {}         ║", nombre);
    println!("║  Escuchando en {}:{}             ║", mi_ip, mi_puerto);
    println!("╚══════════════════════════════════════════╝");
    println!("Escribe tu mensaje y presiona Enter para enviarlo.");
    println!("Escribe 'salir' para terminar.\n");

    // ──────────────────────────────────────────
    // HILO DISPLAY — imprime mensajes recibidos
    // continuamente sin esperar input del usuario
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
                    // Un hilo por cada conexión entrante
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

        // Enviar a todos los otros nodos
        let paquete = format!("[{}] {}", nombre, texto);
        let mut enviados = 0;
        let mut fallidos = 0;

        for (i, (ip, puerto)) in NODOS.iter().enumerate() {
            if i == nodo_idx {
                continue; // No me envío a mí mismo
            }
            match enviar_mensaje(ip, *puerto, &paquete) {
                Ok(_) => enviados += 1,
                Err(e) => {
                    eprintln!("  ✗ No se pudo enviar a {}:{} — {}", ip, puerto, e);
                    fallidos += 1;
                }
            }
        }

        println!(
            "  ✓ Enviado a {} nodo(s){}",
            enviados,
            if fallidos > 0 {
                format!(", {} no disponible(s)", fallidos)
            } else {
                String::new()
            }
        );

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
                let entrada = format!("  << {}", msg);
                // Guardar en el buffer compartido
                let mut msgs = mensajes.lock().unwrap();
                msgs.push(entrada);
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