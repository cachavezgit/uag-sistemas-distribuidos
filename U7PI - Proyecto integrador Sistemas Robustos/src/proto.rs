// ─────────────────────────────────────────────────────────
// proto.rs — Tipos y servicios RPC compartidos entre server y client
//
// RegistryService: expuesto por el servidor de descubrimiento.
// PeerService:     expuesto por cada cliente (P2P directo + push
//                   del servidor, ver notify_* más abajo).
//
// Nota de diseño: tarpc solo permite llamadas cliente→servidor, no hay
// push nativo del servidor sobre el mismo canal. Para lograr un
// directorio en tiempo real (Pub/Sub real) el servidor mantiene una
// conexión tarpc de vuelta hacia el PeerService de cada nodo registrado
// y le llama los métodos notify_* cuando cambia el directorio, los
// grupos, o llega una solicitud/aceptación de videollamada. Por eso
// no existe un `ServerEvent` ni un canal broadcast: el push es RPC
// directo.
// ─────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

use crate::transfer::FileChunk;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub username: String,
    pub emoji: String, // emoji elegido al conectarse, ej. "🦀"
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub members: Vec<String>, // usernames
}

// Servicio que expone el SERVIDOR de descubrimiento a los clientes
#[tarpc::service]
pub trait RegistryService {
    async fn register(info: NodeInfo) -> Result<Vec<NodeInfo>, String>;
    async fn unregister(username: String) -> Result<(), String>;
    async fn create_group(group: GroupInfo) -> Result<(), String>;
    async fn request_video_call(from: String, to: String) -> Result<(), String>;
    async fn accept_video_call(from: String, to: String) -> Result<(), String>;
    async fn get_directory() -> Result<Vec<NodeInfo>, String>;
}

// Servicio que expone cada CLIENTE, tanto a otros clientes (P2P directo)
// como al servidor (para el push de directorio/grupos/videollamada).
#[tarpc::service]
pub trait PeerService {
    async fn send_message(from: String, content: String) -> Result<(), String>;
    async fn send_group_message(from: String, group: String, content: String) -> Result<(), String>;
    async fn send_file_chunk(from: String, chunk: FileChunk) -> Result<(), String>;
    async fn send_video_frame(from: String, jpeg_data: Vec<u8>) -> Result<(), String>;
    async fn game_move(from: String, position: u8) -> Result<(), String>;
    async fn game_invite(from: String) -> Result<(), String>;
    async fn game_accept(from: String) -> Result<(), String>;

    // Push del servidor de descubrimiento (ver nota de diseño arriba)
    async fn notify_directory(nodes: Vec<NodeInfo>) -> Result<(), String>;
    async fn notify_groups(groups: Vec<GroupInfo>) -> Result<(), String>;
    async fn notify_video_call_request(from: String) -> Result<(), String>;
    async fn notify_video_call_accepted(from: String) -> Result<(), String>;
    async fn notify_video_call_ended(from: String) -> Result<(), String>;
}
