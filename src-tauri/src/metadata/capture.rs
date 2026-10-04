use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use parking_lot::Mutex;

use super::{image, video, video_fallback, CaptureTime, MediaMetadata, MetadataError};
use crate::media::{media_kind, MediaKind};

const CACHE_LIMIT: usize = 8192;
const CAPTURE_TAGS: [u16; 6] = [0x9003, 0x9004, 0x9011, 0x9012, 0x9291, 0x9292];

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
}

fn stamp(metadata: Metadata) -> Stamp {
    Stamp {
        size: metadata.len(),
        modified: metadata.modified().ok(),
    }
}

#[derive(PartialEq, Eq)]
struct Identity {
    file: Stamp,
    sidecars: Vec<Option<Stamp>>,
}

struct Cached {
    identity: Identity,
    capture: Option<CaptureTime>,
}

type Entry = Arc<Mutex<Option<Cached>>>;
static CACHE: OnceLock<Mutex<HashMap<PathBuf, Entry>>> = OnceLock::new();

/// Routing needs only capture time, not pixel data, dimensions or codec discovery.
/// Cache by media and sidecar identity; serialize reads of the same physical file
/// without holding the shared cache lock during I/O.
pub fn extract_capture_time(path: &Path) -> Result<Option<CaptureTime>, String> {
    let kind = media_kind(path);
    if kind == MediaKind::Other {
        return Ok(None);
    }
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular media file", path.display()));
    }
    let identity = Identity {
        file: stamp(metadata),
        sidecars: if kind == MediaKind::Video {
            video_fallback::sidecar_candidates(path)
                .iter()
                .map(|p| std::fs::metadata(p).ok().map(stamp))
                .collect()
        } else {
            Vec::new()
        },
    };
    let entry = {
        let mut cache = CACHE.get_or_init(Mutex::default).lock();
        if cache.len() >= CACHE_LIMIT && !cache.contains_key(path) {
            if let Some(key) = cache.keys().next().cloned() {
                cache.remove(&key);
            }
        }
        cache.entry(path.to_path_buf()).or_default().clone()
    };
    let mut entry = entry.lock();
    if let Some(cached) = entry.as_ref().filter(|c| c.identity == identity) {
        return Ok(cached.capture.clone());
    }
    let capture = match read_capture(path, kind, identity.file.size) {
        Ok(capture) => capture,
        Err(MetadataError::Unsupported { .. }) => None,
        Err(error) => return Err(error.to_string()),
    };
    // A filesystem without reliable mtime cannot safely reuse cached routing.
    if identity.file.modified.is_some() {
        *entry = Some(Cached {
            identity,
            capture: capture.clone(),
        });
    }
    Ok(capture)
}

fn read_capture(
    path: &Path,
    kind: MediaKind,
    size: u64,
) -> Result<Option<CaptureTime>, MetadataError> {
    if kind == MediaKind::Video {
        let mut metadata = MediaMetadata {
            media_type: kind,
            size_bytes: size,
            capture_time: None,
            dimensions: None,
            camera: Default::default(),
            exposure: Default::default(),
            orientation: None,
            video: None,
        };
        // Camera sidecars and mvhd contain the capture date directly. Only videos
        // without a usable embedded date need the slower ffprobe fallback.
        video_fallback::fill(path, &mut metadata);
        if metadata.capture_time.is_none() {
            video::extract(path, &mut metadata)?;
        }
        return Ok(metadata.capture_time);
    }
    let file = File::open(path).map_err(|source| MetadataError::Io {
        path: path.into(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut header = [0u8; 8];
    reader
        .read_exact(&mut header)
        .map_err(|source| MetadataError::Io {
            path: path.into(),
            source,
        })?;
    let tiff = matches!(&header[..4], b"II\x2a\x00" | b"MM\x00\x2a");
    let exif = if tiff {
        let bytes =
            tiff_capture(&mut reader, header, size).map_err(|source| MetadataError::Io {
                path: path.into(),
                source,
            })?;
        exif::Reader::new().read_raw(bytes)
    } else {
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|source| MetadataError::Io {
                path: path.into(),
                source,
            })?;
        exif::Reader::new().read_from_container(&mut reader)
    };
    match exif {
        Ok(exif) => image::capture_from_exif(&exif),
        Err(exif::Error::NotFound(_)) => Ok(None),
        Err(exif::Error::InvalidFormat("Unknown image format")) => {
            Err(MetadataError::Unsupported { path: path.into() })
        }
        Err(error) => Err(error.into()),
    }
}

fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

/// Reads TIFF directory entries and the six capture-time strings by seeking,
/// then builds a tiny TIFF for the existing EXIF parser. Pixel strips and RAW
/// previews are never read, and input offsets never drive allocation sizes.
fn tiff_capture(
    reader: &mut (impl Read + Seek),
    header: [u8; 8],
    size: u64,
) -> std::io::Result<Vec<u8>> {
    let little = header[0] == b'I';
    let u16_at = |bytes: [u8; 2]| {
        if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        }
    };
    let u32_at = |bytes: [u8; 4]| {
        if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        }
    };
    let directory = |reader: &mut dyn ReadSeek, offset: u32| -> std::io::Result<Vec<[u8; 12]>> {
        if u64::from(offset) + 2 > size {
            return Err(invalid("TIFF directory outside file"));
        }
        reader.seek(SeekFrom::Start(u64::from(offset)))?;
        let mut count = [0; 2];
        reader.read_exact(&mut count)?;
        let count = u16_at(count) as usize;
        if count > 4096 || u64::from(offset) + 2 + count as u64 * 12 + 4 > size {
            return Err(invalid("Invalid TIFF directory length"));
        }
        let mut entries = vec![[0; 12]; count];
        for entry in &mut entries {
            reader.read_exact(entry)?;
        }
        Ok(entries)
    };
    let ifd = u32_at(header[4..8].try_into().expect("header offset"));
    let entries = directory(reader, ifd)?;
    let exif_ifd = entries
        .iter()
        .find(|entry| u16_at(entry[..2].try_into().unwrap()) == 0x8769)
        .map(|entry| {
            if u16_at(entry[2..4].try_into().unwrap()) != 4
                || u32_at(entry[4..8].try_into().unwrap()) != 1
            {
                return Err(invalid("Invalid EXIF directory pointer"));
            }
            Ok(u32_at(entry[8..12].try_into().unwrap()))
        })
        .transpose()?;
    let entries = match exif_ifd {
        Some(offset) => directory(reader, offset)?,
        None => Vec::new(),
    };
    let mut tags = Vec::new();
    for entry in entries {
        let tag = u16_at(entry[..2].try_into().unwrap());
        if !CAPTURE_TAGS.contains(&tag) {
            continue;
        }
        let kind = u16_at(entry[2..4].try_into().unwrap());
        let count = u32_at(entry[4..8].try_into().unwrap()) as usize;
        if kind != 2 || count == 0 || count > 128 {
            return Err(invalid("Invalid EXIF capture string"));
        }
        let mut bytes = vec![0; count];
        if count <= 4 {
            bytes.copy_from_slice(&entry[8..8 + count]);
        } else {
            let offset = u64::from(u32_at(entry[8..12].try_into().unwrap()));
            if offset + count as u64 > size {
                return Err(invalid("EXIF capture string outside file"));
            }
            reader.seek(SeekFrom::Start(offset))?;
            reader.read_exact(&mut bytes)?;
        }
        tags.push((tag, bytes));
    }
    // IFD0 contains only the pointer to the reconstructed EXIF directory.
    let mut bytes = vec![
        b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x69, 0x87, 4, 0, 1, 0, 0, 0, 26, 0, 0, 0, 0, 0, 0, 0,
    ];
    bytes.extend_from_slice(&(tags.len() as u16).to_le_bytes());
    let data_start = 26 + 2 + tags.len() * 12 + 4;
    let mut data = Vec::new();
    for (tag, value) in tags {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        if value.len() <= 4 {
            let mut inline = [0; 4];
            inline[..value.len()].copy_from_slice(&value);
            bytes.extend_from_slice(&inline);
        } else {
            bytes.extend_from_slice(&((data_start + data.len()) as u32).to_le_bytes());
            data.extend_from_slice(&value);
        }
    }
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend(data);
    Ok(bytes)
}

trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn raw_fixture(path: &Path, date: &[u8; 20]) {
        let mut bytes = vec![0; 83];
        bytes[..8].copy_from_slice(&[b'I', b'I', 42, 0, 8, 0, 0, 0]);
        bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
        for (offset, tag, kind, count, value) in [
            (10, 0x8769u16, 4u16, 1u32, 26u32),
            (28, 0x9003, 2, 20, 56),
            (40, 0x9011, 2, 7, 76),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&tag.to_le_bytes());
            bytes[offset + 2..offset + 4].copy_from_slice(&kind.to_le_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&count.to_le_bytes());
            bytes[offset + 8..offset + 12].copy_from_slice(&value.to_le_bytes());
        }
        bytes[26..28].copy_from_slice(&2u16.to_le_bytes());
        bytes[56..76].copy_from_slice(date);
        bytes[76..83].copy_from_slice(b"+00:00\0");
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn raw_capture_does_not_read_pixel_payload_and_invalidates_changed_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("A.ARW");
        raw_fixture(&path, b"2026:10:01 12:00:00\0");
        let file = File::options().write(true).open(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();
        struct CountingReader {
            file: File,
            bytes_read: usize,
        }
        impl Read for CountingReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let count = self.file.read(buf)?;
                self.bytes_read += count;
                Ok(count)
            }
        }
        impl Seek for CountingReader {
            fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
                self.file.seek(pos)
            }
        }
        let mut reader = CountingReader {
            file: File::open(&path).unwrap(),
            bytes_read: 0,
        };
        let mut header = [0; 8];
        reader.read_exact(&mut header).unwrap();
        tiff_capture(&mut reader, header, 512 * 1024 * 1024).unwrap();
        assert!(
            reader.bytes_read < 1024,
            "Must read only directories and date fields"
        );
        let capture = extract_capture_time(&path).unwrap().unwrap();
        assert_eq!(capture.local_datetime, "2026-10-01T12:00:00");
        assert_eq!(capture.utc_offset_seconds, Some(0));
        raw_fixture(&path, b"2026:10:02 12:00:00\0");
        assert_eq!(
            extract_capture_time(&path).unwrap().unwrap().local_datetime,
            "2026-10-02T12:00:00"
        );
    }

    #[test]
    fn video_sidecar_capture_is_fast_and_invalidates_when_sidecar_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("C0001.MP4");
        File::create(&path)
            .unwrap()
            .write_all(b"no codec probing needed")
            .unwrap();
        let sidecar = dir.path().join("C0001M01.XML");
        std::fs::write(&sidecar, r#"<NonRealTimeMeta><CreationDate value="2026-10-01T12:00:00+02:00"/></NonRealTimeMeta>"#).unwrap();
        let capture = extract_capture_time(&path).unwrap().unwrap();
        assert_eq!(capture.local_datetime, "2026-10-01T12:00:00");
        assert_eq!(capture.utc_offset_seconds, Some(7200));
        std::fs::write(&sidecar, r#"<NonRealTimeMeta><CreationDate value="2026-10-02T12:00:00.001+02:00"/></NonRealTimeMeta>"#).unwrap();
        assert_eq!(
            extract_capture_time(&path).unwrap().unwrap().local_datetime,
            "2026-10-02T12:00:00.001"
        );
    }

    #[test]
    fn big_endian_capture_preserves_offsets_subseconds_and_unknown_timezone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("A.NEF");
        let mut bytes = vec![0; 99];
        bytes[..8].copy_from_slice(&[b'M', b'M', 0, 42, 0, 0, 0, 8]);
        bytes[8..10].copy_from_slice(&1u16.to_be_bytes());
        bytes[26..28].copy_from_slice(&3u16.to_be_bytes());
        for (offset, tag, kind, count, value) in [
            (10, 0x8769u16, 4u16, 1u32, 26u32),
            (28, 0x9003, 2, 20, 72),
            (40, 0x9011, 2, 7, 92),
            (52, 0x9291, 2, 4, u32::from_be_bytes(*b"123\0")),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&tag.to_be_bytes());
            bytes[offset + 2..offset + 4].copy_from_slice(&kind.to_be_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&count.to_be_bytes());
            bytes[offset + 8..offset + 12].copy_from_slice(&value.to_be_bytes());
        }
        bytes[72..92].copy_from_slice(b"2026:10:01 12:00:00\0");
        bytes[92..99].copy_from_slice(b"-07:00\0");
        std::fs::write(&path, &bytes).unwrap();
        let capture = extract_capture_time(&path).unwrap().unwrap();
        assert_eq!(capture.local_datetime, "2026-10-01T12:00:00.123");
        assert_eq!(capture.utc_offset_seconds, Some(-25200));
        // Drop the offset and subseconds entries without inventing a timezone.
        bytes[26..28].copy_from_slice(&1u16.to_be_bytes());
        bytes.truncate(92);
        std::fs::write(&path, bytes).unwrap();
        let capture = extract_capture_time(&path).unwrap().unwrap();
        assert_eq!(capture.local_datetime, "2026-10-01T12:00:00");
        assert_eq!(capture.utc_offset_seconds, None);
        assert_eq!(crate::plan::capture_time_ms(&capture).unwrap(), None);
    }

    #[test]
    fn invalid_offsets_and_missing_files_are_errors_not_empty_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("A.ARW");
        raw_fixture(&path, b"2026:10:01 12:00:00\0");
        let mut file = File::options().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(18)).unwrap();
        file.write_all(&u32::MAX.to_le_bytes()).unwrap();
        assert!(extract_capture_time(&path)
            .unwrap_err()
            .contains("outside file"));
        std::fs::remove_file(&path).unwrap();
        assert!(extract_capture_time(&path).is_err());
    }
}
