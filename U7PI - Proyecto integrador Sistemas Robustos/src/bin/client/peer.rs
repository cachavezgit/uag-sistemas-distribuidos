// ─────────────────────────────────────────────────────────
// client/peer.rs — PeerService propio del cliente
//
// Cada cliente levanta su propio servidor tarpc (PeerService) para
// recibir tanto llamadas P2P directas de otros clientes (mensajes,
// archivos, gato, video) como el push de vuelta del servidor de
// descubrimiento (notify_*, ver nota de diseño en proto.rs). Cada
// handler simplemente empuja un ClientEvent al canal que consume el
// loop principal de la TUI.
// ─────────────────────────────────────────────────────────

use futures::prelude::*;
use tarpc::{
    client, context,
    server::{self, Channel},
    tokio_serde::formats::Json,
};
use tokio::sync::mpsc;

use gato_p2p::proto::{GroupInfo, NodeInfo, PeerService, PeerServiceClient};
use gato_p2p::transfer::FileChunk;

use super::app::ClientEvent;

#[derive(Clone)]
pub struct PeerServer {
    pub events: mpsc::Sender<ClientEvent>,
}

impl PeerServer {
    async fn emit(&self, event: ClientEvent) -> Result<(), String> {
        self.events
            .send(event)
            .await
            .map_err(|e| format!("canal de eventos cerrado: {}", e))
    }
}

impl PeerService for PeerServer {
    async fn send_message(self, _: context::Context, from: String, content: String) -> Result<(), String> {
        self.emit(ClientEvent::DirectMessage { from, content }).await
    }

    async fn send_group_message(
        self,
        _: context::Context,
        from: String,
        group: String,
        content: String,
    ) -> Result<(), String> {
        self.emit(ClientEvent::GroupMessage { from, group, content }).await
    }

    async fn send_file_chunk(self, _: context::Context, from: String, chunk: FileChunk) -> Result<(), String> {
        self.emit(ClientEvent::FileChunkReceived { from, chunk }).await
    }

    async fn send_video_frame(self, _: context::Context, from: String, jpeg_data: Vec<u8>) -> Result<(), String> {
        self.emit(ClientEvent::VideoFrame { from, jpeg: jpeg_data }).await
    }

    async fn game_move(self, _: context::Context, from: String, position: u8) -> Result<(), String> {
        self.emit(ClientEvent::GameMove { from, position }).await
    }

    async fn game_invite(self, _: context::Context, from: String) -> Result<(), String> {
        self.emit(ClientEvent::GameInvite { from }).await
    }

    async fn game_accept(self, _: context::Context, from: String) -> Result<(), String> {
        self.emit(ClientEvent::GameAccept { from }).await
    }

    async fn notify_directory(self, _: context::Context, nodes: Vec<NodeInfo>) -> Result<(), String> {
        self.emit(ClientEvent::DirectoryUpdated(nodes)).await
    }

    async fn notify_groups(self, _: context::Context, groups: Vec<GroupInfo>) -> Result<(), String> {
        self.emit(ClientEvent::GroupsUpdated(groups)).await
    }

    async fn notify_video_call_request(self, _: context::Context, from: String) -> Result<(), String> {
        self.emit(ClientEvent::VideoCallRequest { from }).await
    }

    async fn notify_video_call_accepted(self, _: context::Context, from: String) -> Result<(), String> {
        self.emit(ClientEvent::VideoCallAccepted { from }).await
    }

    async fn notify_video_call_ended(self, _: context::Context, _from: String) -> Result<(), String> {
        self.emit(ClientEvent::VideoCallEnded).await
    }

    async fn notify_file_offer(self, _: context::Context, from: String, file_name: String) -> Result<(), String> {
        self.emit(ClientEvent::FileOffer { from, file_name }).await
    }

    async fn notify_file_accepted(self, _: context::Context, _from: String) -> Result<(), String> {
        self.emit(ClientEvent::FileAccepted).await
    }

    async fn notify_file_rejected(self, _: context::Context, _from: String) -> Result<(), String> {
        self.emit(ClientEvent::FileRejected).await
    }
}

/// Bindea un puerto local libre, levanta el listener PeerService en
/// background y retorna el puerto asignado por el SO. Debe llamarse
/// ANTES de registrarse en el servidor de descubrimiento, para que este
/// pueda conectarse de vuelta (push) apenas nos registre.
pub async fn start_listener(events: mpsc::Sender<ClientEvent>) -> anyhow::Result<u16> {
    let mut listener = tarpc::serde_transport::tcp::listen("0.0.0.0:0", Json::default).await?;
    listener.config_mut().max_frame_length(usize::MAX);
    let port = listener.local_addr().port();

    tokio::spawn(async move {
        // Cada canal en su propia tarea (ver comentario equivalente en
        // bin/server.rs): de lo contrario solo el primer peer que nos
        // contacte sería atendido, y toda conexión posterior (otro mensaje,
        // otro peer, el push del servidor) se quedaría esperando para
        // siempre en el backlog de TCP.
        listener
            .filter_map(|r| future::ready(r.ok()))
            .map(server::BaseChannel::with_defaults)
            .for_each(|channel| {
                let server = PeerServer { events: events.clone() };
                tokio::spawn(channel.execute(server.serve()).for_each(|response| async move {
                    tokio::spawn(response);
                }));
                future::ready(())
            })
            .await;
    });

    Ok(port)
}

/// Dial efímero al PeerService de otro nodo. Cada llamada abre una conexión
/// nueva en vez de cachearla — simple y suficientemente rápido para la
/// cadencia de un chat/partida de gato (no para streaming, ver `send_file_to`).
async fn dial(ip: &str, port: u16) -> anyhow::Result<PeerServiceClient> {
    let addr = format!("{}:{}", ip, port);
    let transport = tarpc::serde_transport::tcp::connect(&addr, Json::default).await?;
    Ok(PeerServiceClient::new(client::Config::default(), transport).spawn())
}

pub async fn send_message_to(ip: &str, port: u16, from: String, content: String) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .send_message(context::current(), from, content)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn send_group_message_to(
    ip: &str,
    port: u16,
    from: String,
    group: String,
    content: String,
) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .send_group_message(context::current(), from, group, content)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

/// Envía todos los chunks de un archivo ya fragmentado/cifrado
/// (`transfer::fragment_and_encrypt`) a un peer, sobre una única conexión,
/// esperando el ack de cada chunk antes de mandar el siguiente (igual que
/// `network::send_file_chunks` en el U6).
pub async fn send_file_to(ip: &str, port: u16, from: String, chunks: Vec<FileChunk>) -> anyhow::Result<()> {
    let client = dial(ip, port).await?;

    for chunk in chunks {
        client
            .send_file_chunk(context::current(), from.clone(), chunk)
            .await?
            .map_err(|e| anyhow::anyhow!(e))?;
    }

    Ok(())
}

/// Consume los frames JPEG que produce `video::start_capture` y los manda
/// por RPC sobre una única conexión (igual que `send_file_to`, pero sin
/// esperar acks por frame: la videollamada tolera perder algún frame,
/// esperar ack por cada uno a 15 fps le agregaría latencia innecesaria).
/// Termina solo cuando el canal se cierra (colgar la llamada detiene la
/// captura, que a su vez cierra el canal).
#[cfg_attr(not(feature = "camera"), allow(dead_code))]
pub async fn stream_video_to(
    ip: &str,
    port: u16,
    from: String,
    mut frame_rx: mpsc::Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    let client = dial(ip, port).await?;

    while let Some(jpeg) = frame_rx.recv().await {
        let _ = client.send_video_frame(context::current(), from.clone(), jpeg).await;
    }

    Ok(())
}

pub async fn notify_file_offer_to(ip: &str, port: u16, from: String, file_name: String) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .notify_file_offer(context::current(), from, file_name)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn notify_file_accepted_to(ip: &str, port: u16, from: String) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .notify_file_accepted(context::current(), from)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn notify_file_rejected_to(ip: &str, port: u16, from: String) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .notify_file_rejected(context::current(), from)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn notify_video_call_ended_to(ip: &str, port: u16, from: String) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .notify_video_call_ended(context::current(), from)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn game_invite_to(ip: &str, port: u16, from: String) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .game_invite(context::current(), from)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn game_accept_to(ip: &str, port: u16, from: String) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .game_accept(context::current(), from)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn game_move_to(ip: &str, port: u16, from: String, position: u8) -> anyhow::Result<()> {
    dial(ip, port)
        .await?
        .game_move(context::current(), from, position)
        .await?
        .map_err(|e| anyhow::anyhow!(e))
}
