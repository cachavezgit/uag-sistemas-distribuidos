// ─────────────────────────────────────────────────────────
// client/app.rs — Estado de la aplicación TUI
// ─────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use gato_p2p::proto::{GroupInfo, NodeInfo};

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

    /// Agrega un mensaje al historial del chat seleccionado (echo local).
    /// El envío real por RPC se conecta en el commit 4.
    pub fn push_local_message(&mut self, content: String) {
        let Some(target) = self.selected_contact.clone() else { return };
        let content = gato_p2p::emoji::procesar(&content);
        self.chats.entry(target).or_default().push(ChatMessage {
            from: self.my_info.username.clone(),
            content,
            timestamp: timestamp_now(),
        });
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
