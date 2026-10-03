//! Streaming content hashes. BLAKE3 is the default cryptographic hash;
//! xxHash64 remains available for fastest accidental-corruption detection only.
use crate::domain::HashAlgo;
use std::{fs::File, io::Read, path::Path};

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
    mut on_bytes: impl FnMut(u64) -> bool,
) -> std::io::Result<String> {
    let mut file = File::open(path)?;
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
    fn streaming_equals_one_shot() {
        let mut a = hasher(HashAlgo::Xxh64);
        a.update(b"hello ");
        a.update(b"world");
        let mut b = hasher(HashAlgo::Xxh64);
        b.update(b"hello world");
        assert_eq!(a.finish_hex(), b.finish_hex());
    }
}
