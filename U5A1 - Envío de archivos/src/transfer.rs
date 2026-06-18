// ─────────────────────────────────────────────────────────
// transfer.rs — Fragmentación, cifrado y reconstrucción de archivos
//
// Flujo de cifrado por chunk:
//   bytes_raw → Base64 encode → String ASCII → Vigenère encrypt → String cifrada → Vec<u8>
//
// Flujo de descifrado por chunk:
//   Vec<u8> → String cifrada → Vigenère decrypt → String ASCII → Base64 decode → bytes_raw
//
// Se usa Base64 para que el cifrado Vigenère (rango ASCII 32–126)
// opere únicamente sobre caracteres imprimibles, aunque el archivo
// sea binario (imagen, etc.).
// ─────────────────────────────────────────────────────────

use std::fs;
use std::io::Write;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};

use crate::crypto;

const CHUNK_SIZE: usize = 65_536; // 64 KB por chunk

/// Paquete que representa un fragmento cifrado del archivo transferido
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunk {
    pub file_name: String, // nombre del archivo original (sin ruta)
    pub chunk_index: u32,  // índice base 0
    pub total_chunks: u32, // total de chunks del archivo
    pub data: Vec<u8>,     // bytes del chunk cifrado (como UTF-8)
}

/// Lee el archivo en chunks de 64 KB, codifica cada uno en Base64,
/// cifra con Vigenère y retorna el vector de FileChunk.
pub fn fragment_and_encrypt(path: &str, key: &str) -> anyhow::Result<Vec<FileChunk>> {
    let raw = fs::read(path)?;

    let file_name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archivo")
        .to_string();

    let chunks_raw: Vec<&[u8]> = raw.chunks(CHUNK_SIZE).collect();
    let total_chunks = chunks_raw.len() as u32;

    let mut result = Vec::with_capacity(chunks_raw.len());

    for (i, chunk) in chunks_raw.iter().enumerate() {
        let encoded = B64.encode(chunk);
        let cifrado = crypto::cifrar(&encoded, key);
        result.push(FileChunk {
            file_name: file_name.clone(),
            chunk_index: i as u32,
            total_chunks,
            data: cifrado.into_bytes(),
        });
    }

    Ok(result)
}

/// Descifra y reconstruye el archivo a partir de los chunks recibidos.
/// Los chunks deben estar ordenados por chunk_index.
/// Escribe el resultado en `output_dir/file_name` y retorna la ruta.
pub fn decrypt_and_reconstruct(chunks: &[FileChunk], key: &str, output_dir: &str) -> anyhow::Result<String> {
    if chunks.is_empty() {
        return Err(anyhow::anyhow!("No hay chunks para reconstruir"));
    }

    fs::create_dir_all(output_dir)?;

    let file_name = &chunks[0].file_name;
    let output_path = format!("{}/{}", output_dir, file_name);

    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&output_path)?;

    for chunk in chunks {
        let cifrado = String::from_utf8(chunk.data.clone())?;
        let encoded = crypto::descifrar(&cifrado, key);
        let raw = B64.decode(&encoded)?;
        file.write_all(&raw)?;
    }

    Ok(output_path)
}
