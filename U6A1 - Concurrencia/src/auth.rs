// ─────────────────────────────────────────────────────────
// auth.rs — Compuerta de autenticación síncrona y bloqueante
//
// Lee las credenciales válidas desde usuarios.json y valida
// lo que el operador ingresa por stdin ANTES de crear el
// runtime de tokio. Si las credenciales no coinciden, el
// proceso termina con código 1 sin abrir ningún socket.
// ─────────────────────────────────────────────────────────

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;

const ARCHIVO_USUARIOS: &str = "usuarios.json";
const DIR_SESIONES: &str = "sesiones";

#[derive(Deserialize)]
struct RegistroUsuarios {
    usuarios: Vec<Usuario>,
}

#[derive(Deserialize)]
struct Usuario {
    usuario: String,
    contrasena: String,
}

fn ruta_lock(usuario: &str) -> PathBuf {
    Path::new(DIR_SESIONES).join(format!("{}.lock", usuario))
}

fn sesion_activa(usuario: &str) -> bool {
    ruta_lock(usuario).exists()
}

fn crear_sesion(usuario: &str) {
    let _ = std::fs::create_dir_all(DIR_SESIONES);
    let _ = std::fs::write(ruta_lock(usuario), "");
}

/// Elimina el archivo de lock del usuario al cerrar la sesión.
pub fn cerrar_sesion(usuario: &str) {
    let _ = std::fs::remove_file(ruta_lock(usuario));
}

/// Lee usuario y contraseña por stdin y los valida contra
/// usuarios.json. Retorna Some(nombre) si las credenciales
/// coinciden y no hay sesión activa, o None en caso contrario.
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

    let credenciales_ok = registro
        .usuarios
        .iter()
        .any(|u| u.usuario == usuario && u.contrasena == contrasena);

    if !credenciales_ok {
        return None;
    }

    if sesion_activa(usuario) {
        eprintln!("[Auth] El usuario '{}' ya tiene una sesión activa.", usuario);
        return None;
    }

    crear_sesion(usuario);
    Some(usuario.to_string())
}
