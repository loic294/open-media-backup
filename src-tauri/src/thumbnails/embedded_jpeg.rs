use std::ops::Range;

pub fn extract_embedded_jpegs(bytes: &[u8]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    while let Some(rel) = find_soi(&bytes[pos..]) {
        let start = pos + rel;
        match jpeg_end(&bytes[start..]) {
            Some(end) => {
                ranges.push(start..start + end);
                pos = start + end;
            }
            None => pos = start + 3,
        }
    }
    ranges
}

fn find_soi(bytes: &[u8]) -> Option<usize> {
    bytes.windows(3).position(|w| w == [0xff, 0xd8, 0xff])
}

fn jpeg_end(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }

    let mut pos = 2;
    while pos < bytes.len() {
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

        match marker {
            0xd9 => return Some(pos),
            0xda => {
                if pos + 2 > bytes.len() {
                    return None;
                }
                let len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
                if len < 2 || pos + len > bytes.len() {
                    return None;
                }
                return scan_entropy_for_eoi(bytes, pos + len);
            }
            0x01 | 0xd0..=0xd7 => {}
            _ => {
                if pos + 2 > bytes.len() {
                    return None;
                }
                let len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
                if len < 2 || pos + len > bytes.len() {
                    return None;
                }
                pos += len;
            }
        }
    }
    None
}

fn scan_entropy_for_eoi(bytes: &[u8], mut pos: usize) -> Option<usize> {
    while pos + 1 < bytes.len() {
        if bytes[pos] == 0xff {
            let marker = bytes[pos + 1];
            match marker {
                0x00 => pos += 2,
                0xd0..=0xd7 => pos += 2,
                0xd9 => return Some(pos + 2),
                _ => pos += 1,
            }
        } else {
            pos += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_multiple_jpegs_with_noise_between() {
        let jpeg1 = [
            0xff, 0xd8, 0xff, 0xdb, 0x00, 0x04, 1, 2, 0xff, 0xda, 0x00, 0x02, 7, 8, 0xff, 0xd9,
        ];
        let jpeg2 = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 3, 4, 0xff, 0xda, 0x00, 0x02, 9, 0xff, 0x00, 10,
            0xff, 0xd9,
        ];
        let mut bytes = b"raw".to_vec();
        bytes.extend_from_slice(&jpeg1);
        bytes.extend_from_slice(b"gap");
        bytes.extend_from_slice(&jpeg2);

        let ranges = extract_embedded_jpegs(&bytes);
        assert_eq!(ranges, vec![3..19, 22..40]);
    }
}
