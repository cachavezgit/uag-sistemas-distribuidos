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
pub fn decrypt_and_reconstruct(
    chunks: &[FileChunk],
    key: &str,
    output_dir: &str,
) -> anyhow::Result<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn tmp(name: &str) -> String {
        env::temp_dir().join(name).to_str().unwrap().to_string()
    }

    #[test]
    fn round_trip_archivo_pequeno() {
        let path = tmp("transfer_test_small.bin");
        let content = b"Hola mundo! Este es un archivo de prueba con bytes variados: \x01\x02\xFF";
        fs::write(&path, content).unwrap();

        let key = "clave_prueba";
        let chunks = fragment_and_encrypt(&path, key).unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].total_chunks, 1);
        assert_eq!(chunks[0].file_name, "transfer_test_small.bin");
        assert_ne!(chunks[0].data, content.to_vec(), "los datos cifrados no deben ser iguales al original");

        let out_dir = tmp("transfer_test_recibidos_small");
        let out_path = decrypt_and_reconstruct(&chunks, key, &out_dir).unwrap();
        let recovered = fs::read(&out_path).unwrap();

        assert_eq!(recovered, content.to_vec());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&out_path);
    }

    #[test]
    fn round_trip_multi_chunk() {
        let path = tmp("transfer_test_multi.bin");
        // 2 chunks completos + un chunk parcial de 1000 bytes
        let content: Vec<u8> = (0u8..=255).cycle().take(CHUNK_SIZE * 2 + 1000).collect();
        fs::write(&path, &content).unwrap();

        let key = "otra_clave";
        let chunks = fragment_and_encrypt(&path, key).unwrap();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].total_chunks, 3);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i as u32);
            assert_eq!(chunk.total_chunks, 3);
        }

        let out_dir = tmp("transfer_test_recibidos_multi");
        let out_path = decrypt_and_reconstruct(&chunks, key, &out_dir).unwrap();
        let recovered = fs::read(&out_path).unwrap();

        assert_eq!(recovered, content);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&out_path);
    }

    #[test]
    fn clave_incorrecta_produce_contenido_diferente() {
        let path = tmp("transfer_test_wrong_key.bin");
        let content = b"contenido secreto de prueba";
        fs::write(&path, content).unwrap();

        let chunks = fragment_and_encrypt(&path, "clave_correcta").unwrap();

        let out_dir = tmp("transfer_test_recibidos_wrong");
        // Descifrar con clave incorrecta debe producir bytes distintos al original
        if let Ok(out_path) = decrypt_and_reconstruct(&chunks, "clave_incorrecta", &out_dir) {
            if let Ok(recovered) = fs::read(&out_path) {
                assert_ne!(recovered, content.to_vec());
                let _ = fs::remove_file(&out_path);
            }
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn round_trip_archivo_50mb() {
        let path = tmp("transfer_test_50mb.bin");
        // 50 MB exactos → debe generar 800 chunks de 64 KB
        let size = 50 * 1024 * 1024;
        let content: Vec<u8> = (0u8..=255).cycle().take(size).collect();
        fs::write(&path, &content).unwrap();

        let key = "clave_50mb";
        let chunks = fragment_and_encrypt(&path, key).unwrap();

        let expected_chunks = (size + CHUNK_SIZE - 1) / CHUNK_SIZE;
        assert_eq!(chunks.len(), expected_chunks, "número incorrecto de chunks para 50 MB");
        assert_eq!(chunks[0].total_chunks, expected_chunks as u32);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i as u32);
        }

        let out_dir = tmp("transfer_test_recibidos_50mb");
        let out_path = decrypt_and_reconstruct(&chunks, key, &out_dir).unwrap();
        let recovered = fs::read(&out_path).unwrap();

        assert_eq!(recovered.len(), size, "el archivo reconstruido tiene tamaño incorrecto");
        assert_eq!(recovered, content, "el contenido reconstruido no coincide con el original");

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&out_path);
    }

    #[test]
    fn data_cifrada_es_ascii_imprimible() {
        let path = tmp("transfer_test_ascii.bin");
        // Archivo binario con todos los valores de byte posibles
        let content: Vec<u8> = (0u8..=255).collect();
        fs::write(&path, &content).unwrap();

        let chunks = fragment_and_encrypt(&path, "clave_ascii").unwrap();

        for chunk in &chunks {
            for &byte in &chunk.data {
                assert!(
                    byte >= 32 && byte <= 126,
                    "byte {} fuera del rango ASCII imprimible (32–126)",
                    byte
                );
            }
        }

        let _ = fs::remove_file(&path);
    }
}

