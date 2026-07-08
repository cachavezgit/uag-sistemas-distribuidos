// ─────────────────────────────────────────────────────────
// audio.rs — Captura y reproducción de audio para videollamada
// ─────────────────────────────────────────────────────────

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

pub struct AudioCapture {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub device_name: String,
}

unsafe impl Send for AudioCapture {}

pub struct AudioPlayback {
    _stream: cpal::Stream,
    pub device_name: String,
}

unsafe impl Send for AudioPlayback {}

/// Selecciona el dispositivo de entrada: busca primero uno cuyo nombre
/// contenga "VirtualMic" (para la Pi con audio_simulator.py), y si no
/// encuentra ninguno usa el dispositivo por defecto del sistema.
fn pick_input_device(host: &cpal::Host) -> anyhow::Result<cpal::Device> {
    if let Ok(mut devs) = host.input_devices() {
        if let Some(d) = devs.find(|d| {
            d.name()
                .map(|n| n.to_lowercase().contains("virtualmic"))
                .unwrap_or(false)
        }) {
            return Ok(d);
        }
    }
    host.default_input_device()
        .ok_or_else(|| anyhow::anyhow!("Sin dispositivo de entrada de audio"))
}

/// Inicia la captura de audio.
/// Retorna el handle (mantener vivo) + Receiver de chunks PCM mono i16
/// + nombre del dispositivo seleccionado para mostrarlo en la UI.
pub fn start_capture(
    stop: Arc<AtomicBool>,
) -> anyhow::Result<(AudioCapture, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
    let host = cpal::default_host();
    let device = pick_input_device(&host)?;
    let device_name = device.name().unwrap_or_else(|_| "desconocido".to_string());

    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<i16>>(64);

    let stream = match config.sample_format() {
        SampleFormat::F32 => {
            let tx2 = tx.clone();
            let stop2 = stop.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stop2.load(Ordering::Relaxed) {
                        return;
                    }
                    let mono: Vec<i16> = data
                        .chunks(channels)
                        .map(|frame| {
                            let avg = frame.iter().sum::<f32>() / channels as f32;
                            (avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                        })
                        .collect();
                    let _ = tx2.try_send(mono);
                },
                |e| eprintln!("[audio] captura: {e}"),
                None,
            )?
        }
        SampleFormat::I16 => {
            let stop2 = stop.clone();
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if stop2.load(Ordering::Relaxed) {
                        return;
                    }
                    let mono: Vec<i16> = data
                        .chunks(channels)
                        .map(|frame| {
                            let sum: i32 = frame.iter().map(|&s| s as i32).sum();
                            (sum / channels as i32) as i16
                        })
                        .collect();
                    let _ = tx.try_send(mono);
                },
                |e| eprintln!("[audio] captura: {e}"),
                None,
            )?
        }
        fmt => {
            return Err(anyhow::anyhow!(
                "Formato de captura no soportado: {:?}",
                fmt
            ))
        }
    };

    stream.play()?;
    Ok((AudioCapture { _stream: stream, sample_rate, device_name }, rx))
}

/// Inicia la reproducción en el dispositivo de salida por defecto.
/// Maneja tanto f32 como i16 según lo que soporte el dispositivo.
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

    let buf_read = buf.clone();

    // Intenta abrir con f32 (macOS/Windows), luego con i16 (ALSA/Linux)
    let stream = device
        .build_output_stream(
            &config,
            {
                let buf = buf_read.clone();
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut b = buf.lock().unwrap();
                    for frame in out.chunks_mut(out_channels) {
                        let s = b
                            .pop_front()
                            .map(|v| v as f32 / i16::MAX as f32)
                            .unwrap_or(0.0);
                        for ch in frame.iter_mut() {
                            *ch = s;
                        }
                    }
                }
            },
            |e| eprintln!("[audio] reproducción: {e}"),
            None,
        )
        .or_else(|_| {
            // Fallback: i16
            device.build_output_stream(
                &config,
                move |out: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let mut b = buf_read.lock().unwrap();
                    for frame in out.chunks_mut(out_channels) {
                        let s = b.pop_front().unwrap_or(0);
                        for ch in frame.iter_mut() {
                            *ch = s;
                        }
                    }
                },
                |e| eprintln!("[audio] reproducción: {e}"),
                None,
            )
        })?;

    stream.play()?;
    Ok((AudioPlayback { _stream: stream, device_name }, sync_tx))
}

/// Lista los dispositivos de entrada disponibles (para diagnóstico).
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| {
            devs.filter_map(|d| d.name().ok()).collect()
        })
        .unwrap_or_default()
}
