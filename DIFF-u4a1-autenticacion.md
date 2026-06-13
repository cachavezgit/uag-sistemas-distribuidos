# Diferencias: `feature/u4a1-autenticacion` vs `main`

## Resumen

Esta rama introduce el proyecto **U4A1** completo — un juego del gato P2P con RPC al que se le añadió una **capa de autenticación bloqueante** antes de que el nodo abra cualquier socket. La rama agrega 9 archivos nuevos (2 315 líneas en total) y no modifica ningún archivo existente en `main`.

---

## Commits incluidos (más reciente primero)

| Hash | Mensaje |
|------|---------|
| `a8c2c20` | Merge branch 'main' into feature/u4a1-autenticacion |
| `d437176` | Validación para evitar que se inicie sesión con el mismo usuario que ya está activo |
| `a079291` | Se mantiene la sesión del usuario y se muestra en la UI |
| `f0359d6` | Se agregaron cambios para implementar autenticación antes de comenzar la partida |
| `4017a42` | Copia del proyecto U3A1 como base para la cuarta unidad |

---

## Archivos nuevos

Todos los archivos viven bajo `U4A1 - Desarrollo de encriptación simple para comunicación inter-nodos/`.

| Archivo | Líneas | Responsabilidad |
|---------|--------|-----------------|
| `src/auth.rs` | 100 | Compuerta de autenticación síncrona |
| `src/game.rs` | 171 | Lógica del juego del gato |
| `src/main.rs` | 234 | Punto de entrada; orquesta auth → async runtime → UI |
| `src/network.rs` | 158 | Servidor y cliente RPC con `tarpc` |
| `src/rpc.rs` | 38 | Definición del servicio RPC (trait + tipos) |
| `src/ui.rs` | 282 | Interfaz TUI con `ratatui` |
| `usuarios.json` | 8 | Credenciales válidas en JSON |
| `Cargo.toml` | 28 | Manifiesto del proyecto Rust |
| `Cargo.lock` | 1 296 | Árbol de dependencias fijado |

---

## Cambios funcionales detallados

### 1. `src/auth.rs` — Compuerta de autenticación

Módulo nuevo que implementa autenticación **síncrona y bloqueante**, ejecutada **antes** de crear el runtime de Tokio.

**Flujo:**
1. Lee `usuarios.json` y deserializa la lista de usuarios.
2. Solicita usuario y contraseña por `stdin`.
3. Verifica las credenciales contra el JSON.
4. Comprueba que no exista ya un archivo `sesiones/<usuario>.lock` (sesión duplicada).
5. Si todo es correcto, crea el lock y retorna `Some(nombre)`.
6. Expone `cerrar_sesion()` para eliminar el lock al salir.

**Decisión de diseño clave:** la autenticación ocurre *antes* de `tokio::runtime::Runtime::new()`, por lo que si las credenciales fallan, ningún socket ni tarea asíncrona llega a existir.

```
main()
  │
  ├─► parse_args()          ← argumentos de CLI
  ├─► auth::autenticar()    ← NUEVO: bloquea aquí si falla
  └─► tokio::runtime → iniciar_nodo()   ← sólo si auth pasó
```

---

### 2. `usuarios.json` — Registro de usuarios

Archivo de credenciales planas en JSON (sin hashing). Contiene cuatro cuentas de prueba:

```json
{ "usuarios": [
    { "usuario": "nodo",     "contrasena": "clave123" },
    { "usuario": "jugador1", "contrasena": "pass1"    },
    { "usuario": "jugador2", "contrasena": "pass2"    },
    { "usuario": "admin",    "contrasena": "admin456" }
]}
```

---

### 3. `src/main.rs` — Punto de entrada integrado con auth

- Llama a `auth::autenticar()` y termina con `process::exit(1)` si retorna `None`.
- Pasa el nombre del usuario autenticado (`String`) a `iniciar_nodo()` y luego a `Game::new()`.
- Llama a `auth::cerrar_sesion()` en cualquier camino de salida (normal o error).

---

### 4. `src/game.rs` — Estado del juego

Estructura `Game` ahora incluye el campo `usuario: String` (nombre del operador autenticado), que la UI usa para mostrarlo en el título.

Funciones clave: `apply_move`, `check_winner`, `reset`, `is_my_turn`, `status_message`.

---

### 5. `src/network.rs` + `src/rpc.rs` — Comunicación P2P con RPC

- **`rpc.rs`**: define el servicio `TicTacToe` mediante el atributo `#[tarpc::service]`, que genera automáticamente el trait del servidor y el stub del cliente (`TicTacToeClient`). Métodos: `make_move(casilla)` y `ping()`.
- **`network.rs`**: levanta el servidor tarpc en background, crea el cliente con reintentos (hasta 15), y expone `send_move()` para enviar jugadas al peer remoto.

---

### 6. `src/ui.rs` — Interfaz TUI

Interfaz construida con `ratatui`. Muestra:
- **Título**: nombre del usuario autenticado (`@<usuario>`) junto con los símbolos X/O.
- **Tablero 3×3**: casillas numeradas, coloreadas por jugador (cian = X, amarillo = O), resaltado verde en línea ganadora.
- **Estado**: turno actual, resultado (ganaste / perdiste / empate).
- **Panel lateral**: historial completo de movidas de la partida.
- **Controles**: `1-9` elegir casilla, `R` reiniciar, `Q` salir.

---

## Cómo ejecutar (desde la carpeta del proyecto)

```bash
# Jugador 1
cargo run -- --jugador 1 --escucha 8001 --rival 127.0.0.1:8002

# Jugador 2 (en otra terminal)
cargo run -- --jugador 2 --escucha 8002 --rival 127.0.0.1:8001
```

Ambos jugadores verán el prompt de autenticación antes de que el nodo levante su servidor RPC.
