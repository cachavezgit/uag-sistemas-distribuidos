# Sistemas Distribuidos

Repositorio de actividades y ejemplos para la materia **Sistemas Distribuidos** del profesor **Mtro. Diego Enrique Cordero Baltazar**.

## Contenido

### U1A1 — Sockets TCP y UDP

Implementaciones básicas de comunicación cliente-servidor usando la biblioteca `socket` de Python.

#### Socket TCP (`U1A1: Socket TPC/tcp-socket/`)

Comunicación orientada a conexión mediante el protocolo TCP (SOCK_STREAM).

| Archivo | Descripción |
|---|---|
| `tcp-server.py` | Servidor que escucha en `127.0.0.1:8888`, acepta una conexión, recibe el mensaje y responde. |
| `tcp-client.py` | Cliente que se conecta al servidor, envía `¡¡¡HOLA MUNDO!!!` y muestra la respuesta. |

**Cómo ejecutar:**

```bash
# Terminal 1 — iniciar el servidor
python tcp-server.py

# Terminal 2 — ejecutar el cliente
python tcp-client.py
```

#### Socket UDP (`U1A1: Socket TPC/udp-socket/`)

Comunicación sin conexión mediante el protocolo UDP (SOCK_DGRAM).

| Archivo | Descripción |
|---|---|
| `udp-server.py` | Servidor que escucha en `127.0.0.1:9999` en un bucle continuo y responde a cada datagrama recibido. |
| `udp-client.py` | Cliente que envía `¡¡¡HOLA MUNDO!!!` al servidor y muestra la respuesta. |

**Cómo ejecutar:**

```bash
# Terminal 1 — iniciar el servidor
python udp-server.py

# Terminal 2 — ejecutar el cliente
python udp-client.py
```

---

### U2 A1 — Chat P2P con 3 Nodos (Rust)

Implementación de un chat punto a punto entre 3 nodos usando TCP y concurrencia con hilos en Rust.

**Ubicación:** `U2 A1 - Comunicacion cliente-servidor/parte1/`

#### Arquitectura

Cada nodo actúa simultáneamente como servidor (escucha conexiones entrantes) y como cliente (envía mensajes a los otros dos nodos). Los puertos están predefinidos:

| Nodo | Dirección |
|---|---|
| Nodo 1 | `127.0.0.1:8001` |
| Nodo 2 | `127.0.0.1:8002` |
| Nodo 3 | `127.0.0.1:8003` |

Cada instancia lanza tres hilos:
- **Hilo servidor** — acepta conexiones TCP entrantes (un hilo adicional por cada conexión).
- **Hilo display** — imprime los mensajes recibidos desde el buffer compartido.
- **Hilo principal** — lee la entrada del usuario y difunde cada mensaje a los otros dos nodos.

**Cómo ejecutar:**

```bash
cd "U2 A1 - Comunicacion cliente-servidor/parte1"

# Terminal 1 — Nodo 1
cargo run -- 1

# Terminal 2 — Nodo 2
cargo run -- 2

# Terminal 3 — Nodo 3
cargo run -- 3
```

**Formato de mensajes:**

| Comando | Descripción |
|---|---|
| `@<nodo> <mensaje>` | Envío directo a un nodo específico (ej: `@2 hola`) |
| `@all <mensaje>` | Broadcast a todos los demás nodos (ej: `@all hola`) |
| `salir` | Terminar el proceso |

#### `parte2` — Topología Estrella con Servidor Central (`U2 A1 - Comunicacion cliente-servidor/parte2/`)

Variante donde un **servidor central** gestiona el registro de peers y enruta cada mensaje: ya sea a un destinatario específico (comunicación directa) o a todos los peers conectados (broadcast).

**Binarios:**

| Binario | Archivo | Descripción |
|---|---|---|
| `servidor` | `src/servidor.rs` | Servidor central en `127.0.0.1:9000`. Mantiene la lista de peers registrados, enruta mensajes directos a un puerto destino y hace broadcast cuando el destino es `all`. Elimina al peer de la lista cuando se desconecta. |
| `cliente` | `src/cliente.rs` | Peer que abre su propio puerto TCP de escucha, se registra en el servidor central y envía mensajes usando el protocolo de enrutamiento. Reconecta automáticamente si pierde la conexión al servidor. |

**Protocolo:**

| Línea enviada al servidor | Propósito |
|---|---|
| `REGISTRO:<puerto_propio>` | Registrar el peer al conectarse |
| `DESTINO:<puerto>\|<mensaje>` | Enviar a un peer específico por su puerto |
| `DESTINO:all\|<mensaje>` | Broadcast a todos los peers conectados |

**Flujo de enrutamiento:**
1. El cliente se conecta al servidor y envía `REGISTRO:<puerto_propio>`.
2. Para enviar, el cliente escribe `DESTINO:<destino>|<paquete>` donde `<destino>` es un puerto o `all`.
3. El servidor localiza al peer destino en su lista y se conecta a su puerto de escucha para entregarle el mensaje.
4. En broadcast, repite el envío a todos excepto al remitente.
5. Si la conexión al servidor se pierde, el cliente reconecta y se vuelve a registrar automáticamente.

**Cómo ejecutar:**

```bash
cd "U2 A1 - Comunicacion cliente-servidor/parte2"

# Terminal 1 — Servidor central (iniciar primero)
cargo run --bin servidor

# Terminal 2 — Peer en puerto 8001
cargo run --bin cliente -- 8001

# Terminal 3 — Peer en puerto 8002
cargo run --bin cliente -- 8002

# Terminal 4 — Peer en puerto 8003
cargo run --bin cliente -- 8003
```

**Formato de mensajes:**

| Comando | Descripción |
|---|---|
| `@<puerto> <mensaje>` | Envío directo a un peer por su puerto (ej: `@8002 hola`) |
| `@all <mensaje>` | Broadcast a todos los peers conectados (ej: `@all hola`) |
| `salir` | Terminar el proceso |

---

### U3A1 — Juego del Gato P2P con RPC (Rust)

Implementación del juego del gato (Tic-Tac-Toe) en una arquitectura P2P donde cada nodo actúa simultáneamente como servidor y como cliente usando el framework de RPC **tarpc** sobre TCP.

**Ubicación:** `U3A1 - Implementación del juego del gato con RPC en entorno P2P/`

#### Arquitectura

Cada instancia del nodo levanta dos canales independientes al arrancar:

- **Servidor tarpc** — escucha en su propio puerto y expone el método `make_move(casilla)`. Cuando el rival envía un movimiento, tarpc lo ejecuta en este proceso como si fuera una llamada local.
- **Cliente tarpc** — se conecta al puerto del rival y llama `make_move(casilla)` de forma remota para notificar los movimientos propios.

Este esquema es equivalente a Java RMI: la interfaz `TicTacToe` define el contrato, `TicTacToeClient` actúa como stub generado automáticamente y `TicTacToeServer` es el skeleton que ejecuta las llamadas entrantes.

#### Módulos

| Archivo | Descripción |
|---|---|
| `src/main.rs` | Punto de entrada: parseo de argumentos, inicialización del servidor y cliente RPC, loop principal de eventos. |
| `src/rpc.rs` | Define el servicio tarpc `TicTacToe` con los métodos `make_move(casilla: usize)` y `ping()`. tarpc genera el trait del servidor y el stub del cliente automáticamente. |
| `src/network.rs` | Levanta el servidor tarpc en background (`start_server`), crea el cliente con reintentos automáticos (`connect_to_peer`) y envía movimientos remotos (`send_move`). Gestiona el estado compartido entre el servidor RPC y el loop de UI mediante `SharedState`. |
| `src/game.rs` | Lógica del juego: tablero 3×3, validación de movimientos, detección de ganador/empate y registro del historial de jugadas. |
| `src/ui.rs` | Interfaz TUI construida con Ratatui: tablero interactivo, panel de estado, controles y panel lateral con el historial de movimientos de la partida. |

#### Cómo ejecutar

```bash
cd "U3A1 - Implementación del juego del gato con RPC en entorno P2P"

# Terminal 1 — Jugador 1 (escucha en 8001, rival en 8002)
cargo run -- --jugador 1 --escucha 8001 --rival 127.0.0.1:8002

# Terminal 2 — Jugador 2 (escucha en 8002, rival en 8001)
cargo run -- --jugador 2 --escucha 8002 --rival 127.0.0.1:8001
```

Jugador 1 siempre inicia primero y espera 2 segundos para que Jugador 2 levante su servidor antes de intentar conectar.

#### Controles

| Tecla | Acción |
|---|---|
| `1` – `9` | Seleccionar casilla del tablero (distribución igual a un teclado numérico) |
| `R` | Reiniciar partida al terminar |
| `Q` | Salir del juego |

#### Flujo de un movimiento

1. El jugador local presiona una tecla `1-9`.
2. El movimiento se aplica al tablero local.
3. Se invoca `make_move(casilla)` en el peer rival a través de tarpc (RPC real sobre TCP).
4. El servidor del rival almacena el movimiento en `SharedState`.
5. El loop de UI del rival lo detecta en el siguiente frame y actualiza su tablero.

---

### U4A1 — Juego del Gato P2P con Autenticación y Cifrado Vigenère (Rust)

Extensión de U3A1 que añade dos capas de seguridad sobre la comunicación inter-nodos:

1. **Autenticación** — compuerta síncrona y bloqueante antes de que el nodo abra cualquier socket. El operador debe ingresar credenciales válidas contra `usuarios.json`; si falla, el proceso termina sin exponer ningún puerto.
2. **Cifrado Vigenère** — cada movimiento se serializa como texto, se cifra con la contraseña del operador como clave y viaja por la red en forma cifrada. El nodo receptor lo descifra antes de aplicarlo al tablero.

**Ubicación:** `U4A1 - Desarrollo de encriptación simple para comunicación inter-nodos/`

#### Arquitectura

```
main() [síncrona]
  ├─► parse_args()            ← incluye --clave
  ├─► iniciar_log()           ← crea crypto.log
  ├─► auth::autenticar()      ← bloquea aquí; exit(1) si falla
  ├─► tokio::Runtime::new()   ← solo se crea si auth pasó
  │     └─► iniciar_nodo()
  │           ├─► start_server()     ← servidor RPC (descifra entrantes)
  │           ├─► connect_to_peer()  ← cliente RPC (cifra salientes)
  │           └─► run_loop()         ← TUI Ratatui
  └─► auth::cerrar_sesion()   ← siempre se ejecuta al salir
```

#### Módulos

| Archivo | Descripción |
|---|---|
| `src/auth.rs` | Compuerta de autenticación síncrona. Lee `usuarios.json`, valida credenciales por `stdin` y gestiona archivos de lock en `sesiones/` para impedir sesiones duplicadas. |
| `src/crypto.rs` | Cifrado Vigenère sobre ASCII imprimible (32–126). Expone `cifrar(texto, clave)` y `descifrar(texto, clave)`. Incluye 5 tests unitarios. |
| `src/main.rs` | Punto de entrada síncrono: autentica, crea el runtime de Tokio y propaga la clave por todo el call stack hasta `send_move`. |
| `src/rpc.rs` | Define el servicio tarpc. `make_move` cambió su firma de `casilla: usize` a `payload: String` para transportar el texto cifrado. |
| `src/network.rs` | `send_move` cifra la casilla antes de enviarla. `TicTacToeServer::make_move` descifra el payload al recibirlo. Ambas operaciones se registran en `crypto.log` sin tocar stdout. |
| `src/game.rs` | Lógica del juego. Incorpora el campo `usuario: String` para mostrar el nombre del operador autenticado en la UI. |
| `src/ui.rs` | Interfaz TUI con Ratatui. El título muestra `@<usuario>` en verde para identificar la sesión activa. |
| `usuarios.json` | Registro de credenciales válidas en JSON. |

#### Algoritmo de cifrado

Vigenère sobre el rango ASCII imprimible (códigos 32–126, 95 caracteres):

```
Cifrado:    c' = ((c - 32) + (k - 32)) % 95 + 32
Descifrado:  c = ((c' - 32) - (k - 32) + 95) % 95 + 32
```

La clave es la contraseña que el operador introdujo en la fase de autenticación, reutilizándola sin intercambio de claves adicional. Cada movimiento (casilla 0–8) se serializa a `String`, se cifra y viaja por TCP en su forma cifrada.

#### Cómo ejecutar

```bash
cd "U4A1 - Desarrollo de encriptación simple para comunicación inter-nodos"

# Terminal 1 — Jugador 1
cargo run -- --jugador 1 --escucha 8001 --rival 127.0.0.1:8002 --clave misecreta

# Terminal 2 — Jugador 2
cargo run -- --jugador 2 --escucha 8002 --rival 127.0.0.1:8001 --clave misecreta

# Terminal 3 — Ver payloads cifrados en tiempo real (opcional)
tail -f crypto.log
```

Ambos jugadores deben usar la misma `--clave`. Si se omite, el valor por defecto es `clave_defecto`.

#### Controles

| Tecla | Acción |
|---|---|
| `1` – `9` | Seleccionar casilla del tablero |
| `R` | Reiniciar partida al terminar |
| `Q` | Salir del juego y cerrar sesión |

#### Flujo de un movimiento cifrado

1. El jugador local presiona `5`.
2. El movimiento se aplica al tablero local.
3. La casilla (`4`) se serializa a `"4"`, se cifra con Vigenère → p.ej. `"S"`.
4. Se invoca `make_move("S")` en el peer rival a través de tarpc.
5. El servidor del rival recibe `"S"`, lo descifra → `"4"`, parsea a `usize` y aplica al tablero.
6. Ambas operaciones quedan registradas en `crypto.log`.

#### `usuarios.json`

```json
{
  "usuarios": [
    { "usuario": "nodo",     "contrasena": "clave123" },
    { "usuario": "jugador1", "contrasena": "pass1"    },
    { "usuario": "jugador2", "contrasena": "pass2"    },
    { "usuario": "admin",    "contrasena": "admin456" }
  ]
}
```

---

### U5A1 — Juego del Gato P2P con Transferencia de Archivos Cifrada (Rust)

Extensión de U4A1 que añade transferencia de archivos binarios sobre el mismo canal RPC del juego. Los archivos se fragmentan en chunks de 64 KB, se codifican en Base64 y se cifran con Vigenère antes de viajar por la red; el receptor los descifra, decodifica y reconstruye automáticamente en `./recibidos/`.

**Ubicación:** `U5A1 - Envío de archivos/`

#### Arquitectura

Sobre la misma arquitectura P2P de U4A1 (autenticación, cifrado Vigenère, TUI Ratatui) se añaden dos canales Tokio y el método RPC `send_chunk`:

```
run_loop
  ├─► [Tecla M] FileExplorer modal (ratatui-explorer)
  │     └─► [Enter] fragment_and_encrypt(path, clave)
  │               └─► send_file_chunks(client, chunks, progress_tx)
  │                     └─► client.send_chunk(chunk) → ChunkAck  [por cada chunk]
  │
  ├─► chunk_rx  ←  TicTacToeServer::send_chunk (RPC entrante)
  │     └─► decrypt_and_reconstruct(chunks, clave, "./recibidos")
  │
  └─► progress_rx  →  barra de progreso en la TUI
```

#### Pipeline de cifrado de archivos

```
Envío:    bytes → Base64 → Vigenère cifrado → Vec<u8> (ASCII 32–126) → RPC
Recepción: Vec<u8> → Vigenère descifrado → Base64 decode → bytes → archivo
```

Base64 garantiza que el cifrado Vigenère opere únicamente sobre caracteres imprimibles, aunque el archivo contenga bytes binarios arbitrarios.

#### Módulos

| Archivo | Descripción |
|---|---|
| `src/transfer.rs` | **Nuevo.** `FileChunk` (struct serializable con serde), `fragment_and_encrypt` (lee el archivo, lo parte en chunks de 64 KB, Base64-codifica y cifra cada uno) y `decrypt_and_reconstruct` (descifra, decodifica y escribe en `output_dir`). Incluye 5 tests unitarios (round-trip pequeño, multi-chunk, clave incorrecta, 50 MB, ASCII imprimible). |
| `src/rpc.rs` | Añade el método `send_chunk(chunk: FileChunk) -> ChunkAck` al servicio tarpc y define `ChunkAck { chunk_index, ok }`. |
| `src/network.rs` | `SharedState` incorpora `chunk_tx: mpsc::Sender<FileChunk>`. Añade `send_file_chunks` (envía chunks secuencialmente esperando `ChunkAck` de cada uno y reportando progreso) y el enum `TransferProgress { Sending, Done, Error }`. `TicTacToeServer::send_chunk` encola el chunk recibido en `chunk_tx`. |
| `src/main.rs` | Crea los canales `(chunk_tx, chunk_rx)` y `(progress_tx, progress_rx)`. Gestiona el modal de explorador de archivos (`M` lo abre, `Esc` lo cierra, `Enter` dispara la transferencia en un `tokio::spawn`). Reconstruye el archivo cuando llega el último chunk. |
| `src/ui.rs` | Panel de transferencia en la parte inferior de la TUI: muestra barra de progreso con `Gauge` durante el envío y el último evento (enviado/recibido/error). El explorador de archivos se superpone como overlay modal. |
| `src/auth.rs` | Sin cambios respecto a U4A1. |
| `src/crypto.rs` | Sin cambios respecto a U4A1. |
| `src/game.rs` | Sin cambios respecto a U4A1. |

#### Cómo ejecutar

```bash
cd "U5A1 - Envío de archivos"

# Terminal 1 — Jugador 1
cargo run -- --jugador 1 --escucha 8001 --rival 127.0.0.1:8002 --clave misecreta

# Terminal 2 — Jugador 2
cargo run -- --jugador 2 --escucha 8002 --rival 127.0.0.1:8001 --clave misecreta

# Opcional: ver el log de cifrado en tiempo real
tail -f crypto.log
```

Ambos jugadores deben usar la misma `--clave`. Si se omite, el valor por defecto es `clave_defecto`.

#### Controles

| Tecla | Acción |
|---|---|
| `1` – `9` | Seleccionar casilla del tablero |
| `M` | Abrir explorador de archivos para enviar un archivo al rival |
| `Esc` | Cerrar el explorador de archivos sin enviar |
| `Enter` | Confirmar archivo seleccionado e iniciar transferencia |
| `R` | Reiniciar partida al terminar |
| `Q` | Salir del juego y cerrar sesión |

#### Flujo de una transferencia

1. El jugador presiona `M`; se abre el modal de explorador de archivos.
2. Navega con las flechas hasta seleccionar un archivo y presiona `Enter`.
3. `fragment_and_encrypt` lee el archivo, lo parte en chunks de 64 KB, codifica cada uno en Base64 y lo cifra con Vigenère.
4. `send_file_chunks` envía los chunks uno a uno vía `send_chunk` RPC, esperando el `ChunkAck` de cada uno. La TUI muestra una barra de progreso.
5. El receptor encola cada chunk en `chunk_rx` al recibirlo.
6. Cuando llega el chunk con `chunk_index == total_chunks - 1`, `decrypt_and_reconstruct` descifra, decodifica y escribe el archivo en `./recibidos/<nombre_original>`.
7. La TUI del receptor muestra `Recibido: <nombre> → ./recibidos/<nombre>`.

#### Tests unitarios (`src/transfer.rs`)

| Test | Descripción |
|---|---|
| `round_trip_archivo_pequeno` | Cifra y reconstruye un archivo de < 64 KB; verifica contenido idéntico. |
| `round_trip_multi_chunk` | Archivo de 2 chunks completos + 1 parcial; verifica orden y contenido. |
| `clave_incorrecta_produce_contenido_diferente` | Descifrar con clave errónea produce bytes distintos al original. |
| `round_trip_archivo_50mb` | Archivo de 50 MB (800 chunks); verifica conteo y contenido byte a byte. |
| `data_cifrada_es_ascii_imprimible` | Todos los bytes cifrados están en el rango 32–126. |

**Cómo ejecutar los tests:**

```bash
cd "U5A1 - Envío de archivos"

# Ejecutar todos los tests
cargo test

# Ejecutar solo los tests de transfer.rs
cargo test --test-threads=1 -- transfer::

# Ejecutar un test específico (ej. el de 50 MB)
cargo test round_trip_archivo_50mb -- --nocapture
```

> El flag `--test-threads=1` es recomendable porque varios tests escriben archivos temporales en el mismo directorio del sistema operativo (`std::env::temp_dir()`); ejecutarlos en paralelo puede causar colisiones de nombres.

---

### U6A1 — Juego del Gato P2P con Streaming de Video en Tiempo Real (Rust)

Extensión de U5A1 que añade streaming de video P2P en tiempo real. En vez de reconstruir el archivo en disco para luego reproducirlo, cada chunk de video se descifra y se escribe directamente al `stdin` del reproductor (`mpv` o `ffplay`) conforme llega, de modo que la reproducción comienza desde el primer chunk recibido sin esperar a que termine la transmisión.

**Ubicación:** `U6A1 - Concurrencia/`

#### Arquitectura

Sobre la misma base P2P de U5A1 (autenticación, cifrado Vigenère, TUI Ratatui, transferencia por chunks) se añaden el pipeline de streaming y el protocolo de cancelación:

```
[V] abre FileExplorer
  └─► fragment_and_encrypt(video, clave)
        └─► send_file_chunks(client, chunks, progress_tx)
              └─► client.send_chunk(chunk) → ChunkAck      [por cada chunk]
                    └─► ChunkAck.ok = false → detener  ←  receptor canceló con [Q]

[chunk_rx]  ←  TicTacToeServer::send_chunk (RPC entrante)
  ├─► is_video_file() → sí:
  │     ├─► 1er chunk: PlayerHandle::open_stream() → (handle, ChildStdin)
  │     ├─► siguientes: decrypt_chunk_bytes(chunk, clave) → stdin.write_all(bytes)
  │     └─► último chunk: cierra pipe → reproductor termina al vaciar su buffer
  └─► is_video_file() → no: recv_buffer → decrypt_and_reconstruct → ./recibidos/
```

#### Módulos

| Archivo | Descripción |
|---|---|
| `src/player.rs` | **Nuevo.** `enum Player { MpvIpc(bin, socket), Ffplay(bin) }` con `detect()` que prioriza mpv sobre ffplay. `PlayerHandle` con `open_stream()` (pipe de `stdin` para streaming), `stop()`, `is_running()` y `Drop` que mata el proceso hijo. `PlaybackCommand` (`TogglePause`, `SeekForward`, `SeekBackward`, `Restart`) con `to_json()` para el protocolo IPC de mpv. `try_send_ipc()` conecta al socket Unix de mpv con hasta 10 reintentos. 6 tests unitarios. |
| `src/network.rs` | Añade `is_video_file()` (detecta `.mp4 .mkv .avi .mov .webm .mpg .mpeg`), `decrypt_chunk_bytes()` (descifra un chunk individual para streaming en tiempo real), campo `video_cancel: Arc<Mutex<bool>>` en `SharedState` y `set_video_cancel()`. `TicTacToeServer::send_chunk` consulta `video_cancel` y retorna `ChunkAck { ok: false }` para cortar la transmisión cuando el receptor cancela. |
| `src/ui.rs` | `enum VideoState { Inactivo, Explorando, Transmitiendo { chunk_actual, total }, Reproduciendo, Error(String) }`. Panel " VIDEO — Streaming P2P " separado debajo del panel de memes. Panel de controles se adapta dinámicamente: muestra atajos de IPC (`Espacio`, `←/→`, `R`) cuando mpv está activo, o avisa que hay que instalar mpv si el reproductor es ffplay. |
| `src/main.rs` | Tecla `[V]` abre el explorador en modo video. Primer chunk entrante abre `PlayerHandle::open_stream()`; siguientes se escriben al `ChildStdin`; último cierra el pipe. Flag local `video_cancelado` descarta en silencio chunks que ya estaban en el canal `mpsc` antes de que el rechazo RPC surtiera efecto. Tecla `[Q]` con reproductor activo lo detiene y llama `state.set_video_cancel(true)`. |
| `src/auth.rs` | Sin cambios respecto a U5A1. |
| `src/crypto.rs` | Sin cambios respecto a U5A1. |
| `src/game.rs` | Sin cambios respecto a U5A1. |
| `src/transfer.rs` | Sin cambios respecto a U5A1. |
| `src/rpc.rs` | Sin cambios respecto a U5A1. |

#### Cómo ejecutar

```bash
cd "U6A1 - Concurrencia"

# Terminal 1 — Jugador 1
cargo run -- --jugador 1 --escucha 8001 --rival 127.0.0.1:8002 --clave misecreta

# Terminal 2 — Jugador 2
cargo run -- --jugador 2 --escucha 8002 --rival 127.0.0.1:8001 --clave misecreta

# Opcional: ver payloads cifrados en tiempo real
tail -f crypto.log
```

Ambos jugadores deben usar la misma `--clave`. Si se omite, el valor por defecto es `clave_defecto`.

#### Controles

| Tecla | Acción |
|---|---|
| `1` – `9` | Seleccionar casilla del tablero |
| `M` | Abrir explorador de archivos para enviar un meme/archivo al rival |
| `V` | Abrir explorador de archivos para enviar un video al rival (streaming en tiempo real) |
| `Esc` | Cerrar el explorador sin enviar |
| `Enter` | Confirmar archivo seleccionado e iniciar transferencia |
| `Espacio` | Pausar/reanudar video (solo con mpv) |
| `→` / `←` | Avanzar/retroceder ±10 segundos (solo con mpv) |
| `R` | Reiniciar video desde el inicio (solo con mpv) / reiniciar partida al terminar |
| `Q` | Detener reproducción de video (si hay una activa) / salir del juego |

#### Pipeline de streaming en tiempo real

```
Emisor:    bytes → Base64 → Vigenère cifrado → chunks → RPC send_chunk
Receptor:  chunk → Vigenère descifrado → Base64 decode → stdin.write_all(mpv/ffplay)
```

El video empieza a reproducirse desde el **primer chunk recibido**, sin esperar a que llegue el archivo completo. El emisor fragmenta y cifra con el mismo pipeline que en U5A1; el receptor omite la reconstrucción en disco y alimenta los bytes directamente al pipe `stdin` del reproductor.

Para evitar que `stdin.write_all()` (bloqueante cuando el pipe se llena) impida leer eventos de teclado, el loop principal procesa **un solo chunk de video por iteración** (~16 ms), dejando el resto en el buffer del canal `mpsc`.

#### Protocolo de cancelación (receptor → emisor)

Cuando el receptor presiona `[Q]` mientras un video se está transmitiendo:

1. El reproductor local se detiene y el pipe `stdin` se cierra.
2. `state.set_video_cancel(true)` activa el flag en `SharedState`.
3. El siguiente `send_chunk` RPC del emisor encuentra `video_cancel = true` y retorna `ChunkAck { ok: false }`.
4. `send_file_chunks` en el emisor detecta `ok: false` y detiene la transmisión.
5. El flag se resetea a `false` cuando comienza la siguiente transmisión de video.

Un flag local `video_cancelado` descarta en silencio los chunks que ya estaban en el canal antes de que el rechazo RPC surtiera efecto.

#### Tests unitarios (`src/player.rs`)

| Test | Descripción |
|---|---|
| `detect_prioriza_mpv_sobre_ffplay` | Verifica que mpv se detecta y se prioriza sobre ffplay. |
| `find_binary_retorna_ruta_valida` | La ruta retornada por `find_binary` existe en disco. |
| `find_binary_binario_inexistente_retorna_none` | Retorna `None` para binarios no instalados. |
| `play_file_archivo_inexistente_no_panics` | `play_file()` sobre una ruta inexistente no entra en pánico. |
| `send_command_a_socket_inexistente_retorna_err` | `try_send_ipc` retorna error cuando el socket no existe. |
| `playback_command_genera_json_valido` | Cada variante de `PlaybackCommand` genera el JSON IPC correcto. |

**Cómo ejecutar los tests:**

```bash
cd "U6A1 - Concurrencia"

# Todos los tests (player.rs + transfer.rs + crypto.rs)
cargo test

# Solo los tests de player.rs
cargo test -- player::

# Un test específico
cargo test playback_command_genera_json_valido -- --nocapture
```

---

## Requisitos

- Python 3.x (sin dependencias externas, solo biblioteca estándar)
- Rust + Cargo (para las actividades U2A1 en adelante)
- `mpv` (recomendado) o `ffplay` para reproducción de video en U6A1
  - macOS: `brew install mpv`
  - Linux: `sudo apt install mpv -y`
