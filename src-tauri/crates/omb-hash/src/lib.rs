//! Streaming content hashes. BLAKE3 is the default cryptographic hash;
//! xxHash64 remains available for fastest accidental-corruption detection only.
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

pub mod protocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgo {
    Xxh64,
    #[default]
    Blake3,
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(av ^ bv);
    }
    diff == 0
}

pub const BUFFER_SIZE: usize = 4 * 1024 * 1024;

pub trait StreamHasher: Send {
    fn update(&mut self, bytes: &[u8]);
    fn finish_hex(self: Box<Self>) -> String;
}

struct Xxh64(xxhash_rust::xxh64::Xxh64);
impl StreamHasher for Xxh64 {
    fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    fn finish_hex(self: Box<Self>) -> String {
        format!("{:016x}", self.0.digest())
    }
}

struct Blake3(blake3::Hasher);
impl StreamHasher for Blake3 {
    fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    fn finish_hex(self: Box<Self>) -> String {
        self.0.finalize().to_hex().to_string()
    }
}

pub fn hasher(algo: HashAlgo) -> Box<dyn StreamHasher> {
    match algo {
        HashAlgo::Xxh64 => Box::new(Xxh64(xxhash_rust::xxh64::Xxh64::new(0))),
        HashAlgo::Blake3 => Box::new(Blake3(blake3::Hasher::new())),
    }
}

/// Hashes a file, calling `on_bytes` with each chunk size read. Returning false aborts.
pub fn hash_file(
    path: &Path,
    algo: HashAlgo,
    on_bytes: impl FnMut(u64) -> bool,
) -> std::io::Result<String> {
    hash_reader(File::open(path)?, algo, on_bytes)
}

pub fn hash_reader(
    mut file: impl Read,
    algo: HashAlgo,
    mut on_bytes: impl FnMut(u64) -> bool,
) -> std::io::Result<String> {
    let mut buf = vec![0u8; BUFFER_SIZE];
    let mut state = hasher(algo);
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(state.finish_hex());
        }
        state.update(&buf[..n]);
        if !on_bytes(n as u64) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "hashing cancelled",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digests() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            hash_file(&path, HashAlgo::Xxh64, |_| true).unwrap(),
            "44bc2cf5ad770999"
        );
        assert_eq!(
            hash_file(&path, HashAlgo::Blake3, |_| true).unwrap(),
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
    }

    #[test]
    fn cancellation_interrupts_stream_and_token_comparison_rejects_variants() {
        let input = vec![1; BUFFER_SIZE * 2];
        let mut read = 0;
        let error = hash_reader(input.as_slice(), HashAlgo::Blake3, |bytes| {
            read += bytes;
            false
        })
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(read, BUFFER_SIZE as u64);
        assert!(constant_time_eq("token", "token"));
        for candidate in ["", "toke", "tokem", "token-extra", "token\0"] {
            assert!(!constant_time_eq("token", candidate));
        }
    }
    #[test]
    fn streaming_equals_one_shot() {
        let mut a = hasher(HashAlgo::Xxh64);
        a.update(b"hello ");
        a.update(b"world");
        let mut b = hasher(HashAlgo::Xxh64);
        b.update(b"hello world");
        assert_eq!(a.finish_hex(), b.finish_hex());
    }
}
