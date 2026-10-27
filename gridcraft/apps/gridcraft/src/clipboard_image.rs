//! Read an image only in response to Paste. Normalize pixels to PNG for portable XLSX files.
use std::io::Cursor;

const MAX_PIXELS: usize = 16_000_000;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

pub fn read(clipboard: &mut arboard::Clipboard) -> Result<Option<Vec<u8>>, String> {
    if let Ok(files) = clipboard.get().file_list() {
        for path in files {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "webp") {
                continue;
            }
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_BYTES {
                return Err("剪贴板图片文件超过 64 MB。".into());
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            return normalize(&bytes).map(Some);
        }
    }
    match clipboard.get_image() {
        Ok(image) => {
            if image.width == 0
                || image.height == 0
                || image.width > 8192
                || image.height > 8192
                || image.width.checked_mul(image.height).is_none_or(|p| p > MAX_PIXELS)
            {
                return Err("剪贴板图片过大，请缩小后再粘贴。".into());
            }
            let pixels =
                image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.bytes.into_owned()).ok_or("剪贴板图片数据不完整。")?;
            encode(pixels).map(Some)
        }
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(format!("无法读取剪贴板图片：{error}")),
    }
}

/// Publish selected floating pictures, including the crop the user sees.
pub fn copy(clipboard: &mut arboard::Clipboard, bytes: &[u8], crop: [f32; 4]) -> Result<(), String> {
    let pixels = if gridcraft_metafile::is_metafile(bytes) {
        let (rgba, w, h) = gridcraft_metafile::rasterize(bytes, 4096, 4096).ok_or("无法解码此矢量图片。")?;
        image::RgbaImage::from_raw(w, h, rgba).ok_or("图片像素数据不完整。")?
    } else {
        image::load_from_memory(&normalize(bytes)?).map_err(|e| e.to_string())?.into_rgba8()
    };
    let [l, t, w, h] = gridcraft_metafile::crop_window(crop);
    let x = (l * f64::from(pixels.width())).floor() as u32;
    let y = (t * f64::from(pixels.height())).floor() as u32;
    let w = ((w * f64::from(pixels.width())).round() as u32).max(1).min(pixels.width().saturating_sub(x));
    let h = ((h * f64::from(pixels.height())).round() as u32).max(1).min(pixels.height().saturating_sub(y));
    let pixels = image::imageops::crop_imm(&pixels, x, y, w, h).to_image();
    clipboard
        .set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Owned(pixels.into_raw()) })
        .map_err(|e| e.to_string())
}

fn normalize(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_BYTES);
    reader.limits(limits);
    let pixels = reader.decode().map_err(|e| format!("无法读取图片：{e}"))?.into_rgba8();
    if u64::from(pixels.width()) * u64::from(pixels.height()) > MAX_PIXELS as u64 {
        return Err("图片超过 1600 万像素。".into());
    }
    encode(pixels)
}

fn encode(pixels: image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut output = Cursor::new(Vec::new());
    pixels.write_to(&mut output, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_pixels_keep_alpha_and_size() {
        let pixels = image::RgbaImage::from_pixel(17, 9, image::Rgba([120, 40, 80, 128]));
        let bytes = encode(pixels.clone()).unwrap();
        assert_eq!(image::load_from_memory(&normalize(&bytes).unwrap()).unwrap().to_rgba8(), pixels);
        assert!(normalize(b"not an image").is_err());
    }
}
