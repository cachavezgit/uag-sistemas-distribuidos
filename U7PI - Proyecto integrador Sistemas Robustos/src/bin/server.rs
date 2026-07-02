// ─────────────────────────────────────────────────────────
// bin/server.rs — Servidor de descubrimiento (registry)
//
// Etapa 1 (scaffolding): mantiene el directorio de nodos en memoria
// y responde register/unregister/get_directory. El push en tiempo
// real hacia los clientes (notify_directory) y los grupos/videollamada
// se agregan en un commit posterior.
// ─────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::prelude::*;
use tarpc::{
    context,
    server::{self, Channel},
    tokio_serde::formats::Json,
};

use gato_p2p::proto::{GroupInfo, NodeInfo, RegistryService};

const LISTEN_ADDR: &str = "0.0.0.0:9000";

#[derive(Clone, Default)]
struct RegistryState {
    nodes: Arc<Mutex<HashMap<String, NodeInfo>>>,
    #[allow(dead_code)] // usado a partir del commit de grupos
    groups: Arc<Mutex<Vec<GroupInfo>>>,
}

impl RegistryState {
    fn directory_snapshot(&self) -> Vec<NodeInfo> {
        self.nodes.lock().unwrap().values().cloned().collect()
    }
}

#[derive(Clone)]
struct RegistryServer {
    state: RegistryState,
}

impl RegistryService for RegistryServer {
    async fn register(self, _: context::Context, info: NodeInfo) -> Result<Vec<NodeInfo>, String> {
        println!("[Registry] {} ({}) se registró desde {}:{}", info.username, info.emoji, info.ip, info.port);
        self.state.nodes.lock().unwrap().insert(info.username.clone(), info);
        Ok(self.state.directory_snapshot())
    }

    async fn unregister(self, _: context::Context, username: String) -> Result<(), String> {
        println!("[Registry] {} se desconectó", username);
        self.state.nodes.lock().unwrap().remove(&username);
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

    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .for_each(|channel| {
            let server = RegistryServer { state: state.clone() };
            channel.execute(server.serve()).for_each(|response| async move {
                tokio::spawn(response);
            })
        })
        .await;

    Ok(())
}
