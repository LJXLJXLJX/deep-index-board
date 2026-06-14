#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::collections::HashMap;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::fs::File;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::io::{BufRead, BufReader};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub struct SimpleBertTokenizer {
    vocab: HashMap<String, i32>,
    unk_id: i32,
    cls_id: i32,
    sep_id: i32,
    pad_id: i32,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl SimpleBertTokenizer {
    pub fn new(vocab_path: &Path) -> Result<Self, String> {
        let file = File::open(vocab_path).map_err(|e| format!("Failed to open vocab: {}", e))?;
        let reader = BufReader::new(file);
        let mut vocab = HashMap::new();

        for (i, line) in reader.lines().enumerate() {
            let token = line
                .map_err(|e| format!("Error reading vocab line: {}", e))?
                .trim()
                .to_string();
            vocab.insert(token, i as i32);
        }

        Ok(Self {
            unk_id: *vocab.get("[UNK]").unwrap_or(&100),
            cls_id: *vocab.get("[CLS]").unwrap_or(&101),
            sep_id: *vocab.get("[SEP]").unwrap_or(&102),
            pad_id: *vocab.get("[PAD]").unwrap_or(&0),
            vocab,
        })
    }

    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub fn tokenize_to_ids(&self, text: &str, max_length: usize) -> Vec<f32> {
        self.tokenize_to_i64_ids(text, max_length)
            .into_iter()
            .map(|id| id as f32)
            .collect()
    }

    pub fn tokenize_to_i64_ids(&self, text: &str, max_length: usize) -> Vec<i64> {
        let mut ids = vec![self.cls_id as f32];

        // 简单字符分割实现（匹配 Python 的 tokenize 逻辑）
        for c in text.chars() {
            if c.is_whitespace() {
                continue;
            }
            let s = c.to_lowercase().to_string();
            let id = self.vocab.get(&s).cloned().unwrap_or(self.unk_id);
            ids.push(id as f32);
            if ids.len() >= max_length - 1 {
                break;
            }
        }

        ids.push(self.sep_id as f32);

        // Padding
        while ids.len() < max_length {
            ids.push(self.pad_id as f32);
        }

        ids.into_iter().map(|id| id as i64).collect()
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn preprocess_image(img: &image::DynamicImage, size: (u32, u32)) -> Vec<f32> {
    // 1. 处理 EXIF 旋转 (非常重要，否则图片可能是倒着的)
    // 默认 DynamicImage 不会自动应用旋转
    // 注意：这里我们使用 resize 之前的原图进行处理
    // 实际上 image::open 后的修正通常是通过具体的 exif crate 完成，
    // 这里我们先强制使用标准的 resize 流程，但更换滤镜。

    // 2. 使用 Lanczos3，它在下采样时比 CatmullRom 更接近 PIL 的 Bicubic 抗锯齿效果
    let resized = img.resize_exact(size.0, size.1, image::imageops::FilterType::Lanczos3);
    let rgb = resized.to_rgb8();

    // 2. Normalize constants (Standard CLIP)
    let mean = [0.48145466, 0.4578275, 0.40821073];
    let std = [0.26862954, 0.26130258, 0.27577711];

    let mut pixels = Vec::with_capacity((3 * size.0 * size.1) as usize);

    // 3. Mirror Python: (img - mean) / std -> transpose(2, 0, 1)
    // Python transpose(2, 0, 1) 把 Channel 放在最前面
    // 顺序必须是: Channel -> Height -> Width
    for c in 0..3 {
        for y in 0..size.1 {
            for x in 0..size.0 {
                let p = rgb.get_pixel(x, y); // [R, G, B]
                let val = p[c] as f32 / 255.0;
                pixels.push((val - mean[c]) / std[c]);
            }
        }
    }

    // 4. Debugging: Log first 10 processed values
    log::debug!(
        "Rust Preprocessing Sample (first 10, Lanczos3): {:?}",
        &pixels[..10.min(pixels.len())]
    );

    pixels
}
