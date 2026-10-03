use std::fs;

use super::*;

type Entry = (u16, u16, u32, Vec<u8>);

fn ascii(tag: u16, text: &str) -> Entry {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    (tag, 2, bytes.len() as u32, bytes)
}

fn long(tag: u16, value: u32) -> Entry {
    (tag, 4, 1, value.to_le_bytes().to_vec())
}

fn rational(tag: u16, num: i32, den: i32, signed: bool) -> Entry {
    (
        tag,
        if signed { 10 } else { 5 },
        1,
        [num.to_le_bytes(), den.to_le_bytes()].concat(),
    )
}

/// Entirely synthetic little-endian TIFF with primary and EXIF IFDs.
fn tiff(mut primary: Vec<Entry>, exif: Vec<Entry>) -> Vec<u8> {
    let exif_offset = 8 + 6 + (primary.len() + 1) * 12;
    primary.push(long(0x8769, exif_offset as u32));
    let payload_offset = exif_offset + 6 + exif.len() * 12;
    let mut bytes = vec![0; payload_offset];
    bytes[..8].copy_from_slice(&[b'I', b'I', 42, 0, 8, 0, 0, 0]);
    for (start, mut entries) in [(8, primary), (exif_offset, exif)] {
        entries.sort_by_key(|entry| entry.0);
        bytes[start..start + 2].copy_from_slice(&(entries.len() as u16).to_le_bytes());
        for (index, (tag, kind, count, value)) in entries.into_iter().enumerate() {
            let offset = start + 2 + index * 12;
            bytes[offset..offset + 2].copy_from_slice(&tag.to_le_bytes());
            bytes[offset + 2..offset + 4].copy_from_slice(&kind.to_le_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&count.to_le_bytes());
            if value.len() <= 4 {
                bytes[offset + 8..offset + 8 + value.len()].copy_from_slice(&value);
            } else {
                let payload = bytes.len() as u32;
                bytes[offset + 8..offset + 12].copy_from_slice(&payload.to_le_bytes());
                bytes.extend(value);
            }
        }
    }
    bytes
}

fn base(kind: MediaKind) -> MediaMetadata {
    MediaMetadata {
        media_type: kind,
        size_bytes: 123,
        capture_time: None,
        dimensions: None,
        camera: CameraMetadata::default(),
        exposure: ExposureMetadata::default(),
        orientation: None,
        video: None,
    }
}

fn image_metadata(primary: Vec<Entry>, exif: Vec<Entry>) -> Result<MediaMetadata, MetadataError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.DNG");
    fs::write(&path, tiff(primary, exif)).unwrap();
    extract_metadata(&path)
}

#[test]
fn extracts_rich_embedded_image_fields_and_offset() {
    let metadata = image_metadata(
        vec![
            ascii(0x010f, "Test Camera Co"),
            ascii(0x0110, "Synthetic"),
            long(0x0100, 6000),
            long(0x0101, 4000),
            long(0x0112, 6),
        ],
        vec![
            ascii(0x9003, "2026:07:12 14:30:45"),
            ascii(0x9291, "125"),
            ascii(0x9011, "+02:30"),
            ascii(0xa433, "Test Lens Co"),
            ascii(0xa434, "35mm"),
            rational(0x829a, 1, 250, false),
            rational(0x829d, 28, 10, false),
            long(0x8827, 400),
            rational(0x920a, 35, 1, false),
            long(0xa405, 50),
            rational(0x9204, -1, 3, true),
        ],
    )
    .unwrap();
    assert_eq!(metadata.media_type, MediaKind::Raw);
    assert!(metadata.size_bytes > 0);
    assert_eq!(
        metadata.capture_time,
        Some(CaptureTime {
            local_datetime: "2026-07-12T14:30:45.125".into(),
            utc_offset_seconds: Some(9000),
            source: CaptureTimeSource::ExifOriginal,
        })
    );
    assert_eq!(
        metadata.dimensions,
        Some(Dimensions {
            width: 6000,
            height: 4000
        })
    );
    assert_eq!(metadata.orientation, Some(6));
    assert_eq!(
        metadata.camera,
        CameraMetadata {
            make: Some("Test Camera Co".into()),
            model: Some("Synthetic".into()),
            lens_make: Some("Test Lens Co".into()),
            lens_model: Some("35mm".into()),
        }
    );
    assert_eq!(
        metadata.exposure,
        ExposureMetadata {
            shutter_seconds: Some(0.004),
            aperture_f_number: Some(2.8),
            iso: Some(400),
            focal_length_mm: Some(35.0),
            focal_length_35mm: Some(50),
            compensation_ev: Some(-1.0 / 3.0),
        }
    );
    assert_eq!(metadata.video, None);
}

#[test]
fn prefers_original_and_preserves_unknown_timezone() {
    let metadata = image_metadata(
        vec![],
        vec![
            ascii(0x9003, "2026:01:02 03:04:05"),
            ascii(0x9004, "2025:01:02 03:04:05"),
        ],
    )
    .unwrap();
    let capture = metadata.capture_time.unwrap();
    assert_eq!(capture.source, CaptureTimeSource::ExifOriginal);
    assert_eq!(capture.local_datetime, "2026-01-02T03:04:05");
    assert_eq!(capture.utc_offset_seconds, None);
    let digitized = image_metadata(vec![], vec![ascii(0x9004, "2025:01:02 03:04:05")]).unwrap();
    assert_eq!(
        digitized.capture_time.unwrap().source,
        CaptureTimeSource::ExifDigitized
    );
}

#[test]
fn modification_only_exif_does_not_assign_capture_date() {
    let metadata = image_metadata(vec![ascii(0x0132, "2026:01:02 03:04:05")], vec![]).unwrap();
    assert_eq!(metadata.capture_time, None);
    assert_eq!(metadata.camera, CameraMetadata::default());
    assert_eq!(metadata.exposure, ExposureMetadata::default());
}

#[test]
fn png_without_exif_never_uses_filesystem_mtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.png");
    ::image::RgbImage::new(3, 2).save(&path).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        )
        .unwrap();
    let metadata = extract_metadata(&path).unwrap();
    assert_eq!(metadata.capture_time, None);
    assert_eq!(
        metadata.dimensions,
        Some(Dimensions {
            width: 3,
            height: 2
        })
    );
    assert_eq!(metadata.size_bytes, fs::metadata(&path).unwrap().len());
    let json = serde_json::to_value(metadata).unwrap();
    assert!(json["capture_time"].is_null());
    assert!(json["camera"]["model"].is_null());
    assert!(json["video"].is_null());
}

use std::time::Duration;

#[test]
fn jpeg_header_dimensions_override_exif_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.jpg");
    ::image::RgbImage::new(3, 2).save(&path).unwrap();
    let jpeg = fs::read(&path).unwrap();
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend(tiff(
        vec![long(0x0100, 6000), long(0x0101, 4000)],
        vec![ascii(0x9003, "2026:01:02 03:04:05")],
    ));
    let mut output = vec![0xff, 0xd8, 0xff, 0xe1];
    output.extend(((app1.len() + 2) as u16).to_be_bytes());
    output.extend(app1);
    output.extend(&jpeg[2..]);
    fs::write(&path, output).unwrap();
    let metadata = extract_metadata(&path).unwrap();
    assert_eq!(
        metadata.dimensions,
        Some(Dimensions {
            width: 3,
            height: 2
        })
    );
    assert!(metadata.capture_time.is_some());
}

#[test]
fn malformed_metadata_is_an_error() {
    for fields in [
        vec![ascii(0x9003, "2026:02:30 01:00:00")],
        vec![
            ascii(0x9003, "2026:01:02 03:04:05"),
            ascii(0x9011, "invalid"),
        ],
        vec![ascii(0x9003, "2026:01:02 03:04:05"), ascii(0x9291, "abc")],
        vec![rational(0x829a, 1, 0, false)],
    ] {
        assert!(matches!(
            image_metadata(vec![], fields),
            Err(MetadataError::Invalid(_))
        ));
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.jpg");
    fs::write(&path, [0xff, 0xd8, 0xff, 0xe1, 0, 20, b'E']).unwrap();
    assert!(matches!(
        extract_metadata(&path),
        Err(MetadataError::Exif(_))
    ));
}

#[test]
fn unsupported_and_io_errors_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsupported.txt");
    fs::write(&path, "not media").unwrap();
    assert!(matches!(
        extract_metadata(&path),
        Err(MetadataError::Unsupported { .. })
    ));
    assert!(matches!(
        extract_metadata(dir.path()),
        Err(MetadataError::Unsupported { .. })
    ));
    assert!(matches!(
        extract_metadata(&dir.path().join("missing.jpg")),
        Err(MetadataError::Io { .. })
    ));
    let path = dir.path().join("unsupported.cr3");
    fs::write(&path, "unsupported proprietary container").unwrap();
    assert!(matches!(
        extract_metadata(&path),
        Err(MetadataError::Unsupported { .. })
    ));
}

fn video_json(json: &str) -> Result<MediaMetadata, MetadataError> {
    let mut metadata = base(MediaKind::Video);
    video::populate(serde_json::from_str(json)?, &mut metadata)?;
    Ok(metadata)
}

#[test]
fn extracts_rich_video_fields_and_audio() {
    let metadata = video_json(
        r#"{
        "format": {"format_name":"mov,mp4", "duration":"2.5", "bit_rate":"100000",
            "tags":{"creation_time":"2026-07-12T12:00:00Z",
                "com.apple.quicktime.creationdate":"2026-07-12T14:00:00+02:00",
                "com.apple.quicktime.make":"Test", "com.apple.quicktime.model":"Camera",
                "lens_model":"35mm", "iso":"800", "f_number":"2.8"}},
        "streams": [
            {"codec_type":"video", "codec_name":"mjpeg", "disposition":{"attached_pic":1}},
            {"codec_type":"video", "codec_name":"h264", "pix_fmt":"yuv420p",
                "width":1920, "height":1080, "avg_frame_rate":"30000/1001",
                "side_data_list":[{"rotation":-90}]},
            {"codec_type":"audio","codec_name":"aac","channels":2,"sample_rate":"48000"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(
        metadata.capture_time,
        Some(CaptureTime {
            local_datetime: "2026-07-12T14:00:00".into(),
            utc_offset_seconds: Some(7200),
            source: CaptureTimeSource::VideoOriginalDate,
        })
    );
    assert_eq!(
        metadata.dimensions,
        Some(Dimensions {
            width: 1920,
            height: 1080
        })
    );
    assert_eq!(metadata.camera.model.as_deref(), Some("Camera"));
    assert_eq!(metadata.camera.lens_model.as_deref(), Some("35mm"));
    assert_eq!(metadata.exposure.iso, Some(800));
    assert_eq!(metadata.exposure.aperture_f_number, Some(2.8));
    assert_eq!(
        metadata.video,
        Some(VideoMetadata {
            container: Some("mov,mp4".into()),
            codec: Some("h264".into()),
            pixel_format: Some("yuv420p".into()),
            duration_seconds: Some(2.5),
            frame_rate: Some(30000.0 / 1001.0),
            bit_rate_bps: Some(100000),
            rotation_degrees: Some(-90),
            audio: vec![AudioMetadata {
                codec: Some("aac".into()),
                channels: Some(2),
                sample_rate_hz: Some(48000)
            }],
        })
    );
}

#[test]
fn absent_video_metadata_is_explicit_and_unknown_rate_is_not_an_error() {
    let metadata =
        video_json(r#"{"streams":[{"codec_type":"video","avg_frame_rate":"0/0"}]}"#).unwrap();
    assert_eq!(metadata.capture_time, None);
    assert_eq!(metadata.dimensions, None);
    assert_eq!(metadata.camera, CameraMetadata::default());
    assert_eq!(metadata.video.unwrap().frame_rate, None);
    let metadata = video_json(
        r#"{"streams":[{"codec_type":"video","avg_frame_rate":"0/0","r_frame_rate":"24/1",
        "tags":{"CREATION_TIME":"2026-07-12T12:00:00.125Z"}}]}"#,
    )
    .unwrap();
    assert_eq!(metadata.video.unwrap().frame_rate, Some(24.0));
    assert_eq!(metadata.capture_time.unwrap().utc_offset_seconds, Some(0));
}

#[test]
fn invalid_probe_data_is_an_error_not_empty_metadata() {
    for json in [
        r#"{"streams":[]}"#,
        r#"{"streams":[{"codec_type":"video","avg_frame_rate":"24/0"}]}"#,
        r#"{"streams":[{"codec_type":"video","duration":"NaN"}]}"#,
        r#"{"streams":[{"codec_type":"video","tags":{"creation_time":"not a date"}}]}"#,
        r#"{"streams":[{"codec_type":"video","width":0,"height":1080}]}"#,
    ] {
        assert!(matches!(video_json(json), Err(MetadataError::Invalid(_))));
    }
    assert!(matches!(
        video_json("not JSON"),
        Err(MetadataError::ProbeJson(_))
    ));
}

#[test]
fn video_container_fallbacks_and_compact_quicktime_offset() {
    let metadata = video_json(
        r#"{
        "format":{"duration":"3.5","bit_rate":"1234",
            "tags":{"com.apple.quicktime.creationdate":"2026-07-12T14:00:00+0230"}},
        "streams":[{"codec_type":"video","duration":"N/A","bit_rate":"N/A"}]
    }"#,
    )
    .unwrap();
    assert_eq!(
        metadata.capture_time.unwrap().utc_offset_seconds,
        Some(9000)
    );
    let video = metadata.video.unwrap();
    assert_eq!(video.duration_seconds, Some(3.5));
    assert_eq!(video.bit_rate_bps, Some(1234));
}

#[test]
#[ignore = "requires ffmpeg and ffprobe; generates all media in a temporary directory"]
fn generated_video_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    for has_capture in [false, true] {
        let path = dir.path().join(format!("generated-{has_capture}.mp4"));
        let mut command = std::process::Command::new("ffmpeg");
        command.args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=16x16:rate=2:duration=1",
            "-c:v",
            "mpeg4",
        ]);
        if has_capture {
            command.args(["-metadata", "creation_time=2026-07-12T12:00:00Z"]);
        }
        let output = command
            .arg(&path)
            .output()
            .expect("ffmpeg required for this test");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata = extract_metadata(&path).unwrap();
        assert_eq!(metadata.capture_time.is_some(), has_capture);
        if let Some(capture) = metadata.capture_time {
            assert_eq!(capture.local_datetime, "2026-07-12T12:00:00");
            assert_eq!(capture.utc_offset_seconds, Some(0));
            assert_eq!(capture.source, CaptureTimeSource::VideoCreationTime);
        }
        assert_eq!(metadata.media_type, MediaKind::Video);
        assert_eq!(metadata.size_bytes, fs::metadata(&path).unwrap().len());
        assert_eq!(
            metadata.dimensions,
            Some(Dimensions {
                width: 16,
                height: 16
            })
        );
        let video = metadata.video.unwrap();
        assert_eq!(video.codec.as_deref(), Some("mpeg4"));
        assert_eq!(video.duration_seconds, Some(1.0));
        assert_eq!(video.frame_rate, Some(2.0));
        assert!(video.audio.is_empty());
    }
    let path = dir.path().join("malformed.mp4");
    fs::write(&path, "this is not a video").unwrap();
    assert!(matches!(
        extract_metadata(&path),
        Err(MetadataError::ProbeFailed(_))
    ));
}
