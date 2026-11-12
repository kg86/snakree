//! Benchmark inputs: a deterministic generator, synthetic data and real files.

use std::io::Read;
use std::path::PathBuf;

use crate::cli::Options;

/// Deterministic xorshift64* generator, so that every run sees the same input.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }

    pub fn byte(&mut self) -> u8 {
        (self.next() >> 56) as u8
    }
}

/// Uniformly random bytes.
fn random_data(rng: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| rng.byte()).collect()
}

/// A random 4 KiB block repeated over the whole input with about 0.1% of the bytes mutated.
fn repetitive_data(rng: &mut Rng, n: usize) -> Vec<u8> {
    let block = random_data(rng, 4096);
    (0..n).map(|i| if rng.below(1000) == 0 { rng.byte() } else { block[i % block.len()] }).collect()
}

/// Reads at most `limit` bytes of the file.
/// Unused pages are not read, so they do not enter the RSS baseline.
fn read_prefix(path: &std::path::Path, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    std::fs::File::open(path)?.take(limit as u64).read_to_end(&mut data)?;
    Ok(data)
}

#[derive(Clone)]
pub enum Source {
    Random,
    Repetitive,
    File(PathBuf),
}

impl Source {
    pub fn name(&self) -> String {
        match self {
            Source::Random => "random".into(),
            Source::Repetitive => "repetitive".into(),
            Source::File(p) => p.file_name().map_or(p.display().to_string(), |s| s.to_string_lossy().into_owned()),
        }
    }

    /// Returns the input length without loading the input.
    pub fn len(&self, o: &Options) -> usize {
        let n = match self {
            Source::Random | Source::Repetitive => o.size,
            Source::File(p) => {
                std::fs::metadata(p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display())).len() as usize
            }
        };
        n.min(o.limit.unwrap_or(usize::MAX))
    }

    pub fn load(&self, o: &Options) -> Vec<u8> {
        let seed = o.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut data = match self {
            Source::Random => random_data(&mut Rng(seed | 1), o.size),
            Source::Repetitive => repetitive_data(&mut Rng(seed.rotate_left(17) | 1), o.size),
            Source::File(p) => read_prefix(p, o.limit.unwrap_or(usize::MAX))
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display())),
        };
        data.truncate(o.limit.unwrap_or(usize::MAX));
        data.shrink_to_fit();
        data
    }
}
