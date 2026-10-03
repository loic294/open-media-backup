#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
}

pub fn jpeg_orientation(bytes: &[u8]) -> Orientation {
    parse_jpeg_orientation(bytes).unwrap_or(Orientation::Normal)
}

fn parse_jpeg_orientation(bytes: &[u8]) -> Option<Orientation> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }
    let mut pos = 2;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xff {
            return None;
        }
        while pos < bytes.len() && bytes[pos] == 0xff {
            pos += 1;
        }
        if pos >= bytes.len() {
            return None;
        }
        let marker = bytes[pos];
        pos += 1;
        if marker == 0xda || marker == 0xd9 {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if pos + 2 > bytes.len() {
            return None;
        }
        let len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        if len < 2 || pos + len > bytes.len() {
            return None;
        }
        let segment = &bytes[pos + 2..pos + len];
        if marker == 0xe1 && segment.starts_with(b"Exif\0\0") {
            return parse_exif_orientation(&segment[6..]);
        }
        pos += len;
    }
    None
}

fn parse_exif_orientation(tiff: &[u8]) -> Option<Orientation> {
    if tiff.len() < 8 {
        return None;
    }
    let little = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if read_u16(tiff, 2, little)? != 42 {
        return None;
    }
    let ifd = read_u32(tiff, 4, little)? as usize;
    let entries = read_u16(tiff, ifd, little)? as usize;
    let table_start = ifd.checked_add(2)?;
    for idx in 0..entries {
        let entry = table_start.checked_add(idx.checked_mul(12)?)?;
        if entry + 12 > tiff.len() {
            return None;
        }
        if read_u16(tiff, entry, little)? == 0x0112 {
            let value = read_u16(tiff, entry + 8, little)?;
            return Some(match value {
                3 => Orientation::Rotate180,
                6 => Orientation::Rotate90,
                8 => Orientation::Rotate270,
                _ => Orientation::Normal,
            });
        }
    }
    None
}

fn read_u16(bytes: &[u8], pos: usize, little: bool) -> Option<u16> {
    let data = [*bytes.get(pos)?, *bytes.get(pos + 1)?];
    Some(if little {
        u16::from_le_bytes(data)
    } else {
        u16::from_be_bytes(data)
    })
}

fn read_u32(bytes: &[u8], pos: usize, little: bool) -> Option<u32> {
    let data = [
        *bytes.get(pos)?,
        *bytes.get(pos + 1)?,
        *bytes.get(pos + 2)?,
        *bytes.get(pos + 3)?,
    ];
    Some(if little {
        u32::from_le_bytes(data)
    } else {
        u32::from_be_bytes(data)
    })
}
