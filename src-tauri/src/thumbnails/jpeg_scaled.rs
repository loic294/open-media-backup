use image::{DynamicImage, GrayImage, RgbImage};
use jpeg_decoder::{Decoder, PixelFormat};

/// Decodes a JPEG at the smallest DCT scale (1/8 … 1) that still covers `min_edge` on its long side.
/// Much faster than a full decode for camera-sized images. `None` for unsupported pixel formats.
pub fn decode_scaled(bytes: &[u8], min_edge: u32) -> Option<DynamicImage> {
    let mut decoder = Decoder::new(bytes);
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    let long = w.max(h).max(1);
    let target = |side: u32| {
        ((u64::from(side) * u64::from(min_edge)).div_ceil(u64::from(long)))
            .clamp(1, u64::from(u16::MAX)) as u16
    };
    let (sw, sh) = decoder.scale(target(w), target(h)).ok()?;
    let pixels = decoder.decode().ok()?;
    let (sw, sh) = (u32::from(sw), u32::from(sh));
    match info.pixel_format {
        PixelFormat::RGB24 => RgbImage::from_raw(sw, sh, pixels).map(DynamicImage::ImageRgb8),
        PixelFormat::L8 => GrayImage::from_raw(sw, sh, pixels).map(DynamicImage::ImageLuma8),
        _ => None,
    }
}

/// Reads only the JPEG header. Returns (width, height).
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut decoder = Decoder::new(bytes);
    decoder.read_info().ok()?;
    decoder
        .info()
        .map(|i| (u32::from(i.width), u32::from(i.height)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 90)
            .encode_image(&img)
            .unwrap();
        out
    }

    #[test]
    fn downscales_by_dct_but_keeps_min_edge() {
        let bytes = jpeg(3200, 2400);
        assert_eq!(dimensions(&bytes), Some((3200, 2400)));
        let img = decode_scaled(&bytes, 360).unwrap();
        assert!(
            img.width() >= 360 && img.width() <= 800,
            "got {}",
            img.width()
        );
        assert_eq!(img.width() * 3, img.height() * 4);
    }

    #[test]
    fn small_images_decode_at_full_size() {
        let img = decode_scaled(&jpeg(200, 100), 360).unwrap();
        assert_eq!((img.width(), img.height()), (200, 100));
    }
}
