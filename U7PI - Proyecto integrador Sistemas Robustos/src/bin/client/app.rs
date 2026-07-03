// ─────────────────────────────────────────────────────────
// client/app.rs — Estado de la aplicación TUI
// ─────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use gato_p2p::proto::{GroupInfo, NodeInfo};
use gato_p2p::transfer::FileChunk;

/// Eventos que el `PeerService` propio (ver `client/peer.rs`) empuja hacia
/// el loop principal de la TUI a través de un canal `mpsc`. Se declaran
/// todas las variantes desde el commit 4 aunque el loop todavía solo
/// consuma `DirectMessage`/`DirectoryUpdated` — así los commits
/// siguientes (archivos, gato, grupos, video) solo agregan un `match`
/// arm nuevo en `client.rs` en vez de tocar `peer.rs` cada vez.
pub enum ClientEvent {
    DirectMessage { from: String, content: String },
    #[allow(dead_code)] // consumido a partir del commit de grupos
    GroupMessage { from: String, group: String, content: String },
    #[allow(dead_code)] // consumido a partir del commit de transferencia de archivos
    FileChunkReceived(FileChunk),
    #[allow(dead_code)] // consumido a partir del commit de videollamada
    VideoFrame { from: String, jpeg: Vec<u8> },
    #[allow(dead_code)] // consumido a partir del commit de gato embebido
    GameMove { from: String, position: u8 },
    #[allow(dead_code)]
    GameInvite { from: String },
    #[allow(dead_code)]
    GameAccept { from: String },
    DirectoryUpdated(Vec<NodeInfo>),
    #[allow(dead_code)] // consumido a partir del commit de grupos
    GroupsUpdated(Vec<GroupInfo>),
    #[allow(dead_code)] // consumido a partir del commit de videollamada
    VideoCallRequest { from: String },
    #[allow(dead_code)]
    VideoCallAccepted { from: String },
}

pub struct AppState {
    pub my_info: NodeInfo,
    pub directory: Vec<NodeInfo>, // actualizado por push del servidor a partir del commit 4
    pub groups: Vec<GroupInfo>,
    pub selected_contact: Option<String>, // username o nombre de grupo
    pub chats: HashMap<String, Vec<ChatMessage>>, // historial por contacto/grupo
    pub input_buffer: String,
    #[allow(dead_code)] // usado a partir del commit de gato/archivos
    pub mode: AppMode,
    pub focus: Focus,
    #[allow(dead_code)] // usado a partir del commit de transferencia de archivos
    pub file_explorer_open: bool,
    #[allow(dead_code)] // usado a partir del commit de gato embebido
    pub game_state: Option<GameState>,
    #[allow(dead_code)] // usado a partir del commit de videollamada
    pub video_active: bool,
    pub should_quit: bool,
}

#[derive(PartialEq)]
pub enum AppMode {
    Chat,
    #[allow(dead_code)]
    Game,
    #[allow(dead_code)]
    FileExplorer,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Focus {
    Contacts,
    Input,
}

pub struct ChatMessage {
    pub from: String,
    pub content: String,
    pub timestamp: String,
}

#[allow(dead_code)] // usado a partir del commit de gato embebido
pub struct GameState {
    pub board: [Option<char>; 9],
    pub my_symbol: char,
    pub opponent: String,
    pub my_turn: bool,
}

impl AppState {
    pub fn new(my_info: NodeInfo, directory: Vec<NodeInfo>) -> Self {
        let mut app = AppState {
            my_info,
            directory,
            groups: Vec::new(),
            selected_contact: None,
            chats: HashMap::new(),
            input_buffer: String::new(),
            mode: AppMode::Chat,
            focus: Focus::Contacts,
            file_explorer_open: false,
            game_state: None,
            video_active: false,
            should_quit: false,
        };
        // Selecciona el primer contacto disponible, si hay alguno.
        if let Some(first) = app.contact_keys().first() {
            app.selected_contact = Some(first.clone());
        }
        app
    }

    /// Lista combinada de claves de chat navegables: usernames (excepto el
    /// propio) seguidos de nombres de grupo, en ese orden — refleja el
    /// panel CONTACTOS / GRUPOS del layout.
    pub fn contact_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .directory
            .iter()
            .filter(|n| n.username != self.my_info.username)
            .map(|n| n.username.clone())
            .collect();
        keys.extend(self.groups.iter().map(|g| g.name.clone()));
        keys
    }

    pub fn is_group(&self, key: &str) -> bool {
        self.groups.iter().any(|g| g.name == key)
    }

    pub fn select_next(&mut self) {
        let keys = self.contact_keys();
        if keys.is_empty() {
            self.selected_contact = None;
            return;
        }
        let idx = self
            .selected_contact
            .as_ref()
            .and_then(|s| keys.iter().position(|k| k == s))
            .map(|i| (i + 1) % keys.len())
            .unwrap_or(0);
        self.selected_contact = Some(keys[idx].clone());
    }

    pub fn select_prev(&mut self) {
        let keys = self.contact_keys();
        if keys.is_empty() {
            self.selected_contact = None;
            return;
        }
        let idx = self
            .selected_contact
            .as_ref()
            .and_then(|s| keys.iter().position(|k| k == s))
            .map(|i| if i == 0 { keys.len() - 1 } else { i - 1 })
            .unwrap_or(0);
        self.selected_contact = Some(keys[idx].clone());
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Contacts => Focus::Input,
            Focus::Input => Focus::Contacts,
        };
    }

    /// Agrega un mensaje al historial de `target` (propio o recibido de un
    /// peer). El envío/recepción real por RPC vive en `client.rs`/`peer.rs`;
    /// esto solo actualiza el estado ya procesado.
    pub fn record_message(&mut self, target: String, from: String, content: String) {
        self.chats.entry(target).or_default().push(ChatMessage {
            from,
            content,
            timestamp: timestamp_now(),
        });
    }

    /// Nodo del directorio para un username dado (para resolver ip:puerto
    /// antes de una llamada P2P directa).
    pub fn find_node(&self, username: &str) -> Option<&NodeInfo> {
        self.directory.iter().find(|n| n.username == username)
    }
}

pub fn timestamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs_of_day = secs % 86400;
    format!("{:02}:{:02}", secs_of_day / 3600, (secs_of_day % 3600) / 60)
}
