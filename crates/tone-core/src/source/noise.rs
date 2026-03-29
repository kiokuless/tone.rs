use crate::graph::AudioNode;

/// Noise color type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseType {
    White,
    Pink,
    Brown,
}

/// A noise generator source.
///
/// Generates white, pink, or brown noise using simple algorithms.
pub struct Noise {
    noise_type: NoiseType,
    // State for PRNG (xorshift32)
    rng_state: u32,
    // Pink noise state (Paul Kellet's algorithm)
    pink: [f32; 7],
    // Brown noise state
    brown: f32,
}

impl Noise {
    pub fn new(noise_type: NoiseType) -> Self {
        Self {
            noise_type,
            rng_state: 0x12345678,
            pink: [0.0; 7],
            brown: 0.0,
        }
    }

    /// Generate a random f32 in [-1, 1] using xorshift32.
    #[inline]
    fn next_random(&mut self) -> f32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        // Convert to [-1, 1]
        (self.rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn generate_white(&mut self) -> f32 {
        self.next_random()
    }

    /// Paul Kellet's pink noise algorithm.
    fn generate_pink(&mut self) -> f32 {
        let white = self.next_random();
        self.pink[0] = 0.99886 * self.pink[0] + white * 0.0555179;
        self.pink[1] = 0.99332 * self.pink[1] + white * 0.0750759;
        self.pink[2] = 0.96900 * self.pink[2] + white * 0.153_852;
        self.pink[3] = 0.86650 * self.pink[3] + white * 0.3104856;
        self.pink[4] = 0.55000 * self.pink[4] + white * 0.5329522;
        self.pink[5] = -0.7616 * self.pink[5] - white * 0.0168980;
        let pink = self.pink[0]
            + self.pink[1]
            + self.pink[2]
            + self.pink[3]
            + self.pink[4]
            + self.pink[5]
            + self.pink[6]
            + white * 0.5362;
        self.pink[6] = white * 0.115926;
        pink * 0.11 // normalize
    }

    fn generate_brown(&mut self) -> f32 {
        let white = self.next_random();
        self.brown = (self.brown + 0.02 * white).clamp(-1.0, 1.0);
        self.brown
    }
}

impl AudioNode for Noise {
    fn process(&mut self, _input: &[f32], output: &mut [f32], _sample_rate: u32) {
        for sample in output.iter_mut() {
            *sample = match self.noise_type {
                NoiseType::White => self.generate_white(),
                NoiseType::Pink => self.generate_pink(),
                NoiseType::Brown => self.generate_brown(),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_produces_output() {
        for nt in [NoiseType::White, NoiseType::Pink, NoiseType::Brown] {
            let mut noise = Noise::new(nt);
            let mut output = [0.0f32; 256];
            noise.process(&[], &mut output, 44100);

            let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            assert!(max > 0.01, "{nt:?} noise should produce non-zero output");
        }
    }

    #[test]
    fn test_noise_in_range() {
        let mut noise = Noise::new(NoiseType::White);
        let mut output = [0.0f32; 4096];
        noise.process(&[], &mut output, 44100);

        for &s in &output {
            assert!(s >= -1.0 && s <= 1.0, "sample out of range: {s}");
        }
    }
}
