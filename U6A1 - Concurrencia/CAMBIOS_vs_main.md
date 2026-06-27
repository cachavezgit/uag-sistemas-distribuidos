# Cambios en `feature/streaming-video` vs `main`

Comparación: `git diff main...feature/streaming-video`
Punto de divergencia: `fda5d9d` ("Agregué el folder U6A1...") — main y la rama comparten ese commit como base.

4 commits, 4 archivos tocados dentro de `U6A1 - Concurrencia/src/`, 421 líneas insertadas / 26 eliminadas:

| Archivo | Cambio |
|---|---|
| `src/player.rs` | nuevo — 183 líneas |
| `src/main.rs` | modificado — +148 / -26 |
| `src/ui.rs` | modificado — +98 |
| `src/network.rs` | modificado — +18 |

```
d628890 Agregué la tecla V para enviar video, lancé el reproductor al recibir VideoReady y la tecla Q para detenerlo
b3ca8b8 Agregué VideoState y el panel de video en ui.rs, y conecté video_state desde TransferProgress en main.rs
3858320 Se modificó main.rs para incluir VideoError VideoReady y network.rs para agregar esos dos a TransferProgress
57cd318 Agregué el player.rs con la funcionalidad mínima y pruebas unitarias. Hice el import en main
```

---

## Resumen funcional

`main` tenía el juego del gato P2P con cifrado Vigenère y transferencia de archivos por chunks (memes vía tecla `[M]`). Esta rama agrega **streaming de video**: al recibir un archivo de video completo, se reproduce automáticamente con `ffplay`/`mpv`, y se puede enviar video con la tecla `[V]` igual que se envían memes con `[M]`.

No es streaming en vivo — el video se reproduce **después** de recibirse y reconstruirse por completo. La base para streaming en tiempo real (U7) ya quedó preparada en `player.rs` (`open_stream()`) pero sin conectar a `network.rs` todavía.

---

## `src/player.rs` — nuevo módulo

Encapsula el proceso hijo del reproductor de video.

- `enum Player { Ffplay(String), Mpv(String) }` — variante detectada, con la ruta al binario.
  - `Player::detect()` busca `ffplay` primero (por consistencia con el pipe de stdin que usará U7), luego `mpv`.
- `find_binary(name)` — busca en `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`; si no está en ninguna, cae a `which <name>`.
- `struct PlayerHandle { child: Child, player: Player }`
  - `play_file(path)` — lanza el reproductor sobre un archivo ya en disco (`ffplay -autoexit -loglevel quiet <path>` / `mpv --no-terminal <path>`). Es lo que usa U6.
  - `open_stream()` — abre el reproductor con `stdin` en modo pipe (`ffplay -i pipe:0 ...` / `mpv -- -`) y devuelve `(PlayerHandle, ChildStdin)`. **Sin uso en U6**, marcado `#[allow(dead_code)]`; pensado para que U7 escriba chunks de video en vivo a ese pipe.
  - `is_running()` — `try_wait()` sin bloquear, para detectar si el proceso ya terminó solo.
  - `stop()` — `kill()` + `wait()`.
  - `impl Drop` — mismo `kill()` + `wait()`, para que un `PlayerHandle` abandonado nunca deje un proceso huérfano.
- 4 tests unitarios cubriendo detección de binario y manejo de rutas inválidas sin pánico.

## `src/network.rs`

- `TransferProgress` gana dos variantes: `VideoReady(PathBuf)` (video reconstruido, listo para reproducir) y `VideoError(String)`.
- Nueva función pública `is_video_file(file_name: &str) -> bool`, que mira la extensión (`.mp4 .mkv .avi .mov .webm .mpg .mpeg`) para decidir si un archivo recibido es video o no (p. ej. un meme).

## `src/ui.rs`

- Nuevo `enum VideoState { Inactivo, Explorando, Transmitiendo { chunk_actual, total }, Reconstruyendo, Reproduciendo, Error(String) }`, con `Inactivo` como valor por defecto (`#[derive(Default)]` + `#[default]`).
- `TransferState` gana el campo `video_state: VideoState`.
- El layout principal pasa de 2 a 3 filas: zona de juego, panel de memes (sin cambios) y un **panel de video nuevo** debajo, con su propio borde, color según estado (azul inactivo/explorando, amarillo reconstruyendo, verde reproduciendo, rojo error) y una barra de progreso (`Gauge`) mientras se transmite.

## `src/main.rs`

- `mod player;` y se importa `PlayerHandle`.
- Reconstrucción de archivos recibidos: ahora se distingue si el archivo es video (`network::is_video_file`). Si lo es, se emite `TransferProgress::VideoReady`/`VideoError` por el canal de progreso en vez de solo anotar `last_event` (que sigue siendo el comportamiento de los memes, sin cambios).
- Nuevo flag local `video_mode: bool` que recuerda si el explorador/transferencia en curso es de video o de memes, para enrutar `Sending`/`Done`/`Error` hacia `video_state` o hacia `progress`/`last_event` según corresponda.
- **Tecla `[V]`**: abre el mismo `FileExplorer` que memes; al confirmar con Enter, valida que el archivo elegido sea video (el explorador no soporta filtrar por extensión de forma nativa, así que se valida después de elegir) y dispara el mismo flujo de `fragment_and_encrypt` + `send_file_chunks` que ya usaban los memes.
- Al recibir `VideoReady`, se llama `PlayerHandle::play_file(path)`: si arranca, se guarda el handle y el estado pasa a `Reproduciendo`; si falla, `VideoState::Error`.
- **Tecla `[Q]`**: antes solo salía del juego. Ahora, si hay un video reproduciéndose, `Q` lo detiene (`handle.stop()`) y deja el estado en `Inactivo`; si no hay video activo, sigue saliendo de la app como siempre.
- Chequeo por tick con `is_running()` para notar cuando el reproductor termina solo (`ffplay -autoexit` al acabar el video) y limpiar el estado automáticamente.

---

## Decisiones de diseño notables

- **`Q` hace doble función** (detener video / salir de la app) en lugar de agregar una tecla nueva, para no introducir un atajo adicional no contemplado originalmente. Si se prefiere separarlos, sería una tecla dedicada (p. ej. `[X]`) solo para detener el video.
- **Sin streaming en vivo todavía**: el video se reproduce solo después de reconstruirse completo en disco. `open_stream()` queda listo en `player.rs` para que U7 lo conecte a la recepción de chunks en tiempo real.
- **Validación de extensión post-selección**: como `ratatui-explorer` 0.2 no permite filtrar la lista de archivos por extensión, la tecla `[V]` abre el mismo explorador sin restricciones y valida la extensión al confirmar.
