// ─────────────────────────────────────────────────────────
// audio.rs — Captura y reproducción de audio para videollamada
//
// En Linux usa `pw-cat` para captura (PipeWire nativo, necesario cuando
// ALSA no expone el dispositivo virtual VirtualMic.monitor).
// En macOS/Windows usa cpal directamente.
// La reproducción usa cpal en todas las plataformas.
// ─────────────────────────────────────────────────────────

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

// ── Tipos públicos ────────────────────────────────────────

enum CaptureInner {
    #[allow(dead_code)]
    Cpal(cpal::Stream),
    #[cfg(target_os = "linux")]
    PwCat(std::process::Child),
}

pub struct AudioCapture {
    _inner: CaptureInner,
    pub sample_rate: u32,
    pub device_name: String,
}

// cpal::Stream y Child son Send en las plataformas que usamos
unsafe impl Send for AudioCapture {}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let CaptureInner::PwCat(ref mut child) = self._inner {
            let _ = child.kill();
        }
    }
}

pub struct AudioPlayback {
    _stream: cpal::Stream,
    pub device_name: String,
}

unsafe impl Send for AudioPlayback {}

// ── Captura ───────────────────────────────────────────────

/// En Linux: usa `pw-cat --record` para capturar desde el source por defecto
/// de PipeWire (que debe ser VirtualMic.monitor tras `pactl set-default-source`).
/// Retorna handle + Receiver de chunks PCM mono i16 a 48 000 Hz.
#[cfg(target_os = "linux")]
fn start_capture_pwcat(
    stop: Arc<AtomicBool>,
) -> anyhow::Result<(AudioCapture, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    const RATE: u32 = 48_000;
    const CHUNK_BYTES: usize = RATE as usize / 50 * 2; // 20 ms, mono, s16le

    let mut child = Command::new("pw-cat")
        .args([
            "--record",
            "--format=s16",
            &format!("--rate={}", RATE),
            "--channels=1",
            "-", // stdout
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("pw-cat no encontrado: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("pw-cat sin stdout"))?;

    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<i16>>(64);

    std::thread::spawn(move || {
        use std::io::Read;
        let mut reader = std::io::BufReader::new(stdout);
        let mut buf = vec![0u8; CHUNK_BYTES];
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            // read_exact garantiza chunks de exactamente 20 ms (960 muestras)
            // evitando cientos de RPC por segundo con chunks diminutos
            match reader.read_exact(&mut buf) {
                Ok(()) => {
                    let samples: Vec<i16> = buf
                        .chunks_exact(2)
                        .map(|b| i16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    let _ = tx.try_send(samples);
                }
                Err(_) => break,
            }
        }
    });

    Ok((
        AudioCapture {
            _inner: CaptureInner::PwCat(child),
            sample_rate: RATE,
            device_name: "VirtualMic (pw-cat)".to_string(),
        },
        rx,
    ))
}

/// Captura vía cpal (macOS / Windows, y Linux sin PipeWire).
fn start_capture_cpal(
    stop: Arc<AtomicBool>,
) -> anyhow::Result<(AudioCapture, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
    let host = cpal::default_host();

    // Busca VirtualMic por nombre, si no usa el device por defecto
    let device = host
        .input_devices()
        .ok()
        .and_then(|mut d| {
            d.find(|dev| {
                dev.name()
                    .map(|n| n.to_lowercase().contains("virtualmic"))
                    .unwrap_or(false)
            })
        })
        .or_else(|| host.default_input_device())
        .ok_or_else(|| anyhow::anyhow!("Sin dispositivo de entrada de audio"))?;

    let device_name = device.name().unwrap_or_else(|_| "desconocido".to_string());
    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<i16>>(64);

    // Número de muestras mono para 20 ms al sample_rate del dispositivo
    let chunk_samples = (sample_rate / 50) as usize;
    let buf_cpal: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(chunk_samples * 2)));

    let stream = match config.sample_format() {
        SampleFormat::F32 => {
            let tx2 = tx.clone();
            let stop2 = stop.clone();
            let buf2 = buf_cpal.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stop2.load(Ordering::Relaxed) { return; }
                    let mut b = buf2.lock().unwrap();
                    for frame in data.chunks(channels) {
                        let avg = frame.iter().sum::<f32>() / channels as f32;
                        b.push((avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                    }
                    if b.len() >= chunk_samples {
                        let chunk = b.drain(..chunk_samples).collect();
                        let _ = tx2.try_send(chunk);
                    }
                },
                |e| eprintln!("[audio] captura: {e}"),
                None,
            )?
        }
        SampleFormat::I16 => {
            let stop2 = stop.clone();
            let buf2 = buf_cpal.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if stop2.load(Ordering::Relaxed) { return; }
                    let mut b = buf2.lock().unwrap();
                    for frame in data.chunks(channels) {
                        let sum: i32 = frame.iter().map(|&s| s as i32).sum();
                        b.push((sum / channels as i32) as i16);
                    }
                    if b.len() >= chunk_samples {
                        let chunk = b.drain(..chunk_samples).collect();
                        let _ = tx.try_send(chunk);
                    }
                },
                |e| eprintln!("[audio] captura: {e}"),
                None,
            )?
        }
        fmt => return Err(anyhow::anyhow!("Formato de captura no soportado: {:?}", fmt)),
    };

    stream.play()?;
    Ok((
        AudioCapture {
            _inner: CaptureInner::Cpal(stream),
            sample_rate,
            device_name,
        },
        rx,
    ))
}

/// Punto de entrada público: en Linux prueba pw-cat primero, luego cpal.
pub fn start_capture(
    stop: Arc<AtomicBool>,
) -> anyhow::Result<(AudioCapture, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
    #[cfg(target_os = "linux")]
    {
        match start_capture_pwcat(stop.clone()) {
            Ok(r) => return Ok(r),
            Err(e) => eprintln!("[audio] pw-cat falló ({e}), intentando cpal..."),
        }
    }
    start_capture_cpal(stop)
}

// ── Reproducción ──────────────────────────────────────────

/// Inicia la reproducción en el dispositivo de salida por defecto.
/// Maneja f32 e i16; usa la config nativa para evitar errores de backend.
pub fn start_playback() -> anyhow::Result<(AudioPlayback, std::sync::mpsc::SyncSender<Vec<i16>>)> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("Sin dispositivo de salida de audio"))?;

    let device_name = device.name().unwrap_or_else(|_| "desconocido".to_string());
    let default_cfg = device.default_output_config()?;
    let out_channels = default_cfg.channels() as usize;
    let sample_rate = default_cfg.sample_rate().0;
    let config = default_cfg.config();

    let buf: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
    let buf_write = buf.clone();

    let (sync_tx, sync_rx) = std::sync::mpsc::sync_channel::<Vec<i16>>(64);

    std::thread::spawn(move || {
        while let Ok(samples) = sync_rx.recv() {
            let mut b = buf_write.lock().unwrap();
            b.extend(&samples);
            while b.len() > sample_rate as usize * 2 {
                b.pop_front();
            }
        }
    });

    let buf_f32 = buf.clone();
    let buf_i16 = buf.clone();

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut b = buf_f32.lock().unwrap();
                for frame in out.chunks_mut(out_channels) {
                    let s = b.pop_front().map(|v| v as f32 / i16::MAX as f32).unwrap_or(0.0);
                    for ch in frame.iter_mut() { *ch = s; }
                }
            },
            |e| eprintln!("[audio] reproducción: {e}"),
            None,
        )
        .or_else(|_| {
            device.build_output_stream(
                &config,
                move |out: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let mut b = buf_i16.lock().unwrap();
                    for frame in out.chunks_mut(out_channels) {
                        let s = b.pop_front().unwrap_or(0);
                        for ch in frame.iter_mut() { *ch = s; }
                    }
                },
                |e| eprintln!("[audio] reproducción: {e}"),
                None,
            )
        })?;

    stream.play()?;
    Ok((AudioPlayback { _stream: stream, device_name }, sync_tx))
}

/// Lista los dispositivos de entrada disponibles (para diagnóstico en UI).
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}
