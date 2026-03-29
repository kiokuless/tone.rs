use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use tone_core::backend::{AudioBackend, AudioCallback};

pub struct CpalBackend {
    device: Device,
    config: StreamConfig,
    stream: Option<Stream>,
}

impl CpalBackend {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("no output device available");
        let supported_config = device
            .default_output_config()
            .expect("no default output config");

        let config: StreamConfig = StreamConfig {
            channels: 1,
            sample_rate: supported_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        Self {
            device,
            config,
            stream: None,
        }
    }
}

impl Default for CpalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBackend for CpalBackend {
    fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }

    fn start(&mut self, mut callback: AudioCallback) {
        let stream = self
            .device
            .build_output_stream(
                &self.config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    callback(data);
                },
                |err| {
                    eprintln!("audio stream error: {err}");
                },
                None,
            )
            .expect("failed to build output stream");

        stream.play().expect("failed to start audio stream");
        self.stream = Some(stream);
    }

    fn stop(&mut self) {
        self.stream = None;
    }
}
