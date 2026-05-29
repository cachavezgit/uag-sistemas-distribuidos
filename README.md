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

Escribe un mensaje y presiona Enter para enviarlo a los otros nodos. Escribe `salir` para terminar.

---

## Requisitos

- Python 3.x (sin dependencias externas, solo biblioteca estándar)
- Rust + Cargo (para la actividad U2 A1)
