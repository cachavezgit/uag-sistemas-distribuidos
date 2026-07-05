// ─────────────────────────────────────────────────────────
// client/app.rs — Estado de la aplicación TUI
// ─────────────────────────────────────────────────────────

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui_explorer::FileExplorer;

use gato_p2p::player::PlayerHandle;
use gato_p2p::proto::{GroupInfo, NodeInfo};
use gato_p2p::transfer::FileChunk;

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "mpg", "mpeg"];

/// true si `file_name` tiene una extensión de video conocida (para
/// autoreproducir con mpv al terminar de recibirlo).
pub fn is_video_file(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Eventos que el `PeerService` propio (ver `client/peer.rs`) empuja hacia
/// el loop principal de la TUI a través de un canal `mpsc`.
pub enum ClientEvent {
    DirectMessage { from: String, content: String },
    GroupMessage { from: String, group: String, content: String },
    FileChunkReceived { from: String, chunk: FileChunk },
    /// Mensaje de estado genérico (progreso/errores de una tarea en
    /// background, p. ej. una transferencia de archivo) que se agrega al
    /// historial de `target` como si lo mandara "Sistema".
    SystemMessage { target: String, content: String },
    VideoFrame { from: String, jpeg: Vec<u8> },
    GameMove { from: String, position: u8 },
    GameInvite { from: String },
    GameAccept { from: String },
    DirectoryUpdated(Vec<NodeInfo>),
    GroupsUpdated(Vec<GroupInfo>),
    VideoCallRequest { from: String },
    VideoCallAccepted { from: String },
    VideoCallEnded,
    FileOffer { from: String, file_name: String },
    FileAccepted,
    FileRejected,
}

pub struct AppState {
    pub my_info: NodeInfo,
    pub camera_index: u32,
    pub directory: Vec<NodeInfo>, // actualizado en tiempo real por push del servidor
    pub groups: Vec<GroupInfo>,
    pub selected_contact: Option<String>, // username o nombre de grupo
    pub chats: HashMap<String, Vec<ChatMessage>>, // historial por contacto/grupo
    pub input_buffer: String,
    pub mode: AppMode,
    pub focus: Focus,
    pub file_explorer: Option<FileExplorer>,
    /// Some() mientras el prompt inline de creación de grupo está abierto.
    pub group_creation: Option<GroupCreationState>,
    /// Chunks acumulados de una transferencia en curso, por (remitente, nombre de archivo).
    pub file_recv_buffers: HashMap<(String, String), Vec<FileChunk>>,
    /// Canales de streaming de video activos: descifra cada chunk on-the-fly
    /// y lo manda al hilo escritor que alimenta el stdin de mpv/ffplay.
    pub video_stream_senders: HashMap<(String, String), std::sync::mpsc::Sender<Vec<u8>>>,
    /// Reproductores mpv/ffplay lanzados para archivos recibidos; se
    /// conservan aquí para que `Drop` no los mate apenas termina la función
    /// que los abrió (`Drop` de `PlayerHandle` mata el proceso).
    pub active_players: Vec<PlayerHandle>,
    pub game_state: Option<GameState>,
    /// Username al que le mandé una invitación de gato y todavía no responde.
    pub pending_game_invite_sent: Option<String>,
    /// Username que me invitó a jugar y todavía no acepté/rechacé.
    pub pending_game_invite: Option<String>,
    pub video_active: bool,
    /// Some() mientras hay una videollamada en curso (enviando y/o recibiendo).
    pub video_session: Option<VideoSession>,
    /// Username que me invitó a una videollamada y todavía no acepté/rechacé.
    pub pending_video_call_invite: Option<String>,
    /// Oferta de archivo entrante pendiente de respuesta (receptor).
    pub pending_file_offer: Option<PendingFileOffer>,
    /// Canal para notificar al task de envío que el receptor aceptó (emisor).
    pub pending_file_send: Option<tokio::sync::oneshot::Sender<()>>,
    pub should_quit: bool,
}

#[derive(PartialEq, Debug)]
pub enum AppMode {
    Chat,
    Game,
    FileExplorer,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Focus {
    Contacts,
    Input,
}

pub struct ChatMessage {
    pub from: String,
    pub content: String,
    pub timestamp: String,
}

pub struct GameState {
    pub board: [Option<char>; 9],
    pub my_symbol: char,
    pub opponent: String,
    pub my_turn: bool,
}

/// Estado de una videollamada activa. El reproductor en sí se guarda en
/// `AppState.active_players`. El `frame_tx` alimenta un hilo dedicado que
/// escribe al pipe del reproductor sin bloquear el event loop de la TUI —
/// si el hilo escritor está ocupado, el frame se descarta (`try_send`), lo
/// cual es aceptable para video en vivo.
pub struct VideoSession {
    pub peer: String,
    pub frame_tx: Option<std::sync::mpsc::SyncSender<Vec<u8>>>,
    /// Compartido con el hilo de captura (`video::start_capture`); ponerlo
    /// en `true` le pide que se detenga al colgar.
    pub stop_capture: Arc<AtomicBool>,
}

/// Oferta de archivo pendiente de aceptar/rechazar (receptor).
pub struct PendingFileOffer {
    pub from: String,
    pub file_name: String,
}

/// Estado del prompt inline de `[G]` — primero el nombre, luego el
/// checklist de contactos conectados con `[Espacio]` para marcar.
pub struct GroupCreationState {
    pub step: GroupCreationStep,
    pub name: String,
    pub candidates: Vec<String>,
    pub selected: HashSet<String>,
    pub cursor: usize,
}

#[derive(PartialEq)]
pub enum GroupCreationStep {
    NamingGroup,
    SelectingMembers,
}

impl AppState {
    pub fn new(my_info: NodeInfo, directory: Vec<NodeInfo>, camera_index: u32) -> Self {
        let mut app = AppState {
            my_info,
            camera_index,
            directory,
            groups: Vec::new(),
            selected_contact: None,
            chats: HashMap::new(),
            input_buffer: String::new(),
            mode: AppMode::Chat,
            focus: Focus::Contacts,
            file_explorer: None,
            group_creation: None,
            file_recv_buffers: HashMap::new(),
            video_stream_senders: HashMap::new(),
            active_players: Vec::new(),
            game_state: None,
            pending_game_invite_sent: None,
            pending_game_invite: None,
            video_active: false,
            video_session: None,
            pending_video_call_invite: None,
            pending_file_offer: None,
            pending_file_send: None,
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

    /// Recibe un chunk de archivo. Para videos, descifra on-the-fly y hace
    /// streaming a mpv/ffplay desde el primer chunk (sin guardar en disco).
    /// Para otros archivos, acumula todos los chunks, reconstruye y guarda
    /// en `./recibidos/` al llegar el último.
    pub fn receive_file_chunk(&mut self, from: String, chunk: FileChunk) {
        let file_name = chunk.file_name.clone();
        let key = (from.clone(), file_name.clone());
        let is_last = chunk.chunk_index + 1 == chunk.total_chunks;

        if is_video_file(&file_name) {
            // ── Ruta streaming: descifrar y pipar on-the-fly a mpv ──────────

            // Primer chunk: abrir el reproductor y lanzar el hilo escritor.
            if chunk.chunk_index == 0 {
                match PlayerHandle::open_stream() {
                    Ok((handle, stdin)) => {
                        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                        std::thread::spawn(move || {
                            use std::io::Write;
                            let mut stdin = stdin;
                            while let Ok(data) = rx.recv() {
                                if stdin.write_all(&data).is_err() {
                                    break;
                                }
                            }
                            // Al cerrarse el canal (último chunk enviado), stdin
                            // se descarta → mpv recibe EOF y termina solo.
                        });
                        self.active_players.push(handle);
                        self.video_stream_senders.insert(key.clone(), tx);
                        self.record_message(
                            from.clone(),
                            "Sistema".to_string(),
                            format!("📹 Reproduciendo '{}' en tiempo real...", file_name),
                        );
                    }
                    Err(e) => {
                        self.record_message(
                            from,
                            "Sistema".to_string(),
                            format!("❌ No se pudo abrir el reproductor para '{}': {}", file_name, e),
                        );
                        return;
                    }
                }
            }

            // Descifrar este chunk y mandarlo al hilo escritor.
            match gato_p2p::transfer::decrypt_single_chunk(chunk.data, gato_p2p::CLAVE_VIGENERE) {
                Ok(raw) => {
                    if let Some(tx) = self.video_stream_senders.get(&key) {
                        let _ = tx.send(raw);
                    }
                }
                Err(e) => {
                    self.record_message(
                        from.clone(),
                        "Sistema".to_string(),
                        format!("❌ Error descifrando chunk {}: {}", chunk.chunk_index, e),
                    );
                    self.video_stream_senders.remove(&key);
                    return;
                }
            }

            if is_last {
                // Cerrar el canal → hilo escritor sale → mpv recibe EOF.
                self.video_stream_senders.remove(&key);
            }
        } else {
            // ── Ruta archivo: acumular, reconstruir y guardar en disco ───────

            self.file_recv_buffers.entry(key.clone()).or_default().push(chunk);

            if !is_last {
                return;
            }

            let Some(mut chunks) = self.file_recv_buffers.remove(&key) else {
                self.record_message(from.clone(), "Sistema".to_string(),
                    "⚠️ Error interno: buffer no encontrado".to_string());
                return;
            };
            chunks.sort_by_key(|c| c.chunk_index);

            match gato_p2p::transfer::decrypt_and_reconstruct(
                &chunks, gato_p2p::CLAVE_VIGENERE, "./recibidos",
            ) {
                Ok(path) => {
                    let abs_path = std::fs::canonicalize(&path)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| path.clone());
                    self.record_message(
                        from.clone(),
                        from,
                        format!("📎 Archivo recibido: {} → {}", file_name, abs_path),
                    );
                }
                Err(e) => {
                    self.record_message(
                        from,
                        "Sistema".to_string(),
                        format!("❌ Error recibiendo {}: {}", file_name, e),
                    );
                }
            }
        }
    }

    /// Limpia reproductores que ya terminaron (evita que `active_players`
    /// crezca indefinidamente durante una sesión larga).
    pub fn sweep_finished_players(&mut self) {
        self.active_players.retain_mut(|h| h.is_running());
    }

    /// Marca que le mandé una invitación de gato a `target` y deja constancia
    /// en el chat. El envío del RPC lo hace el llamador (necesita red).
    pub fn mark_game_invite_sent(&mut self, target: String) {
        self.pending_game_invite_sent = Some(target.clone());
        self.record_message(target.clone(), "Sistema".to_string(), format!("🎮 Invitación de gato enviada a {}...", target));
    }

    /// Registra una invitación entrante de `from` y selecciona su chat para
    /// que el aviso "[A] Aceptar" sea visible de inmediato.
    pub fn receive_game_invite(&mut self, from: String) {
        self.record_message(
            from.clone(),
            "Sistema".to_string(),
            format!("🎮 {} te invita a jugar al gato. [A] para aceptar (con foco en Contactos).", from),
        );
        self.pending_game_invite = Some(from.clone());
        self.selected_contact = Some(from);
    }

    /// El peer `from` aceptó nuestra invitación a jugar: arranca la partida
    /// como jugador `X` (el que invita siempre empieza). Ignora aceptaciones
    /// que no corresponden a una invitación pendiente nuestra.
    pub fn start_game_as_inviter(&mut self, from: String) {
        if self.pending_game_invite_sent.as_deref() != Some(from.as_str()) {
            return;
        }
        self.pending_game_invite_sent = None;
        self.game_state = Some(GameState {
            board: [None; 9],
            my_symbol: 'X',
            opponent: from.clone(),
            my_turn: true,
        });
        self.mode = AppMode::Game;
        self.record_message(from, "Sistema".to_string(), "🎮 ¡Partida iniciada! Tu turno.".to_string());
    }

    /// Acepté la invitación de `from`: arranca la partida como jugador `O`.
    pub fn start_game_as_acceptor(&mut self, from: String) {
        self.pending_game_invite = None;
        self.game_state = Some(GameState {
            board: [None; 9],
            my_symbol: 'O',
            opponent: from.clone(),
            my_turn: false,
        });
        self.mode = AppMode::Game;
        self.record_message(from, "Sistema".to_string(), "🎮 ¡Partida iniciada! Esperando movimiento rival.".to_string());
    }

    /// Aplica mi jugada local si es válida (mi turno, casilla libre, 0-8).
    /// Retorna `(ip, puerto, mi_username)` del rival para que el llamador
    /// dispare el RPC `game_move` — separado porque `AppState` no conoce
    /// tokio/red.
    pub fn play_local_move(&mut self, position: u8) -> Option<(String, u16, String)> {
        let idx = position as usize;
        if idx >= 9 {
            return None;
        }

        let opponent = {
            let game = self.game_state.as_mut()?;
            if !game.my_turn || game.board[idx].is_some() {
                return None;
            }
            game.board[idx] = Some(game.my_symbol);
            game.my_turn = false;
            game.opponent.clone()
        };

        self.check_game_result();

        let node = self.find_node(&opponent)?;
        Some((node.ip.clone(), node.port, self.my_info.username.clone()))
    }

    /// Aplica la jugada recibida del rival (RPC `game_move`).
    pub fn apply_remote_move(&mut self, from: String, position: u8) {
        let idx = position as usize;
        if idx >= 9 {
            return;
        }
        {
            let Some(game) = self.game_state.as_mut() else { return };
            if game.opponent != from || game.board[idx].is_some() {
                return;
            }
            let rival_symbol = if game.my_symbol == 'X' { 'O' } else { 'X' };
            game.board[idx] = Some(rival_symbol);
            game.my_turn = true;
        }
        self.check_game_result();
    }

    /// Abandona la partida en curso (tecla `[Esc]` en `AppMode::Game`), si hay alguna.
    pub fn abandon_game(&mut self) {
        if let Some(game) = self.game_state.take() {
            self.record_message(game.opponent, "Sistema".to_string(), "🚪 Abandonaste la partida de gato.".to_string());
        }
        self.mode = AppMode::Chat;
    }

    /// Revisa si la partida en curso ya tiene ganador o terminó en empate;
    /// si es así, deja constancia en el chat y vuelve a `AppMode::Chat`.
    fn check_game_result(&mut self) {
        let Some(game) = self.game_state.as_ref() else { return };
        let ganador = gato_p2p::game::verificar_ganador(&game.board);
        let empate = ganador.is_none() && game.board.iter().all(|c| c.is_some());

        if ganador.is_none() && !empate {
            return;
        }

        let opponent = game.opponent.clone();
        let mensaje = match ganador {
            Some(symbol) if symbol == game.my_symbol => "🎉 ¡Ganaste la partida de gato!".to_string(),
            Some(_) => "😞 Perdiste la partida de gato.".to_string(),
            None => "🤝 ¡Empate en la partida de gato!".to_string(),
        };

        self.game_state = None;
        self.mode = AppMode::Chat;
        self.record_message(opponent, "Sistema".to_string(), mensaje);
    }

    /// `[G]` — abre el prompt inline de creación de grupo con los contactos
    /// conectados como candidatos.
    pub fn start_group_creation(&mut self) {
        let candidates = self
            .directory
            .iter()
            .filter(|n| n.username != self.my_info.username)
            .map(|n| n.username.clone())
            .collect();
        self.group_creation = Some(GroupCreationState {
            step: GroupCreationStep::NamingGroup,
            name: String::new(),
            candidates,
            selected: HashSet::new(),
            cursor: 0,
        });
    }

    /// Mueve el cursor del checklist de miembros (con wraparound).
    pub fn group_creation_move(&mut self, delta: isize) {
        let Some(gc) = self.group_creation.as_mut() else { return };
        if gc.candidates.is_empty() {
            return;
        }
        let len = gc.candidates.len() as isize;
        gc.cursor = (((gc.cursor as isize + delta) % len + len) % len) as usize;
    }

    /// `[Espacio]` — marca/desmarca el candidato bajo el cursor.
    pub fn group_creation_toggle_current(&mut self) {
        let Some(gc) = self.group_creation.as_mut() else { return };
        let Some(username) = gc.candidates.get(gc.cursor) else { return };
        if !gc.selected.remove(username) {
            gc.selected.insert(username.clone());
        }
    }

    /// `[Enter]` en el paso de selección de miembros: si el nombre y al
    /// menos un miembro son válidos, cierra el prompt y retorna el
    /// `GroupInfo` (con el creador incluido) para que el llamador dispare
    /// `create_group` por RPC. Si no es válido, deja el prompt abierto.
    pub fn finish_group_creation(&mut self) -> Option<GroupInfo> {
        let gc = self.group_creation.as_ref()?;
        if gc.name.trim().is_empty() || gc.selected.is_empty() {
            return None;
        }
        let gc = self.group_creation.take().unwrap();
        let mut members: Vec<String> = gc.selected.into_iter().collect();
        members.push(self.my_info.username.clone());
        Some(GroupInfo { name: gc.name, members })
    }

    /// Registra una oferta de archivo de `from` y selecciona su chat
    /// para que el aviso "[A]/[R]" sea visible de inmediato.
    pub fn receive_file_offer(&mut self, from: String, file_name: String) {
        self.record_message(
            from.clone(),
            "Sistema".to_string(),
            format!("📎 {} quiere enviarte '{}'. [A] Aceptar  [R] Rechazar (foco Contactos).", from, file_name),
        );
        self.pending_file_offer = Some(PendingFileOffer { from: from.clone(), file_name });
        self.selected_contact = Some(from);
    }

    /// Registra una solicitud entrante de videollamada de `from` y
    /// selecciona su chat para que el aviso "[A] Aceptar" sea visible.
    pub fn receive_video_call_request(&mut self, from: String) {
        self.record_message(
            from.clone(),
            "Sistema".to_string(),
            format!("📹 {} quiere hacer videollamada. [A] para aceptar (con foco en Contactos).", from),
        );
        self.pending_video_call_invite = Some(from.clone());
        self.selected_contact = Some(from);
    }

    /// Arranca (o reinicia) la sesión de videollamada con `peer` — llamado
    /// tanto por quien acepta (apenas presiona `[A]`) como por quien invitó
    /// (apenas le llega la confirmación `notify_video_call_accepted`).
    /// Retorna la bandera de corte para que el llamador arranque la
    /// captura/envío en background (`AppState` no conoce tokio/red).
    pub fn start_video_call(&mut self, peer: String) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        self.video_active = true;
        self.record_message(peer.clone(), "Sistema".to_string(), format!("📹 Videollamada iniciada con {}.", peer));
        self.video_session = Some(VideoSession { peer, frame_tx: None, stop_capture: stop.clone() });
        stop
    }

    /// Aplica un frame de video recibido: al primer frame abre el reproductor
    /// y lanza un hilo dedicado que escribe al pipe de mpv/ffplay. Los frames
    /// siguientes se mandan al hilo vía `try_send` (sin bloquear el event loop).
    /// Si el hilo escritor está ocupado (mpv tarda en consumir) se descarta el
    /// frame — video en vivo tolera perder alguno.
    pub fn receive_video_frame(&mut self, from: String, jpeg: Vec<u8>) {
        let matches_peer = self.video_session.as_ref().map(|s| s.peer == from).unwrap_or(false);
        if !matches_peer {
            return;
        }

        let needs_player = self.video_session.as_ref().is_some_and(|s| s.frame_tx.is_none());
        if needs_player {
            match PlayerHandle::open_mjpeg_stream() {
                Ok((handle, stdin)) => {
                    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(2);
                    std::thread::spawn(move || {
                        use std::io::Write;
                        let mut stdin = stdin;
                        while let Ok(frame) = rx.recv() {
                            if stdin.write_all(&frame).is_err() {
                                break;
                            }
                        }
                        // stdin cae aquí → mpv recibe EOF y reproduce lo que tenga en buffer
                    });
                    self.video_session.as_mut().unwrap().frame_tx = Some(tx);
                    self.active_players.push(handle);
                }
                Err(e) => {
                    self.record_message(from, "Sistema".to_string(), format!("No se pudo abrir el reproductor de video: {}", e));
                    return;
                }
            }
        }

        // Envía el frame al hilo escritor sin bloquear; descarta si está ocupado.
        if let Some(tx) = self.video_session.as_ref().and_then(|s| s.frame_tx.as_ref()) {
            let _ = tx.try_send(jpeg);
        }
    }

    /// `[Esc]` con una llamada activa: detiene la captura local y limpia
    /// el estado. El reproductor (si ya se abrió) se deja terminar solo al
    /// cerrarse el pipe, igual que el streaming de video del U6.
    pub fn end_video_call(&mut self) {
        if let Some(session) = self.video_session.take() {
            session.stop_capture.store(true, Ordering::Relaxed);
            self.record_message(session.peer, "Sistema".to_string(), "📹 Videollamada finalizada.".to_string());
        }
        self.video_active = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn node(username: &str) -> NodeInfo {
        NodeInfo {
            username: username.to_string(),
            emoji: "🙂".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 0,
        }
    }

    /// Simula la partida completa entre dos `AppState` (sin red ni TUI real):
    /// invitación, aceptación, y las jugadas alternadas se "transmiten" a
    /// mano de un estado al otro, igual que haría el RPC real. Cubre el
    /// mismo estado que ejercita `client.rs`, pero de forma determinística.
    #[test]
    fn partida_completa_invitar_aceptar_y_ganar() {
        let mut ivan = AppState::new(node("Ivan"), vec![node("Ivan"), node("Carlos")], 0);
        let mut carlos = AppState::new(node("Carlos"), vec![node("Ivan"), node("Carlos")], 0);

        // Ivan invita a Carlos ("/gato")
        ivan.mark_game_invite_sent("Carlos".to_string());
        assert_eq!(ivan.pending_game_invite_sent.as_deref(), Some("Carlos"));

        // Carlos recibe la invitación y la acepta ("[A]")
        carlos.receive_game_invite("Ivan".to_string());
        assert_eq!(carlos.pending_game_invite.as_deref(), Some("Ivan"));
        carlos.start_game_as_acceptor("Ivan".to_string());
        assert!(carlos.pending_game_invite.is_none());

        // Ivan recibe la aceptación y arranca como X con el primer turno
        ivan.start_game_as_inviter("Carlos".to_string());
        {
            let game = ivan.game_state.as_ref().unwrap();
            assert_eq!(game.my_symbol, 'X');
            assert!(game.my_turn);
        }
        {
            let game = carlos.game_state.as_ref().unwrap();
            assert_eq!(game.my_symbol, 'O');
            assert!(!game.my_turn);
        }

        // X: 0, O: 3, X: 1, O: 4, X: 2 → X gana la fila superior (0,1,2)
        for &(posicion, es_ivan) in &[(0u8, true), (3, false), (1, true), (4, false), (2, true)] {
            if es_ivan {
                let (ip, port, from) = ivan.play_local_move(posicion).expect("jugada de Ivan debe ser válida");
                assert_eq!((ip.as_str(), port, from.as_str()), ("127.0.0.1", 0, "Ivan"));
                carlos.apply_remote_move("Ivan".to_string(), posicion);
            } else {
                let (ip, port, from) = carlos.play_local_move(posicion).expect("jugada de Carlos debe ser válida");
                assert_eq!((ip.as_str(), port, from.as_str()), ("127.0.0.1", 0, "Carlos"));
                ivan.apply_remote_move("Carlos".to_string(), posicion);
            }
        }

        // La partida terminó para ambos: game_state vuelve a None, modo Chat
        assert!(ivan.game_state.is_none());
        assert_eq!(ivan.mode, AppMode::Chat);
        assert!(carlos.game_state.is_none());
        assert_eq!(carlos.mode, AppMode::Chat);

        let ivan_msgs = &ivan.chats["Carlos"];
        let carlos_msgs = &carlos.chats["Ivan"];
        assert!(ivan_msgs.iter().any(|m| m.content.contains("Ganaste")));
        assert!(carlos_msgs.iter().any(|m| m.content.contains("Perdiste")));
    }

    #[test]
    fn jugada_fuera_de_turno_se_ignora() {
        let mut carlos = AppState::new(node("Carlos"), vec![node("Ivan"), node("Carlos")], 0);
        carlos.start_game_as_acceptor("Ivan".to_string()); // O, my_turn = false
        assert!(carlos.play_local_move(0).is_none());
        assert!(carlos.game_state.unwrap().board[0].is_none());
    }

    #[test]
    fn abandonar_partida_limpia_estado_y_avisa() {
        let mut ivan = AppState::new(node("Ivan"), vec![node("Ivan"), node("Carlos")], 0);
        ivan.mark_game_invite_sent("Carlos".to_string());
        ivan.start_game_as_inviter("Carlos".to_string());
        assert!(ivan.game_state.is_some());

        ivan.abandon_game();

        assert!(ivan.game_state.is_none());
        assert_eq!(ivan.mode, AppMode::Chat);
        assert!(ivan.chats["Carlos"].iter().any(|m| m.content.contains("Abandonaste")));
    }

    #[test]
    fn crear_grupo_arma_group_info_con_creador_incluido() {
        let mut ivan = AppState::new(
            node("Ivan"),
            vec![node("Ivan"), node("Carlos"), node("Sofia")],
            0,
        );

        ivan.start_group_creation();
        assert_eq!(ivan.group_creation.as_ref().unwrap().candidates, vec!["Carlos", "Sofia"]);

        // Escribe el nombre y confirma (simulando el teclado de client.rs)
        for c in "Equipo".chars() {
            ivan.group_creation.as_mut().unwrap().name.push(c);
        }
        ivan.group_creation.as_mut().unwrap().step = GroupCreationStep::SelectingMembers;

        // Navega y marca a Carlos y Sofia
        ivan.group_creation_toggle_current(); // Carlos (cursor=0)
        ivan.group_creation_move(1);
        ivan.group_creation_toggle_current(); // Sofia (cursor=1)

        let group = ivan.finish_group_creation().expect("debe crear el grupo con 2 miembros marcados");
        assert_eq!(group.name, "Equipo");

        let mut members = group.members;
        members.sort();
        assert_eq!(members, vec!["Carlos", "Ivan", "Sofia"]);
        assert!(ivan.group_creation.is_none());
    }

    #[test]
    fn crear_grupo_sin_miembros_no_finaliza() {
        let mut ivan = AppState::new(node("Ivan"), vec![node("Ivan"), node("Carlos")], 0);
        ivan.start_group_creation();
        ivan.group_creation.as_mut().unwrap().name = "VacioClub".to_string();
        ivan.group_creation.as_mut().unwrap().step = GroupCreationStep::SelectingMembers;

        assert!(ivan.finish_group_creation().is_none());
        assert!(ivan.group_creation.is_some(), "el prompt debe seguir abierto si no hay miembros marcados");
    }

    #[test]
    fn group_creation_move_hace_wraparound() {
        let mut ivan = AppState::new(
            node("Ivan"),
            vec![node("Ivan"), node("Carlos"), node("Sofia")],
            0,
        );
        ivan.start_group_creation();
        assert_eq!(ivan.group_creation.as_ref().unwrap().cursor, 0);

        ivan.group_creation_move(-1); // de 0 hacia atrás → último
        assert_eq!(ivan.group_creation.as_ref().unwrap().cursor, 1);

        ivan.group_creation_move(1);
        assert_eq!(ivan.group_creation.as_ref().unwrap().cursor, 0);
    }

    #[test]
    fn recibir_mensaje_de_grupo_usa_el_nombre_del_grupo_como_chat() {
        let mut carlos = AppState::new(node("Carlos"), vec![node("Ivan"), node("Carlos")], 0);
        carlos.groups.push(GroupInfo {
            name: "Equipo".to_string(),
            members: vec!["Ivan".to_string(), "Carlos".to_string()],
        });

        carlos.record_message("Equipo".to_string(), "Ivan".to_string(), "hola equipo".to_string());

        assert!(carlos.contact_keys().contains(&"Equipo".to_string()));
        assert!(carlos.chats["Equipo"].iter().any(|m| m.content == "hola equipo"));
    }

    #[test]
    fn recibir_solicitud_de_videollamada_marca_pendiente_y_selecciona_contacto() {
        let mut carlos = AppState::new(node("Carlos"), vec![node("Ivan"), node("Carlos")], 0);
        carlos.receive_video_call_request("Ivan".to_string());

        assert_eq!(carlos.pending_video_call_invite.as_deref(), Some("Ivan"));
        assert_eq!(carlos.selected_contact.as_deref(), Some("Ivan"));
        assert!(carlos.chats["Ivan"].iter().any(|m| m.content.contains("videollamada")));
    }

    #[test]
    fn start_video_call_activa_sesion_y_bandera() {
        let mut ivan = AppState::new(node("Ivan"), vec![node("Ivan"), node("Carlos")], 0);
        assert!(!ivan.video_active);

        let stop = ivan.start_video_call("Carlos".to_string());

        assert!(ivan.video_active);
        assert_eq!(ivan.video_session.as_ref().unwrap().peer, "Carlos");
        assert!(!stop.load(Ordering::Relaxed));
    }

    #[test]
    fn end_video_call_detiene_captura_y_limpia_estado() {
        let mut ivan = AppState::new(node("Ivan"), vec![node("Ivan"), node("Carlos")], 0);
        let stop = ivan.start_video_call("Carlos".to_string());

        ivan.end_video_call();

        assert!(!ivan.video_active);
        assert!(ivan.video_session.is_none());
        assert!(stop.load(Ordering::Relaxed), "debe pedirle al hilo de captura que se detenga");
        assert!(ivan.chats["Carlos"].iter().any(|m| m.content.contains("finalizada")));
    }

    #[test]
    fn frame_de_video_de_otro_peer_se_ignora() {
        let mut ivan = AppState::new(node("Ivan"), vec![node("Ivan"), node("Carlos"), node("Sofia")], 0);
        ivan.start_video_call("Carlos".to_string());

        // Un frame que dice venir de "Sofia" no debería tocar la sesión con Carlos.
        ivan.receive_video_frame("Sofia".to_string(), vec![0u8, 1, 2]);

        assert!(ivan.video_session.as_ref().unwrap().frame_tx.is_none());
    }
}
