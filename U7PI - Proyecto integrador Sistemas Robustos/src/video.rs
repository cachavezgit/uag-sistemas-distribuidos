// ─────────────────────────────────────────────────────────
// video.rs — Captura de cámara para videollamada (U7PI)
//
// Solo se ocupa de CAPTURAR y codificar frames en JPEG; la reproducción
// del lado receptor reutiliza `player::PlayerHandle::open_stream()` tal
// cual (sin modificar player.rs) — cada frame recibido por RPC se escribe
// directo al stdin del reproductor, igual que el streaming de archivos
// de video del U6.
// ─────────────────────────────────────────────────────────

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use image::codecs::jpeg::JpegEncoder;
use image::ExtendedColorType;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use tokio::sync::mpsc;

const JPEG_QUALITY: u8 = 50;
const FRAME_INTERVAL: Duration = Duration::from_millis(66); // ~15 fps

/// Arranca la captura de la cámara por defecto en un hilo dedicado (nokhwa
/// es bloqueante) y manda cada frame codificado en JPEG por `frame_tx`.
/// Se detiene cuando `stop` pasa a `true`, el canal se cierra, o la
/// cámara falla. Retorna de inmediato — los errores de captura se logean
/// y cortan el hilo, no bloquean al llamador (una videollamada sin cámara
/// disponible no debe tumbar el resto de la app).
pub fn start_capture(frame_tx: mpsc::Sender<Vec<u8>>, stop: Arc<AtomicBool>, camera_index: u32) {
    std::thread::spawn(move || {
        // Requisito de nokhwa en macOS: pedir permiso de cámara antes de
        // abrirla. `requestAccessForMediaType` llama al callback de inmediato
        // si el usuario ya había aceptado/rechazado antes, y recién espera
        // de verdad si todavía no hay una decisión (diálogo real del SO) —
        // por eso se espera el callback con un timeout generoso en vez de
        // asumir un sleep fijo.
        let (perm_tx, perm_rx) = std::sync::mpsc::channel();
        nokhwa::nokhwa_initialize(move |granted| {
            let _ = perm_tx.send(granted);
        });
        match perm_rx.recv_timeout(Duration::from_secs(15)) {
            Ok(true) => {}
            Ok(false) => {
                eprintln!("[Video] Permiso de cámara denegado por el sistema operativo.");
                return;
            }
            Err(_) => {
                eprintln!("[Video] El sistema operativo no respondió al pedido de permiso de cámara a tiempo.");
                return;
            }
        }
        if !nokhwa::nokhwa_check() {
            eprintln!("[Video] La cámara no está autorizada para esta app (revisar Ajustes del Sistema → Privacidad y Seguridad → Cámara).");
            return;
        }

        let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = match Camera::new(CameraIndex::Index(camera_index), format) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Video] No se pudo abrir la cámara: {}", e);
                return;
            }
        };
        if let Err(e) = camera.open_stream() {
            eprintln!("[Video] No se pudo iniciar el stream de la cámara: {}", e);
            return;
        }

        while !stop.load(Ordering::Relaxed) {
            let frame = match camera.frame() {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[Video] Error capturando frame: {}", e);
                    break;
                }
            };
            let Ok(decoded) = frame.decode_image::<RgbFormat>() else {
                continue;
            };

            let mut jpeg_buf = Vec::new();
            let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_buf, JPEG_QUALITY);
            if encoder
                .encode(decoded.as_raw(), decoded.width(), decoded.height(), ExtendedColorType::Rgb8)
                .is_err()
            {
                continue;
            }

            if frame_tx.blocking_send(jpeg_buf).is_err() {
                break; // el consumidor (envío por RPC) se cerró
            }

            std::thread::sleep(FRAME_INTERVAL);
        }
    });
}
