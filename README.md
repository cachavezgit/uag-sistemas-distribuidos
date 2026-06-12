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

## Requisitos

- Python 3.x (sin dependencias externas, solo biblioteca estándar)
- Rust + Cargo (para las actividades U2A1, U3A1 y U4A1)
