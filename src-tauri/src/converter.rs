use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageEncoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

#[derive(Clone, Serialize)]
pub struct ConvertProgress {
    pub id: String,
    pub stage: &'static str,
    pub percent: u8,
}

#[derive(Clone, Serialize)]
pub struct ConvertResult {
    pub id: String,
    pub output_path: String,
    pub original_bytes: u64,
    pub output_bytes: u64,
}

#[derive(Clone, Deserialize)]
pub struct ConvertRequest {
    pub id: String,
    pub source_path: String,
    pub target_format: String,
    /// 0-100, used for lossy encoders (JPEG/WebP). Ignored for PNG (always lossless + oxipng pass).
    pub quality: Option<u8>,
}

fn emit_progress(window: &Window, id: &str, stage: &'static str, percent: u8) {
    let _ = window.emit(
        "convert://progress",
        ConvertProgress {
            id: id.to_string(),
            stage,
            percent,
        },
    );
}

fn decode_source(path: &Path) -> Result<DynamicImage, String> {
    let is_heic = matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()),
        Some(ext) if ext == "heic" || ext == "heif"
    );

    if is_heic {
        decode_heic(path)
    } else {
        ImageReader::open(path)
            .map_err(|e| format!("failed to open {}: {e}", path.display()))?
            .with_guessed_format()
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?
            .decode()
            .map_err(|e| format!("failed to decode {}: {e}", path.display()))
    }
}

fn decode_heic(path: &Path) -> Result<DynamicImage, String> {
    use heic::{DecoderConfig, PixelLayout};

    let bytes = std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let output = DecoderConfig::new()
        .decode(&bytes, PixelLayout::Rgba8)
        .map_err(|e| format!("failed to decode HEIC {}: {e}", path.display()))?;

    let buffer = image::RgbaImage::from_raw(output.width, output.height, output.data)
        .ok_or_else(|| format!("decoded HEIC buffer for {} has unexpected size", path.display()))?;

    Ok(DynamicImage::ImageRgba8(buffer))
}

fn target_format_from_str(target: &str) -> Result<ImageFormat, String> {
    match target.to_lowercase().as_str() {
        "png" => Ok(ImageFormat::Png),
        "jpeg" | "jpg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::WebP),
        "gif" => Ok(ImageFormat::Gif),
        "bmp" => Ok(ImageFormat::Bmp),
        "tiff" | "tif" => Ok(ImageFormat::Tiff),
        other => Err(format!("unsupported target format: {other}")),
    }
}

fn encode(image: &DynamicImage, format: ImageFormat, quality: Option<u8>) -> Result<Vec<u8>, String> {
    let capacity = image.width() as usize * image.height() as usize * 3;
    let mut bytes = Vec::with_capacity(capacity);
    let mut cursor = Cursor::new(&mut bytes);

    match format {
        ImageFormat::Jpeg => {
            let q = quality.unwrap_or(85).clamp(1, 100);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, q);
            // Borrow the existing RGB8 buffer when possible; only convert when the source is
            // a different color type (e.g. RGBA, HEIC decoded as Rgba8).
            let rgb_owned;
            let raw = if let Some(rgb) = image.as_rgb8() {
                rgb.as_raw().as_slice()
            } else {
                rgb_owned = image.to_rgb8();
                rgb_owned.as_raw().as_slice()
            };
            encoder
                .write_image(raw, image.width(), image.height(), image::ExtendedColorType::Rgb8)
                .map_err(|e| format!("failed to encode JPEG: {e}"))?;
        }
        ImageFormat::Png => {
            image
                .write_to(&mut cursor, ImageFormat::Png)
                .map_err(|e| format!("failed to encode PNG: {e}"))?;
        }
        other => {
            image
                .write_to(&mut cursor, other)
                .map_err(|e| format!("failed to encode {other:?}: {e}"))?;
        }
    }

    Ok(bytes)
}

/// Lossless PNG recompression pass via oxipng — shrinks file size without
/// touching pixel data, applied after we already have PNG bytes from `encode`.
fn optimize_png(bytes: Vec<u8>) -> Vec<u8> {
    // Preset 2 is the sweet spot for interactive use: meaningfully smaller than unoptimized PNG,
    // but 5-20× faster than preset 4 for typically <2% difference in output size.
    let options = oxipng::Options::from_preset(2);
    oxipng::optimize_from_memory(&bytes, &options).unwrap_or(bytes)
}

fn output_path_for(source: &Path, format: ImageFormat) -> std::path::PathBuf {
    let ext = match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Png => "png",
        ImageFormat::WebP => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        _ => "out",
    };
    source.with_extension(format!("optimized.{ext}"))
}

#[tauri::command]
pub async fn convert_image(window: Window, request: ConvertRequest) -> Result<ConvertResult, String> {
    let source_path = std::path::PathBuf::from(&request.source_path);
    let id = request.id.clone();
    let format = target_format_from_str(&request.target_format)?;
    let quality = request.quality;

    emit_progress(&window, &id, "decoding", 10);

    // Single spawn_blocking for the entire CPU-bound + blocking-I/O pipeline.
    // Three separate spawn_blocking calls (original design) paid two unnecessary Tokio
    // context-switch roundtrips on work that is serial by nature. fs::metadata and
    // fs::write also belong on the blocking thread pool, not the async executor.
    let (original_bytes, output_path, output_bytes) = tauri::async_runtime::spawn_blocking({
        let window = window.clone();
        let id = id.clone();
        move || -> Result<(u64, std::path::PathBuf, u64), String> {
            let original_bytes = std::fs::metadata(&source_path)
                .map_err(|e| format!("failed to stat {}: {e}", source_path.display()))?
                .len();

            let decoded = decode_source(&source_path)?;

            emit_progress(&window, &id, "encoding", 55);
            let encoded = encode(&decoded, format, quality)?;

            let final_bytes = if matches!(format, ImageFormat::Png) {
                emit_progress(&window, &id, "optimizing", 80);
                optimize_png(encoded)
            } else {
                encoded
            };

            let output_path = output_path_for(&source_path, format);
            let output_bytes = final_bytes.len() as u64;
            std::fs::write(&output_path, &final_bytes)
                .map_err(|e| format!("failed to write {}: {e}", output_path.display()))?;

            Ok((original_bytes, output_path, output_bytes))
        }
    })
    .await
    .map_err(|e| format!("conversion task panicked: {e}"))??;

    emit_progress(&window, &id, "done", 100);

    Ok(ConvertResult {
        id,
        output_path: output_path.to_string_lossy().to_string(),
        original_bytes,
        output_bytes,
    })
}

#[tauri::command]
pub async fn save_image_to(source_path: String, dest_path: String) -> Result<(), String> {
    std::fs::copy(&source_path, &dest_path)
        .map(|_| ())
        .map_err(|e| format!("failed to save: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn sample_image() -> DynamicImage {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([x as u8 * 4, y as u8 * 4, 128, 255]);
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn encodes_png_and_optimizes_losslessly() {
        let img = sample_image();
        let png = encode(&img, ImageFormat::Png, None).expect("encode png");
        let optimized = optimize_png(png.clone());

        let redecoded = image::load_from_memory(&optimized).expect("redecode optimized png");
        assert_eq!(redecoded.width(), 64);
        assert_eq!(redecoded.height(), 64);
        assert_eq!(redecoded.to_rgba8(), img.to_rgba8());
    }

    #[test]
    fn encodes_jpeg_with_quality() {
        let img = sample_image();
        let jpeg = encode(&img, ImageFormat::Jpeg, Some(80)).expect("encode jpeg");

        let redecoded = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg).expect("redecode jpeg");
        assert_eq!(redecoded.width(), 64);
        assert_eq!(redecoded.height(), 64);
    }

    #[test]
    fn encodes_webp() {
        let img = sample_image();
        let webp = encode(&img, ImageFormat::WebP, None).expect("encode webp");
        assert!(!webp.is_empty());
    }

    #[test]
    fn maps_target_format_strings() {
        assert!(matches!(target_format_from_str("png"), Ok(ImageFormat::Png)));
        assert!(matches!(target_format_from_str("JPG"), Ok(ImageFormat::Jpeg)));
        assert!(matches!(target_format_from_str("webp"), Ok(ImageFormat::WebP)));
        assert!(target_format_from_str("bogus").is_err());
    }

    #[test]
    fn output_path_swaps_extension() {
        let path = Path::new("/tmp/photo.heic");
        let out = output_path_for(path, ImageFormat::Png);
        assert_eq!(out, Path::new("/tmp/photo.optimized.png"));
    }
}
