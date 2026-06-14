use image::{imageops::FilterType, GenericImageView, RgbImage};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

const DET_DIR: &str = "PP-OCRv6_medium_det_onnx";
const REC_DIR: &str = "PP-OCRv6_medium_rec_onnx";
const DET_THRESH: f32 = 0.2;
const DET_BOX_THRESH: f32 = 0.45;
const DET_UNCLIP_RATIO: f32 = 2.0;
const DET_LIMIT_SIDE_LEN: u32 = 960;
const REC_HEIGHT: u32 = 48;
const REC_MIN_WIDTH: u32 = 160;
const REC_MAX_WIDTH: u32 = 3200;
const REC_WIDTH_MULTIPLE: u32 = 8;

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

pub fn run_onnx_ocr(image_path: &Path, model_root: Option<&Path>) -> Result<String, String> {
    crate::inference::onnx_runtime::ensure_ort_initialized()?;

    let model_root = resolve_model_root(model_root)?;
    let det_model = model_root.join(DET_DIR).join("inference.onnx");
    let rec_model = model_root.join(REC_DIR).join("inference.onnx");
    let rec_config = model_root.join(REC_DIR).join("inference.yml");

    let image = image::open(image_path)
        .map_err(|e| format!("Failed to open image for OCR: {e}"))?
        .to_rgb8();

    let dict = load_character_dict(&rec_config)?;
    let (det_input, det_w, det_h, _ratio_w, _ratio_h) = prepare_det_input(&image)?;
    let (det_data, det_shape) = run_model(
        &det_model,
        det_input,
        vec![1, 3, det_h as usize, det_w as usize],
    )?;
    let boxes = detect_text_boxes(&det_data, &det_shape, image.width(), image.height())?;

    let mut lines = Vec::new();
    for rect in boxes {
        if rect.w < 3 || rect.h < 3 {
            continue;
        }

        let crop = crop_rect(&image, rect);
        let (rec_input, rec_width) = prepare_rec_input(&crop)?;
        let (rec_data, rec_shape) = run_model(
            &rec_model,
            rec_input,
            vec![1, 3, REC_HEIGHT as usize, rec_width as usize],
        )?;
        let text = decode_ctc(&rec_data, &rec_shape, &dict)?;
        let text = text.trim();
        if !text.is_empty() {
            lines.push(text.to_string());
        }
    }

    Ok(lines.join("\n"))
}

fn resolve_model_root(model_root: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(root) = model_root {
        if root.join(DET_DIR).join("inference.onnx").exists()
            && root.join(REC_DIR).join("inference.onnx").exists()
        {
            return Ok(root.to_path_buf());
        }
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let candidates = [
        cwd.join("src-tauri")
            .join("resources")
            .join("models")
            .join("ocr"),
        cwd.join("resources").join("models").join("ocr"),
    ];

    candidates
        .into_iter()
        .find(|root| {
            root.join(DET_DIR).join("inference.onnx").exists()
                && root.join(REC_DIR).join("inference.onnx").exists()
        })
        .ok_or_else(|| "Windows OCR model root not found".to_string())
}

fn run_model(
    model_path: &Path,
    input: Vec<f32>,
    input_shape: Vec<usize>,
) -> Result<(Vec<f32>, Vec<usize>), String> {
    let mut session = Session::builder()
        .map_err(|e| format!("Failed to create ONNX Runtime session builder: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| format!("Failed to load ONNX model {}: {e}", model_path.display()))?;
    let input_tensor = Tensor::from_array((input_shape, input.into_boxed_slice()))
        .map_err(|e| format!("Failed to create ONNX input tensor: {e}"))?;
    let outputs = session
        .run(ort::inputs![input_tensor])
        .map_err(|e| format!("Failed to run ONNX model {}: {e}", model_path.display()))?;
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Unexpected ONNX output tensor: {e}"))?;

    Ok((data.to_vec(), shape.iter().map(|d| *d as usize).collect()))
}

fn prepare_det_input(image: &RgbImage) -> Result<(Vec<f32>, u32, u32, f32, f32), String> {
    let (orig_w, orig_h) = image.dimensions();
    if orig_w == 0 || orig_h == 0 {
        return Err("Empty image".to_string());
    }

    let scale = (DET_LIMIT_SIDE_LEN as f32 / orig_w.max(orig_h) as f32).min(1.0);
    let mut det_w = ((orig_w as f32 * scale).round() as u32).max(32);
    let mut det_h = ((orig_h as f32 * scale).round() as u32).max(32);
    det_w = round_to_multiple(det_w, 32);
    det_h = round_to_multiple(det_h, 32);

    let resized = image::imageops::resize(image, det_w, det_h, FilterType::Triangle);
    let ratio_w = det_w as f32 / orig_w as f32;
    let ratio_h = det_h as f32 / orig_h as f32;
    let mean = [0.485_f32, 0.456, 0.406];
    let std = [0.229_f32, 0.224, 0.225];
    let plane = det_h as usize * det_w as usize;
    let mut data = vec![0.0_f32; 3 * plane];

    for y in 0..det_h {
        for x in 0..det_w {
            let p = resized.get_pixel(x, y).0;
            let bgr = [p[2], p[1], p[0]];
            for c in 0..3 {
                data[c * plane + y as usize * det_w as usize + x as usize] =
                    ((bgr[c] as f32 / 255.0) - mean[c]) / std[c];
            }
        }
    }

    Ok((data, det_w, det_h, ratio_w, ratio_h))
}

fn prepare_rec_input(image: &RgbImage) -> Result<(Vec<f32>, u32), String> {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return Err("Empty text crop".to_string());
    }

    let ratio = w as f32 / h as f32;
    let content_w = ((REC_HEIGHT as f32 * ratio).ceil() as u32).max(1);
    let input_w = round_to_multiple(
        content_w.clamp(REC_MIN_WIDTH, REC_MAX_WIDTH),
        REC_WIDTH_MULTIPLE,
    )
    .min(REC_MAX_WIDTH);
    let resized_w = content_w.min(input_w);
    let resized = image::imageops::resize(image, resized_w, REC_HEIGHT, FilterType::Triangle);
    let plane = REC_HEIGHT as usize * input_w as usize;
    let mut data = vec![0.0_f32; 3 * plane];

    for y in 0..REC_HEIGHT {
        for x in 0..resized_w {
            let p = resized.get_pixel(x, y).0;
            let bgr = [p[2], p[1], p[0]];
            for c in 0..3 {
                data[c * plane + y as usize * input_w as usize + x as usize] =
                    (bgr[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
    }

    Ok((data, input_w))
}

fn detect_text_boxes(
    data: &[f32],
    shape: &[usize],
    orig_w: u32,
    orig_h: u32,
) -> Result<Vec<Rect>, String> {
    if shape.len() < 2 {
        return Err(format!("Unexpected detection output shape: {shape:?}"));
    }

    let map_h = shape[shape.len() - 2];
    let map_w = shape[shape.len() - 1];
    if map_h == 0 || map_w == 0 || data.len() < map_h * map_w {
        return Err(format!("Invalid detection output shape: {shape:?}"));
    }

    let mut visited = vec![false; map_w * map_h];
    let mut rects = Vec::new();

    for y in 0..map_h {
        for x in 0..map_w {
            let idx = y * map_w + x;
            if visited[idx] || data[idx] <= DET_THRESH {
                continue;
            }

            let mut queue = VecDeque::from([(x, y)]);
            visited[idx] = true;
            let (mut min_x, mut max_x) = (x, x);
            let (mut min_y, mut max_y) = (y, y);
            let mut sum = 0.0_f32;
            let mut count = 0_usize;
            let mut perimeter = 0_usize;

            while let Some((cx, cy)) = queue.pop_front() {
                let cidx = cy * map_w + cx;
                sum += data[cidx];
                count += 1;
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);

                for (nx, ny) in neighbor_slots(cx, cy, map_w, map_h) {
                    match (nx, ny) {
                        (Some(nx), Some(ny)) => {
                            let nidx = ny * map_w + nx;
                            if data[nidx] <= DET_THRESH {
                                perimeter += 1;
                            } else if !visited[nidx] {
                                visited[nidx] = true;
                                queue.push_back((nx, ny));
                            }
                        }
                        _ => perimeter += 1,
                    }
                }
            }

            if count < 4 {
                continue;
            }

            let score = box_score_rect(data, map_w, min_x, max_x, min_y, max_y);
            if score < DET_BOX_THRESH && sum / (count as f32) < DET_BOX_THRESH {
                continue;
            }

            if let Some(rect) = expand_text_region(
                min_x, max_x, min_y, max_y, perimeter, map_w, map_h, orig_w, orig_h,
            ) {
                rects.push(rect);
            }
        }
    }

    rects.sort_by_key(|r| (r.y / 16, r.x));
    Ok(rects)
}

fn neighbor_slots(x: usize, y: usize, w: usize, h: usize) -> [(Option<usize>, Option<usize>); 4] {
    [
        (x.checked_sub(1), Some(y)),
        (Some(x), y.checked_sub(1)),
        ((x + 1 < w).then_some(x + 1), Some(y)),
        (Some(x), (y + 1 < h).then_some(y + 1)),
    ]
}

fn box_score_rect(
    data: &[f32],
    map_w: usize,
    min_x: usize,
    max_x: usize,
    min_y: usize,
    max_y: usize,
) -> f32 {
    let mut sum = 0.0_f32;
    let mut count = 0_usize;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            sum += data[y * map_w + x];
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f32
    }
}

fn expand_text_region(
    min_x: usize,
    max_x: usize,
    min_y: usize,
    max_y: usize,
    component_perimeter: usize,
    map_w: usize,
    map_h: usize,
    orig_w: u32,
    orig_h: u32,
) -> Option<Rect> {
    let x0 = min_x as f32;
    let y0 = min_y as f32;
    let x1 = (max_x + 1) as f32;
    let y1 = (max_y + 1) as f32;
    let w = x1 - x0;
    let h = y1 - y0;
    if w < 2.0 || h < 2.0 {
        return None;
    }

    let rect_perimeter = 2.0 * (w + h);
    let perimeter = (component_perimeter as f32).max(rect_perimeter).max(1.0);
    let area = w * h;
    let distance = area * DET_UNCLIP_RATIO / perimeter;

    let left_map = (x0 - distance).floor().max(0.0);
    let top_map = (y0 - distance).floor().max(0.0);
    let right_map = (x1 + distance).ceil().min(map_w as f32);
    let bottom_map = (y1 + distance).ceil().min(map_h as f32);

    let left = (left_map * orig_w as f32 / map_w as f32).floor() as u32;
    let top = (top_map * orig_h as f32 / map_h as f32).floor() as u32;
    let right = (right_map * orig_w as f32 / map_w as f32)
        .ceil()
        .min(orig_w as f32) as u32;
    let bottom = (bottom_map * orig_h as f32 / map_h as f32)
        .ceil()
        .min(orig_h as f32) as u32;

    if right <= left || bottom <= top {
        return None;
    }

    Some(Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    })
}

fn crop_rect(image: &RgbImage, rect: Rect) -> RgbImage {
    image.view(rect.x, rect.y, rect.w, rect.h).to_image()
}

fn decode_ctc(data: &[f32], shape: &[usize], dict: &[String]) -> Result<String, String> {
    if shape.len() < 3 {
        return Err(format!("Unexpected recognition output shape: {shape:?}"));
    }

    let output_classes =
        if *shape.last().unwrap() == dict.len() + 1 || *shape.last().unwrap() == dict.len() + 2 {
            *shape.last().unwrap()
        } else if shape[1] == dict.len() + 1 || shape[1] == dict.len() + 2 {
            shape[1]
        } else {
            return Err(format!(
                "Recognition output classes do not match dictionary: shape={shape:?}, dict={}",
                dict.len()
            ));
        };

    let mut decode_dict = dict.to_vec();
    if output_classes == dict.len() + 2 {
        decode_dict.push(" ".to_string());
    }
    let class_count = decode_dict.len() + 1;

    let (timesteps, classes, index): (usize, usize, Box<dyn Fn(usize, usize) -> usize>) =
        if *shape.last().unwrap() == class_count {
            let t = shape[shape.len() - 2];
            (
                t,
                class_count,
                Box::new(move |step, class| step * class_count + class),
            )
        } else if shape[1] == class_count {
            let t = shape[2];
            (
                t,
                class_count,
                Box::new(move |step, class| class * t + step),
            )
        } else {
            return Err(format!(
                "Recognition output classes do not match dictionary: shape={shape:?}, dict={}",
                dict.len()
            ));
        };

    let mut text = String::new();
    let mut last_id = 0_usize;
    for t in 0..timesteps {
        let mut best_id = 0_usize;
        let mut best_score = f32::NEG_INFINITY;
        for c in 0..classes {
            let score = data[index(t, c)];
            if score > best_score {
                best_score = score;
                best_id = c;
            }
        }

        if best_id != 0 && best_id != last_id {
            if let Some(ch) = decode_dict.get(best_id - 1) {
                text.push_str(ch);
            }
        }
        last_id = best_id;
    }

    Ok(text)
}

fn load_character_dict(path: &Path) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read OCR character dictionary: {e}"))?;
    let mut in_dict = false;
    let mut chars = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed == "character_dict:" {
            in_dict = true;
            continue;
        }

        if !in_dict {
            continue;
        }

        if !trimmed.starts_with("- ") {
            if !trimmed.is_empty() && !line.starts_with(' ') {
                break;
            }
            continue;
        }

        let raw = trimmed.trim_start_matches("- ").trim();
        chars.push(unquote_yaml_scalar(raw));
    }

    if chars.is_empty() {
        return Err("OCR character dictionary is empty".to_string());
    }

    Ok(chars)
}

fn unquote_yaml_scalar(raw: &str) -> String {
    if raw == "''''" {
        return "'".to_string();
    }

    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return raw[1..raw.len() - 1].replace("''", "'");
    }

    raw.to_string()
}

fn round_to_multiple(value: u32, multiple: u32) -> u32 {
    ((value + multiple - 1) / multiple) * multiple
}
