// ─────────────────────────────────────────────────────────
// auth.rs — Compuerta de autenticación síncrona y bloqueante
//
// Lee las credenciales válidas desde usuarios.json y valida
// lo que el operador ingresa por stdin ANTES de crear el
// runtime de tokio. Si las credenciales no coinciden, el
// proceso termina con código 1 sin abrir ningún socket.
// ─────────────────────────────────────────────────────────

use std::io::{self, Write};

use serde::Deserialize;

const ARCHIVO_USUARIOS: &str = "usuarios.json";

#[derive(Deserialize)]
struct RegistroUsuarios {
    usuarios: Vec<Usuario>,
}

#[derive(Deserialize)]
struct Usuario {
    usuario: String,
    contrasena: String,
}

/// Lee usuario y contraseña por stdin y los valida contra
/// usuarios.json. Retorna Some(nombre) si las credenciales
/// coinciden, o None si la autenticación falla.
pub fn autenticar() -> Option<String> {
    println!("╔══════════════════════════════════════════╗");
    println!("║    Nodo P2P — Autenticación requerida    ║");
    println!("╚══════════════════════════════════════════╝");

    print!("  Usuario   : ");
    io::stdout().flush().unwrap();
    let mut usuario = String::new();
    io::stdin().read_line(&mut usuario).unwrap();

    print!("  Contraseña: ");
    io::stdout().flush().unwrap();
    let mut contrasena = String::new();
    io::stdin().read_line(&mut contrasena).unwrap();

    let usuario = usuario.trim();
    let contrasena = contrasena.trim();

    let contenido = match std::fs::read_to_string(ARCHIVO_USUARIOS) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("[Auth] No se encontró el archivo de usuarios: {}", ARCHIVO_USUARIOS);
            return None;
        }
    };

    let registro: RegistroUsuarios = match serde_json::from_str(&contenido) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[Auth] Error al parsear {}: {}", ARCHIVO_USUARIOS, e);
            return None;
        }
    };

    if registro.usuarios.iter().any(|u| u.usuario == usuario && u.contrasena == contrasena) {
        Some(usuario.to_string())
    } else {
        None
    }
}
