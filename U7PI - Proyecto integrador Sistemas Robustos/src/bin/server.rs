// ─────────────────────────────────────────────────────────
// bin/server.rs — Servidor de descubrimiento (registry)
//
// Mantiene el directorio de nodos y los grupos en memoria. Además de
// responder las llamadas RPC de RegistryService, mantiene una conexión
// tarpc "de vuelta" (PeerServiceClient) por cada nodo registrado, para
// poder empujarle notify_directory/notify_groups/notify_video_call_*
// apenas cambie el estado — ver la nota de diseño en proto.rs sobre por
// qué el push es RPC inverso y no un canal broadcast compartido.
// ─────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::prelude::*;
use tarpc::{
    client, context,
    server::{self, Channel},
    tokio_serde::formats::Json,
};

use gato_p2p::proto::{GroupInfo, NodeInfo, PeerServiceClient, RegistryService};

const LISTEN_ADDR: &str = "0.0.0.0:9000";

#[derive(Clone, Default)]
struct RegistryState {
    nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,
    #[allow(dead_code)] // usado a partir del commit de grupos
    groups: Arc<Mutex<Vec<GroupInfo>>>,
    peer_clients: Arc<Mutex<HashMap<String, PeerServiceClient>>>,
}

impl RegistryState {
    fn directory_snapshot(&self) -> Vec<NodeInfo> {
        self.nodes.lock().unwrap().values().cloned().collect()
    }

    /// Clones de los clientes cacheados, tomados con el lock liberado antes
    /// de que el llamador haga `.await` sobre cada uno (tarpc's clients son
    /// baratos de clonar, comparten el canal interno).
    fn peer_client_list(&self) -> Vec<PeerServiceClient> {
        self.peer_clients.lock().unwrap().values().cloned().collect()
    }

    /// Conecta (o reconecta) el PeerServiceClient de vuelta hacia `info` y
    /// lo cachea. Best-effort: si el peer no responde todavía, se registra
    /// igual pero sin push hasta el próximo cambio de directorio. Con
    /// timeout para no quedar colgado si el peer no responde ni rechaza.
    async fn connect_peer_client(&self, info: &NodeInfo) {
        let addr = format!("{}:{}", info.ip, info.port);
        let intento = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            tarpc::serde_transport::tcp::connect(&addr, Json::default),
        )
        .await;

        match intento {
            Ok(Ok(transport)) => {
                let client = PeerServiceClient::new(client::Config::default(), transport).spawn();
                self.peer_clients.lock().unwrap().insert(info.username.clone(), client);
            }
            Ok(Err(e)) => {
                eprintln!("[Registry] No se pudo conectar de vuelta a {} ({}): {}", info.username, addr, e);
            }
            Err(_) => {
                eprintln!("[Registry] Timeout conectando de vuelta a {} ({})", info.username, addr);
            }
        }
    }

    async fn broadcast_directory(&self) {
        let nodes = self.directory_snapshot();
        for client in self.peer_client_list() {
            let nodes = nodes.clone();
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                client.notify_directory(context::current(), nodes),
            )
            .await;
        }
    }
}

#[derive(Clone)]
struct RegistryServer {
    state: RegistryState,
}

impl RegistryService for RegistryServer {
    async fn register(self, _: context::Context, info: NodeInfo) -> Result<Vec<NodeInfo>, String> {
        println!("[Registry] {} ({}) se registró desde {}:{}", info.username, info.emoji, info.ip, info.port);
        self.state.nodes.lock().unwrap().insert(info.username.clone(), info.clone());
        let snapshot = self.state.directory_snapshot();

        // Conectar de vuelta y empujar el push en background: si el peer
        // recién registrado (u otro ya cacheado) tarda o falla en responder,
        // no debe bloquear esta respuesta de `register` ni la del resto de
        // los nodos ya conectados.
        let state = self.state.clone();
        tokio::spawn(async move {
            state.connect_peer_client(&info).await;
            state.broadcast_directory().await;
        });

        Ok(snapshot)
    }

    async fn unregister(self, _: context::Context, username: String) -> Result<(), String> {
        println!("[Registry] {} se desconectó", username);
        self.state.nodes.lock().unwrap().remove(&username);
        self.state.peer_clients.lock().unwrap().remove(&username);

        let state = self.state.clone();
        tokio::spawn(async move {
            state.broadcast_directory().await;
        });

        Ok(())
    }

    async fn create_group(self, _: context::Context, _group: GroupInfo) -> Result<(), String> {
        Err("create_group aún no implementado".to_string())
    }

    async fn request_video_call(self, _: context::Context, _from: String, _to: String) -> Result<(), String> {
        Err("request_video_call aún no implementado".to_string())
    }

    async fn accept_video_call(self, _: context::Context, _from: String, _to: String) -> Result<(), String> {
        Err("accept_video_call aún no implementado".to_string())
    }

    async fn get_directory(self, _: context::Context) -> Result<Vec<NodeInfo>, String> {
        Ok(self.state.directory_snapshot())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("[Registry] Escuchando en {}", LISTEN_ADDR);

    let mut listener = tarpc::serde_transport::tcp::listen(LISTEN_ADDR, Json::default).await?;
    listener.config_mut().max_frame_length(usize::MAX);

    let state = RegistryState::default();

    // Cada canal se despacha en su propia tarea: si el `for_each` externo
    // esperara a que un canal terminara (es decir, a que ese cliente se
    // desconectara) antes de seguir, solo el primer cliente conectado sería
    // atendido y el resto se quedaría en el backlog de TCP indefinidamente.
    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .for_each(|channel| {
            let server = RegistryServer { state: state.clone() };
            tokio::spawn(channel.execute(server.serve()).for_each(|response| async move {
                tokio::spawn(response);
            }));
            future::ready(())
        })
        .await;

    Ok(())
}
