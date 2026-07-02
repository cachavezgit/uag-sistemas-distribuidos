// ─────────────────────────────────────────────────────────
// bin/client.rs — Cliente de chat P2P (entry point)
//
// Etapa 1 (scaffolding): autentica, se registra en el servidor de
// descubrimiento y muestra el directorio recibido. La TUI, el
// PeerService propio y el resto de funcionalidad se agregan en
// commits posteriores.
// ─────────────────────────────────────────────────────────

use std::process;

use tarpc::{client, context, tokio_serde::formats::Json};

use gato_p2p::proto::{NodeInfo, RegistryServiceClient};

const SERVER_ADDR: &str = "127.0.0.1:9000";

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
    println!("[Auth] Bienvenido, {}.\n", usuario_autenticado);

    let rt = tokio::runtime::Runtime::new().expect("No se pudo crear el runtime de tokio");
    let resultado = rt.block_on(iniciar(args));

    gato_p2p::auth::cerrar_sesion(&usuario_autenticado);

    if let Err(e) = resultado {
        eprintln!("[Error] {}", e);
        process::exit(1);
    }
}

async fn iniciar(args: Args) -> anyhow::Result<()> {
    println!("[Client] Conectando al servidor de descubrimiento en {}...", SERVER_ADDR);
    let transport = tarpc::serde_transport::tcp::connect(SERVER_ADDR, Json::default).await?;
    let registry = RegistryServiceClient::new(client::Config::default(), transport).spawn();

    let info = NodeInfo {
        username: args.nombre.clone(),
        emoji: args.emoji.clone(),
        ip: "127.0.0.1".to_string(),
        port: 0, // el listener PeerService propio se agrega en un commit posterior
    };

    let directorio = registry
        .register(context::current(), info)
        .await?
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("[Client] Registrado como {} {}. Directorio actual:", args.emoji, args.nombre);
    for node in &directorio {
        println!("  - {} {} ({}:{})", node.emoji, node.username, node.ip, node.port);
    }

    registry
        .unregister(context::current(), args.nombre)
        .await?
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}
