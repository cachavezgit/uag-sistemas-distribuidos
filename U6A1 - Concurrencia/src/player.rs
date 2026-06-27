// ─────────────────────────────────────────────────────────
// player.rs — Reproducción de video con ffplay/mpv como proceso hijo
//
// U6: play_file() reproduce un archivo ya reconstruido en disco.
// U7-ready: open_stream() deja la puerta abierta para alimentar al
// reproductor con chunks en tiempo real vía stdin pipe (videollamada).
// ─────────────────────────────────────────────────────────

use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::{anyhow, Result};

/// Rutas conocidas donde puede vivir un binario, por plataforma.
const KNOWN_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

// ─────────────────────────────────────────────────────────
// Reproductor detectado en el sistema
// ─────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum Player {
    Ffplay(String),
    Mpv(String),
}

impl Player {
    /// Detecta ffplay o mpv disponible en el sistema.
    /// Prioriza ffplay por consistencia con U7 (pipe stdin).
    pub fn detect() -> Result<Self> {
        if let Some(path) = find_binary("ffplay") {
            return Ok(Player::Ffplay(path));
        }
        if let Some(path) = find_binary("mpv") {
            return Ok(Player::Mpv(path));
        }
        Err(anyhow!("No se encontró ffplay ni mpv instalado en el sistema"))
    }

    /// Sin uso en main.rs desde que se quitó el aviso de "Streaming: archivo
    /// con <path>" del panel de memes; se mantiene como capacidad del módulo.
    #[allow(dead_code)]
    pub fn path(&self) -> &str {
        match self {
            Player::Ffplay(p) => p,
            Player::Mpv(p) => p,
        }
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
    // Sin lectores desde que se quitó el aviso del panel de memes; se
    // mantiene para identificar qué reproductor abrió este proceso.
    #[allow(dead_code)]
    pub player: Player,
}

impl PlayerHandle {
    /// U6: Reproduce un archivo ya reconstruido en disco.
    /// Sin uso en main.rs desde que la recepción de video pasó a streaming en
    /// vivo vía `open_stream()`; se mantiene como capacidad probada del módulo.
    #[allow(dead_code)]
    pub fn play_file(path: &Path) -> Result<Self> {
        let player = Player::detect()?;

        let child = match &player {
            Player::Ffplay(bin) => Command::new(bin)
                .args(["-autoexit", "-loglevel", "quiet"])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
            Player::Mpv(bin) => Command::new(bin)
                .arg("--no-terminal")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        };

        Ok(PlayerHandle { child, player })
    }

    /// Abre el reproductor con stdin pipe para streaming en tiempo real.
    /// Retorna el handle y el `ChildStdin` para que main.rs escriba los chunks
    /// descifrados a medida que llegan, sin esperar a tener el archivo completo.
    pub fn open_stream() -> Result<(Self, std::process::ChildStdin)> {
        let player = Player::detect()?;

        let mut child = match &player {
            Player::Ffplay(bin) => Command::new(bin)
                .args(["-i", "pipe:0", "-autoexit", "-loglevel", "quiet"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
            Player::Mpv(bin) => Command::new(bin)
                .args(["--no-terminal", "-"])
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
}

impl Drop for PlayerHandle {
    /// Garantiza que el proceso hijo no quede huérfano.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_encuentra_ffplay() {
        let result = Player::detect();
        assert!(result.is_ok(), "se esperaba encontrar ffplay o mpv instalado");
        assert!(matches!(result, Ok(Player::Ffplay(_))));
    }

    #[test]
    fn find_binary_retorna_ruta_valida() {
        let path = find_binary("ffplay");
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
}
