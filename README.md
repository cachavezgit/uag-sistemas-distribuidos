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

## Requisitos

- Python 3.x (sin dependencias externas, solo biblioteca estándar)
- Rust + Cargo (para la actividad U2 A1)
