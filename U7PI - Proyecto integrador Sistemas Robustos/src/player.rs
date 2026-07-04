// ─────────────────────────────────────────────────────────
// player.rs — Reproducción de video con mpv (IPC) o ffplay como proceso hijo
//
// open_stream() abre el reproductor con stdin pipe para streaming en tiempo
// real. Si mpv está disponible, usa --input-ipc-server para habilitar
// controles interactivos (pausa, seek, reinicio) vía socket Unix.
// ffplay se mantiene como fallback funcional sin controles IPC.
// ─────────────────────────────────────────────────────────

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};

/// Rutas conocidas donde puede vivir un binario, por plataforma.
const KNOWN_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

// ─────────────────────────────────────────────────────────
// Reproductor detectado en el sistema
// ─────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum Player {
    Ffplay(String),
    /// mpv con ruta al binario y ruta al socket IPC que se usará para esa sesión
    MpvIpc(String, String), // (ruta_binario, ruta_socket)
}

impl Player {
    /// Detecta mpv o ffplay disponible en el sistema.
    /// Prioriza mpv por sus controles IPC; ffplay es fallback funcional sin controles.
    pub fn detect() -> Result<Self> {
        if let Some(path) = find_binary("mpv") {
            let socket_path = format!(
                "/tmp/mpv-socket-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            );
            return Ok(Player::MpvIpc(path, socket_path));
        }
        if let Some(path) = find_binary("ffplay") {
            return Ok(Player::Ffplay(path));
        }
        Err(anyhow!(
            "No se encontró mpv ni ffplay. Instala con:\n\
             - macOS: brew install mpv\n\
             - Linux: sudo apt install mpv -y"
        ))
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &str {
        match self {
            Player::Ffplay(p) => p,
            Player::MpvIpc(p, _) => p,
        }
    }

    /// true si el reproductor soporta controles interactivos vía socket IPC.
    pub fn supports_ipc_control(&self) -> bool {
        matches!(self, Player::MpvIpc(_, _))
    }
}

/// Busca `name` en rutas hardcodeadas conocidas y, si no aparece,
/// recurre a `which` como fallback.
fn find_binary(name: &str) -> Option<String> {
    for dir in KNOWN_DIRS {
        let candidate = format!("{}/{}", dir, name);
        if Path::new(&candidate).exists() {
            return Some(candidate);
        }
    }

    let output = Command::new("which").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

// ─────────────────────────────────────────────────────────
// Proceso hijo del reproductor
// ─────────────────────────────────────────────────────────
pub struct PlayerHandle {
    child: Child,
    pub player: Player,
}

impl PlayerHandle {
    /// U6: Reproduce un archivo ya reconstruido en disco.
    #[allow(dead_code)]
    pub fn play_file(path: &Path) -> Result<Self> {
        let player = Player::detect()?;

        let child = match &player {
            Player::MpvIpc(bin, _) => Command::new(bin)
                .arg("--no-terminal")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
            Player::Ffplay(bin) => Command::new(bin)
                .args(["-autoexit", "-loglevel", "quiet"])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        };

        Ok(PlayerHandle { child, player })
    }

    /// Abre mpv/ffplay configurado para recibir frames JPEG crudos concatenados
    /// (MJPEG) en tiempo real. Fuerza el demuxer MJPEG y desactiva el buffer
    /// para latencia mínima — a diferencia de `open_stream()` que deja que mpv
    /// auto-detecte el formato (útil para archivos, pero bufferiza mucho en vivo).
    pub fn open_mjpeg_stream() -> Result<(Self, std::process::ChildStdin)> {
        let player = Player::detect()?;

        let mut child = match &player {
            Player::MpvIpc(bin, socket_path) => Command::new(bin)
                .args([
                    "--no-terminal",
                    "--demuxer=lavf",
                    "--demuxer-lavf-format=mjpeg",
                    // probesize=32: libavformat solo necesita 3 bytes (FFD8FF) para
                    // identificar JPEG; el default de 5 MB genera ~5 s de buffer.
                    "--demuxer-lavf-probesize=32",
                    // analyzeduration=0: salta el análisis de fps/bitrate del stream.
                    "--demuxer-lavf-analyzeduration=0",
                    "--no-cache",
                    "--cache-pause=no",
                    "--framedrop=vo",
                    "--stream-buffer-size=4096",
                    "--demuxer-readahead-secs=0",
                    "--demuxer-max-bytes=131072",
                    &format!("--input-ipc-server={}", socket_path),
                    "-",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
            Player::Ffplay(bin) => Command::new(bin)
                .args([
                    "-f", "mjpeg",
                    "-probesize", "32",
                    "-analyzeduration", "0",
                    "-flags", "low_delay",
                    "-framedrop",
                    "-loglevel", "quiet",
                    "-i", "pipe:0",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        };

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("No se pudo obtener el stdin del reproductor"))?;

        Ok((PlayerHandle { child, player }, stdin))
    }

    /// Abre el reproductor con stdin pipe para streaming en tiempo real.
    /// Con mpv, registra el socket IPC para habilitar controles de playback.
    pub fn open_stream() -> Result<(Self, std::process::ChildStdin)> {
        let player = Player::detect()?;

        let mut child = match &player {
            Player::MpvIpc(bin, socket_path) => Command::new(bin)
                .args([
                    "--no-terminal",
                    &format!("--input-ipc-server={}", socket_path),
                    "-",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
            Player::Ffplay(bin) => Command::new(bin)
                .args(["-i", "pipe:0", "-autoexit", "-loglevel", "quiet"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        };

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("No se pudo obtener el stdin del reproductor"))?;

        Ok((PlayerHandle { child, player }, stdin))
    }

    /// true si el proceso hijo sigue corriendo.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Termina el reproductor (usado desde la TUI con [Q]).
    pub fn stop(&mut self) -> Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        Ok(())
    }

    /// Envía un comando de control al reproductor vía socket IPC.
    /// No-op silencioso si el reproductor activo es Ffplay (sin soporte IPC).
    pub fn send_command(&self, cmd: PlaybackCommand) -> Result<()> {
        let socket_path = match &self.player {
            Player::MpvIpc(_, socket_path) => socket_path,
            Player::Ffplay(_) => return Ok(()),
        };
        try_send_ipc(socket_path, &cmd.to_json())
    }
}

impl Drop for PlayerHandle {
    /// Garantiza que el proceso hijo no quede huérfano y limpia el socket temporal.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Player::MpvIpc(_, socket_path) = &self.player {
            let _ = std::fs::remove_file(socket_path);
        }
    }
}

// ─────────────────────────────────────────────────────────
// Comandos de control de playback vía IPC
// ─────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy)]
pub enum PlaybackCommand {
    TogglePause,
    SeekForward,  // +10s
    SeekBackward, // -10s
    Restart,      // seek 0 absolute
}

impl PlaybackCommand {
    pub fn to_json(self) -> String {
        match self {
            PlaybackCommand::TogglePause => r#"{"command": ["cycle", "pause"]}"#.to_string(),
            PlaybackCommand::SeekForward => r#"{"command": ["seek", "10"]}"#.to_string(),
            PlaybackCommand::SeekBackward => r#"{"command": ["seek", "-10"]}"#.to_string(),
            PlaybackCommand::Restart => r#"{"command": ["seek", "0", "absolute"]}"#.to_string(),
        }
    }
}

/// Conecta al socket IPC de mpv y envía un comando JSON.
/// Reintenta hasta 10 veces con 50 ms de pausa, ya que el socket puede no
/// existir inmediatamente después de que mpv arranca.
pub fn try_send_ipc(socket_path: &str, json_cmd: &str) -> Result<()> {
    let mut last_err = None;
    for _ in 0..10 {
        match UnixStream::connect(socket_path) {
            Ok(mut stream) => {
                stream.write_all(format!("{}\n", json_cmd).as_bytes())?;
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(anyhow!(
        "No se pudo conectar al socket IPC de mpv: {:?}",
        last_err
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_prioriza_mpv_sobre_ffplay() {
        let result = Player::detect();
        assert!(result.is_ok(), "se esperaba encontrar mpv o ffplay instalado");
        assert!(matches!(result.unwrap(), Player::MpvIpc(_, _)));
    }

    #[test]
    fn find_binary_retorna_ruta_valida() {
        let path = find_binary("mpv").or_else(|| find_binary("ffplay"));
        assert!(path.is_some());
        assert!(Path::new(&path.unwrap()).exists());
    }

    #[test]
    fn find_binary_binario_inexistente_retorna_none() {
        let result = find_binary("reproductor_que_no_existe_xyz");
        assert!(result.is_none());
    }

    #[test]
    fn play_file_archivo_inexistente_no_panics() {
        let path = Path::new("/tmp/video_que_no_existe_u6test.mp4");
        let result = PlayerHandle::play_file(path);
        let _ = result;
    }

    #[test]
    fn send_command_a_socket_inexistente_retorna_err() {
        let result = try_send_ipc(
            "/tmp/socket-que-no-existe-u6test",
            r#"{"command":["seek","0","absolute"]}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn playback_command_genera_json_valido() {
        let cases = [
            (PlaybackCommand::TogglePause, "cycle"),
            (PlaybackCommand::SeekForward, "10"),
            (PlaybackCommand::SeekBackward, "-10"),
            (PlaybackCommand::Restart, "absolute"),
        ];
        for (cmd, expected) in cases {
            let json = cmd.to_json();
            assert!(
                json.contains(expected),
                "JSON '{}' debería contener '{}'",
                json,
                expected
            );
        }
    }
}
