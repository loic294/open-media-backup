use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use exif::{Exif, In, Tag, Value};

use super::{parse_capture_time, CaptureTimeSource, Dimensions, MediaMetadata, MetadataError};

pub(super) fn extract(path: &Path, metadata: &mut MediaMetadata) -> Result<(), MetadataError> {
    let mut reader = BufReader::new(File::open(path).map_err(|source| MetadataError::Io {
        path: path.to_path_buf(),
        source,
    })?);
    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(exif) => Some(exif),
        Err(exif::Error::NotFound(_)) => None,
        Err(exif::Error::InvalidFormat("Unknown image format")) => {
            return Err(MetadataError::Unsupported {
                path: path.to_path_buf(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(exif) = exif.as_ref() {
        populate(exif, metadata)?;
    }
    // Header dimensions take precedence for formats supported by the decoder.
    // RAW/TIFF/HEIF use embedded full-image dimensions, never preview dimensions.
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp") {
        let (width, height) = ::image::image_dimensions(path)?;
        metadata.dimensions = Some(Dimensions { width, height });
    } else if exif.is_none() {
        return Err(MetadataError::Unsupported {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn text(exif: &Exif, tag: Tag) -> Result<Option<String>, MetadataError> {
    let Some(field) = exif.get_field(tag, In::PRIMARY) else {
        return Ok(None);
    };
    let Value::Ascii(values) = &field.value else {
        return Err(MetadataError::Invalid(format!("{tag} is not ASCII")));
    };
    let Some(value) = values.first() else {
        return Ok(None);
    };
    let value = std::str::from_utf8(value)
        .map_err(|_| MetadataError::Invalid(format!("{tag} is not UTF-8")))?
        .trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

fn integer(exif: &Exif, tag: Tag) -> Result<Option<u32>, MetadataError> {
    exif.get_field(tag, In::PRIMARY)
        .map(|field| {
            field
                .value
                .get_uint(0)
                .ok_or_else(|| MetadataError::Invalid(format!("{tag} is not unsigned integer")))
        })
        .transpose()
}

fn rational(exif: &Exif, tag: Tag) -> Result<Option<f64>, MetadataError> {
    let Some(field) = exif.get_field(tag, In::PRIMARY) else {
        return Ok(None);
    };
    let value = match &field.value {
        Value::Rational(values) => values.first().filter(|v| v.denom != 0).map(|v| v.to_f64()),
        Value::SRational(values) => values.first().filter(|v| v.denom != 0).map(|v| v.to_f64()),
        _ => None,
    };
    value
        .map(Some)
        .ok_or_else(|| MetadataError::Invalid(format!("{tag} is not a valid rational")))
}

pub(super) fn populate(exif: &Exif, metadata: &mut MediaMetadata) -> Result<(), MetadataError> {
    let original = text(exif, Tag::DateTimeOriginal)?;
    let digitized = text(exif, Tag::DateTimeDigitized)?;
    let selected = original
        .map(|v| {
            (
                v,
                Tag::SubSecTimeOriginal,
                Tag::OffsetTimeOriginal,
                CaptureTimeSource::ExifOriginal,
            )
        })
        .or_else(|| {
            digitized.map(|v| {
                (
                    v,
                    Tag::SubSecTimeDigitized,
                    Tag::OffsetTimeDigitized,
                    CaptureTimeSource::ExifDigitized,
                )
            })
        });
    if let Some((mut date, subsec_tag, offset_tag, source)) = selected {
        if let Some(subsec) = text(exif, subsec_tag)? {
            if !subsec.bytes().all(|b| b.is_ascii_digit()) || subsec.len() > 9 {
                return Err(MetadataError::Invalid(format!(
                    "{subsec_tag} is not 1-9 decimal digits"
                )));
            }
            date.push('.');
            date.push_str(&subsec);
        }
        let mut capture = parse_capture_time(&date, source)?;
        if let Some(offset) = text(exif, offset_tag)? {
            let date = format!("{}{offset}", capture.local_datetime);
            capture = parse_capture_time(&date, source)?;
            if capture.utc_offset_seconds.is_none() {
                return Err(MetadataError::Invalid(format!(
                    "{offset_tag} is not a timezone offset"
                )));
            }
        }
        metadata.capture_time = Some(capture);
    }
    let width = integer(exif, Tag::PixelXDimension)?.or(integer(exif, Tag::ImageWidth)?);
    let height = integer(exif, Tag::PixelYDimension)?.or(integer(exif, Tag::ImageLength)?);
    if let (Some(width), Some(height)) = (width, height) {
        if width == 0 || height == 0 {
            return Err(MetadataError::Invalid("zero image dimensions".into()));
        }
        metadata.dimensions = Some(Dimensions { width, height });
    }
    metadata.orientation = integer(exif, Tag::Orientation)?;
    if metadata
        .orientation
        .is_some_and(|value| !(1..=8).contains(&value))
    {
        return Err(MetadataError::Invalid(
            "EXIF orientation outside 1-8".into(),
        ));
    }
    metadata.camera.make = text(exif, Tag::Make)?;
    metadata.camera.model = text(exif, Tag::Model)?;
    metadata.camera.lens_make = text(exif, Tag::LensMake)?;
    metadata.camera.lens_model = text(exif, Tag::LensModel)?;
    metadata.exposure.shutter_seconds = rational(exif, Tag::ExposureTime)?;
    metadata.exposure.aperture_f_number = rational(exif, Tag::FNumber)?;
    metadata.exposure.iso = integer(exif, Tag::PhotographicSensitivity)?;
    metadata.exposure.focal_length_mm = rational(exif, Tag::FocalLength)?;
    metadata.exposure.focal_length_35mm = integer(exif, Tag::FocalLengthIn35mmFilm)?;
    metadata.exposure.compensation_ev = rational(exif, Tag::ExposureBiasValue)?;
    Ok(())
}
