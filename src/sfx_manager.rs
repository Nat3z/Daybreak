use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{BufReader, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use protobuf::well_known_types::duration::Duration as ProtoDuration;
use rodio::{Decoder, Source};

struct AudioData {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    playing: bool,
    stream: Option<cpal::Stream>,
    stop_sender: Option<Sender<()>>,
    is_once: bool,
    name: String,
}

// Message enum for communication with the audio thread
enum AudioMessage {
    Play { name: String, once: bool },
    Stop { name: String },
    StopAll,
    Shutdown,
}

pub struct SfxManager {
    audio_tx: Sender<AudioMessage>,
    thread_handle: Option<JoinHandle<()>>,
}

impl SfxManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (audio_tx, audio_rx) = mpsc::channel();

        // Create the audio thread
        let thread_handle = Some(thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_output_device() {
                Some(dev) => dev,
                None => {
                    eprintln!("No output device available");
                    return;
                }
            };

            let config = match device.default_output_config() {
                Ok(cfg) => cfg.config(),
                Err(e) => {
                    eprintln!("Error getting default output config: {}", e);
                    return;
                }
            };

            let config_dir = PathBuf::from(".").join(".daybreak");
            let sfx_dir = config_dir.join("sfx");

            // Create sfx directory if it doesn't exist
            if let Err(e) = fs::create_dir_all(&sfx_dir) {
                eprintln!("Error creating sfx directory: {}", e);
                return;
            }

            let mut audio_map = HashMap::new();
            let mut active_streams: Vec<cpal::Stream> = Vec::new();

            // Load all audio files
            if let Ok(entries) = fs::read_dir(&sfx_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext == "mp3" {
                            if let Some(name) = path.file_stem() {
                                if let Some(name_str) = name.to_str() {
                                    match fs::File::open(&path) {
                                        Ok(file) => {
                                            let reader = BufReader::new(file);
                                            match Decoder::new(reader) {
                                                Ok(decoder) => {
                                                    let sample_rate = decoder.sample_rate();
                                                    let channels = decoder.channels();

                                                    // Convert samples from i16 to f32 and normalize
                                                    let samples: Vec<f32> = decoder
                                                        .convert_samples::<i16>()
                                                        .map(|s| (s as f32) / 32768.0)
                                                        .collect();

                                                    audio_map.insert(
                                                        name_str.to_string(),
                                                        AudioData {
                                                            samples,
                                                            sample_rate,
                                                            channels,
                                                            playing: false,
                                                            stream: None,
                                                            stop_sender: None,
                                                            is_once: false,
                                                            name: name_str.to_string(),
                                                        },
                                                    );
                                                    println!(
                                                        "Loaded sound: {} ({}Hz, {} channels)",
                                                        name_str, sample_rate, channels
                                                    );
                                                }
                                                Err(e) => eprintln!(
                                                    "Failed to decode {}: {}",
                                                    name_str, e
                                                ),
                                            }
                                        }
                                        Err(e) => eprintln!("Failed to open {}: {}", name_str, e),
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let log_path = sfx_dir.parent().unwrap().join("sfx.log");

            // Audio thread main loop
            while let Ok(msg) = audio_rx.recv() {
                match msg {
                    AudioMessage::Play { name, once } => {
                        if let Some(audio_data) = audio_map.get_mut(&name) {
                            // Stop existing playback if needed
                            if audio_data.playing {
                                if let Some(tx) = audio_data.stop_sender.take() {
                                    let _ = tx.send(());
                                }
                                if let Some(stream) = audio_data.stream.take() {
                                    drop(stream);
                                }
                            }

                            let samples = audio_data.samples.clone();
                            let input_sample_rate = audio_data.sample_rate as f32;
                            let output_sample_rate = config.sample_rate.0 as f32;
                            let channels = audio_data.channels;
                            let mut sample_position = 0.0;
                            let sample_step = input_sample_rate / output_sample_rate;
                            let output_channels = config.channels as usize;
                            let total_samples = samples.len();

                            let (tx, rx) = mpsc::channel();
                            let should_stop = Arc::new(AtomicBool::new(false));
                            let should_stop_clone = Arc::clone(&should_stop);

                            // Monitor stop signal
                            thread::spawn(move || {
                                if rx.recv().is_ok() {
                                    should_stop_clone.store(true, Ordering::SeqCst);
                                }
                            });

                            match device.build_output_stream(
                                &config,
                                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                                    if should_stop.load(Ordering::SeqCst) {
                                        for sample in data.iter_mut() {
                                            *sample = 0.0;
                                        }
                                        return;
                                    }

                                    let input_channels = channels as usize;
                                    for frame in data.chunks_mut(output_channels) {
                                        let sample_index =
                                            sample_position as usize * input_channels;

                                        if sample_index >= total_samples {
                                            if once {
                                                for sample in frame.iter_mut() {
                                                    *sample = 0.0;
                                                }
                                                should_stop.store(true, Ordering::SeqCst);
                                                continue;
                                            } else {
                                                sample_position = 0.0;
                                                continue;
                                            }
                                        }

                                        match (input_channels, output_channels) {
                                            (1, 1) => {
                                                frame[0] = samples[sample_index];
                                            }
                                            (1, 2) => {
                                                frame[0] = samples[sample_index];
                                                frame[1] = samples[sample_index];
                                            }
                                            (2, 1) => {
                                                frame[0] = (samples[sample_index]
                                                    + samples[sample_index + 1])
                                                    * 0.5;
                                            }
                                            (2, 2) => {
                                                if sample_index + 1 < total_samples {
                                                    frame[0] = samples[sample_index];
                                                    frame[1] = samples[sample_index + 1];
                                                }
                                            }
                                            _ => {
                                                for sample in frame.iter_mut() {
                                                    *sample = 0.0;
                                                }
                                            }
                                        }
                                        sample_position += sample_step;
                                    }
                                },
                                |err| eprintln!("Audio playback error: {}", err),
                                None,
                            ) {
                                Ok(stream) => {
                                    if let Err(e) = stream.play() {
                                        eprintln!("Error playing stream for {}: {}", name, e);
                                    } else {
                                        audio_data.stream = Some(stream);
                                        audio_data.stop_sender = Some(tx);
                                        audio_data.playing = true;
                                        audio_data.is_once = once;

                                        if let Ok(mut file) = OpenOptions::new()
                                            .create(true)
                                            .append(true)
                                            .open(&log_path)
                                        {
                                            let _ = writeln!(
                                                file,
                                                "Started playing {} ({}, input: {}Hz, output: {}Hz, {} channels)",
                                                name,
                                                if once { "once" } else { "continuous" },
                                                input_sample_rate,
                                                output_sample_rate,
                                                channels
                                            );
                                        }
                                    }
                                }
                                Err(e) => eprintln!("Error building stream for {}: {}", name, e),
                            }
                        }
                    }
                    AudioMessage::Stop { name } => {
                        if let Some(audio_data) = audio_map.get_mut(&name) {
                            if let Some(tx) = audio_data.stop_sender.take() {
                                let _ = tx.send(());
                            }
                            if let Some(stream) = audio_data.stream.take() {
                                drop(stream);
                            }
                            audio_data.playing = false;

                            if let Ok(mut file) =
                                OpenOptions::new().create(true).append(true).open(&log_path)
                            {
                                let _ = writeln!(file, "Stopped sound {}", name);
                            }
                        }
                    }
                    AudioMessage::StopAll => {
                        for (name, audio_data) in audio_map.iter_mut() {
                            if let Some(tx) = audio_data.stop_sender.take() {
                                let _ = tx.send(());
                            }
                            if let Some(stream) = audio_data.stream.take() {
                                drop(stream);
                            }
                            audio_data.playing = false;

                            if let Ok(mut file) =
                                OpenOptions::new().create(true).append(true).open(&log_path)
                            {
                                let _ = writeln!(file, "Stopped sound {}", name);
                            }
                        }
                    }
                    AudioMessage::Shutdown => break,
                }
            }
        }));

        Ok(Self {
            audio_tx,
            thread_handle,
        })
    }

    pub fn play_sfx(&self, name: &str, once: bool) -> Result<(), Box<dyn std::error::Error>> {
        self.audio_tx.send(AudioMessage::Play {
            name: name.to_string(),
            once,
        })?;
        Ok(())
    }

    pub fn stop_sfx(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.audio_tx.send(AudioMessage::Stop {
            name: name.to_string(),
        })?;
        Ok(())
    }

    pub fn stop_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.audio_tx.send(AudioMessage::StopAll)?;
        Ok(())
    }
}

impl Drop for SfxManager {
    fn drop(&mut self) {
        let _ = self.audio_tx.send(AudioMessage::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
