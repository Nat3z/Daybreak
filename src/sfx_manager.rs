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
    thread,
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use protobuf::well_known_types::duration::Duration as ProtoDuration;
use rodio::{Decoder, Source};

struct AudioData {
    data: Vec<f32>,
    playing: bool,
    stream: Option<cpal::Stream>,
    stop_sender: Option<Sender<()>>,
    is_once: bool, // Track if this is a one-shot sound
    name: String,
}

pub struct SfxManager {
    audio_map: HashMap<String, AudioData>,
    device: cpal::Device,
    config: cpal::StreamConfig,
    sfx_dir: PathBuf,
    idle_playing: bool,
}

impl SfxManager {
    fn log_message(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let log_path = self.sfx_dir.parent().unwrap().join("sfx.log");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        writeln!(file, "{}", message)?;
        Ok(())
    }

    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;
        let config = device.default_output_config()?.config();

        let config_dir = PathBuf::from(".").join(".daybreak");
        let sfx_dir = config_dir.join("sfx");

        // Create sfx directory if it doesn't exist
        fs::create_dir_all(&sfx_dir)?;

        // Create or append to log file
        let manager = Self {
            audio_map: HashMap::new(),
            device,
            config,
            sfx_dir,
            idle_playing: false,
        };
        manager.log_message("SFX Manager initialized")?;

        Ok(manager)
    }

    pub fn load_sfx(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Clear existing audio
        self.audio_map.clear();

        let mut loaded_count = 0;
        self.log_message(&format!("Loading SFX from {:?}", self.sfx_dir))?;

        // Load all audio files from the sfx directory
        if let Ok(entries) = fs::read_dir(&self.sfx_dir) {
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
                                                let samples: Vec<f32> =
                                                    decoder.convert_samples().collect();

                                                self.audio_map.insert(
                                                    name_str.to_string(),
                                                    AudioData {
                                                        data: samples,
                                                        playing: false,
                                                        stream: None,
                                                        stop_sender: None,
                                                        is_once: false,
                                                        name: String::new(),
                                                    },
                                                );
                                                loaded_count += 1;
                                                self.log_message(&format!(
                                                    "Successfully loaded {}",
                                                    name_str
                                                ))?;
                                            }
                                            Err(e) => {
                                                self.log_message(&format!(
                                                    "Failed to decode {}: {}",
                                                    name_str, e
                                                ))?;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        self.log_message(&format!(
                                            "Failed to open {}: {}",
                                            name_str, e
                                        ))?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        self.log_message(&format!("Loaded {} sound files", loaded_count))?;
        Ok(())
    }

    fn check_and_play_idle(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Check if any sounds are playing
        let any_playing = self
            .audio_map
            .values()
            .any(|data| data.playing && data.name != "idle");

        match (any_playing, self.idle_playing) {
            (false, false) => {
                // No sounds playing and idle not playing, start idle
                self.play_sfx("idle", false)?;
                self.idle_playing = true;
            }
            (true, true) => {
                // Other sounds playing and idle is playing, stop idle
                self.stop_sfx("idle")?;
                self.idle_playing = false;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn play_sfx(&mut self, name: &str, once: bool) -> Result<(), Box<dyn std::error::Error>> {
        // Check if this is a stop command for a continuous sound
        if name.starts_with("stop_") {
            let original_name = name.trim_start_matches("stop_");
            self.stop_sfx(original_name)?;
            if original_name != "idle" {
                self.check_and_play_idle()?;
            }
            return Ok(());
        }

        // Handle idle sound state before playing new sound
        let should_stop_idle = name != "idle" && self.idle_playing;
        if should_stop_idle {
            self.stop_sfx("idle")?;
            self.idle_playing = false;
        }

        // First check if we need to stop existing playback
        let needs_stop = if let Some(audio_data) = self.audio_map.get(name) {
            if once && audio_data.playing {
                return Ok(());
            }
            audio_data.playing
        } else {
            false
        };

        if needs_stop {
            self.stop_sfx(name)?;
        }

        if let Some(audio_data) = self.audio_map.get_mut(name) {
            let samples = audio_data.data.clone();
            let mut sample_clock = 0;
            let channels = self.config.channels as usize;
            let total_samples = samples.len();

            // Create a channel for stopping the sound
            let (tx, rx) = mpsc::channel();
            audio_data.stop_sender = Some(tx);

            // Create an atomic flag for stopping
            let should_stop = Arc::new(AtomicBool::new(false));
            let should_stop_clone = Arc::clone(&should_stop);

            // Create a channel to signal when a one-shot sound is complete
            let (complete_tx, complete_rx) = mpsc::channel();
            let name_clone = name.to_string();
            let log_path = self.sfx_dir.parent().unwrap().join("sfx.log");

            // Monitor the stop signal
            thread::spawn(move || {
                if rx.recv().is_ok() {
                    should_stop_clone.store(true, Ordering::SeqCst);
                }
            });

            // For one-shot sounds, monitor completion
            if once {
                let should_stop = Arc::clone(&should_stop);
                let log_path = log_path.clone();
                let name_clone = name_clone.clone();
                thread::spawn(move || {
                    if complete_rx.recv().is_ok() {
                        should_stop.store(true, Ordering::SeqCst);
                        // Append completion message to log
                        if let Ok(mut file) =
                            OpenOptions::new().create(true).append(true).open(log_path)
                        {
                            let _ = writeln!(file, "One-shot sound {} completed", name_clone);
                        }
                    }
                });
            }

            let stream = self.device.build_output_stream(
                &self.config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Check if we should stop
                    if should_stop.load(Ordering::SeqCst) {
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                        return;
                    }

                    for frame in data.chunks_mut(channels) {
                        for sample in frame.iter_mut() {
                            if sample_clock >= total_samples {
                                if once {
                                    // Signal completion for one-shot sounds
                                    let _ = complete_tx.send(());
                                    *sample = 0.0;
                                    continue;
                                } else {
                                    // Loop continuous sounds
                                    sample_clock = 0;
                                }
                            }
                            *sample = samples[sample_clock];
                        }
                        sample_clock += 1;
                    }
                },
                |err| eprintln!("Audio playback error: {}", err),
                None,
            )?;

            stream.play()?;
            audio_data.stream = Some(stream);
            audio_data.playing = true;
            audio_data.is_once = once;
            audio_data.name = name.to_string();

            // Log start of playback
            self.log_message(&format!(
                "Started playing {} ({})",
                name,
                if once { "once" } else { "continuous" }
            ))?;
        }
        Ok(())
    }

    pub fn stop_sfx(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let log_path = self.sfx_dir.parent().unwrap().join("sfx.log");
        if let Some(audio_data) = self.audio_map.get_mut(name) {
            if let Some(tx) = audio_data.stop_sender.take() {
                let _ = tx.send(());
            }
            // Wait for the stream to actually stop
            if let Some(stream) = audio_data.stream.take() {
                drop(stream);
            }
            audio_data.playing = false;

            // Update idle state if needed
            if name == "idle" {
                self.idle_playing = false;
            } else {
                self.check_and_play_idle()?;
            }

            // Log the stop event
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(file, "Stopped sound {}", name);
            }
        }
        Ok(())
    }

    pub fn stop_all(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let log_path = self.sfx_dir.parent().unwrap().join("sfx.log");
        for (name, audio_data) in self.audio_map.iter_mut() {
            if let Some(tx) = audio_data.stop_sender.take() {
                let _ = tx.send(());
            }
            // Wait for the stream to actually stop
            if let Some(stream) = audio_data.stream.take() {
                drop(stream);
            }
            audio_data.playing = false;

            // Log the stop event
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(file, "Stopped sound {}", name);
            }
        }
        self.idle_playing = false;
        Ok(())
    }
}
