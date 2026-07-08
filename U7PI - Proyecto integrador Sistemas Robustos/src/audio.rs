// ─────────────────────────────────────────────────────────
// audio.rs — Captura y reproducción de audio para videollamada
//
// start_capture: abre el dispositivo de entrada por defecto (micrófono
//   o VirtualMic en la Pi) y empuja chunks PCM mono i16 a un canal tokio.
//
// start_playback: abre el dispositivo de salida por defecto y reproduce
//   los chunks PCM mono i16 que lleguen por un canal std::sync::mpsc.
//
// Ambas funciones retornan un "handle" que debe mantenerse vivo mientras
// dure la llamada — el Drop del handle cierra el stream de cpal.
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
}

// cpal::Stream contiene raw pointers pero es Send en las plataformas que usamos
unsafe impl Send for AudioCapture {}

pub struct AudioPlayback {
    _stream: cpal::Stream,
}

unsafe impl Send for AudioPlayback {}

/// Inicia la captura del dispositivo de entrada por defecto.
/// Retorna el handle (mantener vivo) y un Receiver de chunks PCM mono i16.
/// El canal es tokio para que el consumidor async pueda hacer `.recv().await`.
pub fn start_capture(
    stop: Arc<AtomicBool>,
) -> anyhow::Result<(AudioCapture, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("Sin dispositivo de entrada de audio"))?;

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
                "Formato de audio de entrada no soportado: {:?}",
                fmt
            ))
        }
    };

    stream.play()?;
    Ok((AudioCapture { _stream: stream, sample_rate }, rx))
}

/// Inicia la reproducción en el dispositivo de salida por defecto.
/// Retorna el handle y un SyncSender para empujar chunks PCM mono i16.
/// Usa la config nativa del dispositivo para evitar errores de backend
/// (Core Audio en Mac rechaza sample rates que no soporten directamente).
pub fn start_playback(
    _sample_rate: u32,
) -> anyhow::Result<(AudioPlayback, std::sync::mpsc::SyncSender<Vec<i16>>)> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("Sin dispositivo de salida de audio"))?;

    let default_cfg = device.default_output_config()?;
    let out_channels = default_cfg.channels() as usize;
    let sample_rate = default_cfg.sample_rate().0;

    // Usar la config nativa del dispositivo; si el emisor captura a un rate
    // distinto el tono variará levemente, aceptable para una demo P2P.
    let config = default_cfg.config();

    let buf: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
    let buf_write = buf.clone();

    let (sync_tx, sync_rx) = std::sync::mpsc::sync_channel::<Vec<i16>>(64);

    // Hilo que drena el canal síncrono hacia el VecDeque compartido con cpal
    std::thread::spawn(move || {
        while let Ok(samples) = sync_rx.recv() {
            let mut b = buf_write.lock().unwrap();
            b.extend(&samples);
            // No acumular más de 2 s de audio (descarta los más antiguos)
            while b.len() > sample_rate as usize * 2 {
                b.pop_front();
            }
        }
    });

    let buf_read = buf.clone();
    let stream = device.build_output_stream(
        &config,
        move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut b = buf_read.lock().unwrap();
            for frame in out.chunks_mut(out_channels) {
                let sample = b
                    .pop_front()
                    .map(|v| v as f32 / i16::MAX as f32)
                    .unwrap_or(0.0);
                for s in frame.iter_mut() {
                    *s = sample;
                }
            }
        },
        |e| eprintln!("[audio] reproducción: {e}"),
        None,
    )?;

    stream.play()?;
    Ok((AudioPlayback { _stream: stream }, sync_tx))
}
