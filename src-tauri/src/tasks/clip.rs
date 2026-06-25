use crate::inference::manager::SessionManager;
#[cfg(target_os = "macos")]
use crate::inference::traits::{InferenceInput, InferenceOutput};
#[cfg(target_os = "windows")]
use ort::{session::Session, value::Tensor};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "windows")]
struct CachedOnnxSession {
    model_path: PathBuf,
    session: Session,
}

#[cfg(target_os = "windows")]
static CLIP_IMAGE_SESSION: OnceLock<Mutex<Option<CachedOnnxSession>>> = OnceLock::new();
#[cfg(target_os = "windows")]
static CLIP_TEXT_SESSION: OnceLock<Mutex<Option<CachedOnnxSession>>> = OnceLock::new();

pub fn run_clip_image_embedding(
    manager: &SessionManager,
    model_path: PathBuf,
    image_path: &Path,
) -> Result<Vec<f32>, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (manager, model_path, image_path);
        return Err("CLIP image embedding is disabled on this platform".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let _ = manager;
        crate::inference::onnx_runtime::ensure_ort_initialized()?;

        let img = image::open(image_path)
            .map_err(|e| format!("Failed to open image for CLIP embedding: {e}"))?;
        let pixels = crate::inference::utils::preprocess_image(&img, (224, 224));
        run_clip_image_onnx(model_path, pixels)
    }

    #[cfg(target_os = "macos")]
    {
        let session =
            manager.get_or_load_session("clip-image-vit-b-16", model_path, Some("CoreML"))?;
        let input = InferenceInput::Image(image_path.to_path_buf());

        let output = session.predict(input)?;
        let InferenceOutput::Tensors(mut tensors) = output;
        if let Some((data, _)) = tensors.pop() {
            Ok(data)
        } else {
            Err("No output tensor".into())
        }
    }
}

pub fn run_clip_text_embedding(
    manager: &SessionManager,
    model_path: PathBuf,
    vocab_path: &Path,
    text: &str,
) -> Result<Vec<f32>, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (manager, model_path, vocab_path, text);
        return Err("CLIP text embedding is disabled on this platform".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let _ = manager;
        crate::inference::onnx_runtime::ensure_ort_initialized()?;

        let tokenizer = crate::inference::utils::SimpleBertTokenizer::new(vocab_path)?;
        let input_ids = tokenizer.tokenize_to_i64_ids(text, 52);
        run_clip_text_onnx(model_path, input_ids)
    }

    #[cfg(target_os = "macos")]
    {
        // 1. 初始化 Tokenizer
        let tokenizer = crate::inference::utils::SimpleBertTokenizer::new(vocab_path)?;
        let input_ids = tokenizer.tokenize_to_ids(text, 52); // CLIP sequence length is 52

        // 2. 获取或加载 Session
        let session =
            manager.get_or_load_session("clip-text-vit-b-16", model_path, Some("CoreML"))?;

        // 3. 推理
        let input = InferenceInput::Tensor(input_ids, vec![1, 52]);
        let output = session.predict(input)?;
        let InferenceOutput::Tensors(mut tensors) = output;
        if let Some((data, _)) = tensors.pop() {
            Ok(data)
        } else {
            Err("No output tensor".into())
        }
    }
}

#[cfg(target_os = "windows")]
fn run_clip_image_onnx(model_path: PathBuf, pixels: Vec<f32>) -> Result<Vec<f32>, String> {
    let cache = CLIP_IMAGE_SESSION.get_or_init(|| Mutex::new(None));
    let mut guard = cache
        .lock()
        .map_err(|_| "CLIP image session lock poisoned".to_string())?;
    let session = get_or_load_onnx_session(&mut guard, model_path)?;

    let input_tensor = Tensor::from_array((vec![1, 3, 224, 224], pixels.into_boxed_slice()))
        .map_err(|e| format!("Failed to create CLIP image tensor: {e}"))?;
    let outputs = session
        .run(ort::inputs!["image" => input_tensor])
        .map_err(|e| format!("Failed to run CLIP image ONNX model: {e}"))?;

    extract_first_f32_output(outputs)
}

#[cfg(target_os = "windows")]
fn run_clip_text_onnx(model_path: PathBuf, input_ids: Vec<i64>) -> Result<Vec<f32>, String> {
    let cache = CLIP_TEXT_SESSION.get_or_init(|| Mutex::new(None));
    let mut guard = cache
        .lock()
        .map_err(|_| "CLIP text session lock poisoned".to_string())?;
    let session = get_or_load_onnx_session(&mut guard, model_path)?;

    let input_tensor = Tensor::from_array((vec![1, 52], input_ids.into_boxed_slice()))
        .map_err(|e| format!("Failed to create CLIP text tensor: {e}"))?;
    let outputs = session
        .run(ort::inputs!["text" => input_tensor])
        .map_err(|e| format!("Failed to run CLIP text ONNX model: {e}"))?;

    extract_first_f32_output(outputs)
}

#[cfg(target_os = "windows")]
fn get_or_load_onnx_session(
    cache: &mut Option<CachedOnnxSession>,
    model_path: PathBuf,
) -> Result<&mut Session, String> {
    let should_reload = cache
        .as_ref()
        .map(|cached| cached.model_path != model_path)
        .unwrap_or(true);

    if should_reload {
        let session = Session::builder()
            .map_err(|e| format!("Failed to create CLIP ONNX session builder: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| {
                format!(
                    "Failed to load CLIP ONNX model {}: {e}",
                    model_path.display()
                )
            })?;
        *cache = Some(CachedOnnxSession {
            model_path,
            session,
        });
    }

    cache
        .as_mut()
        .map(|cached| &mut cached.session)
        .ok_or_else(|| "CLIP ONNX session was not initialized".to_string())
}

#[cfg(target_os = "windows")]
fn extract_first_f32_output(outputs: ort::session::SessionOutputs<'_>) -> Result<Vec<f32>, String> {
    let output = outputs
        .values()
        .next()
        .ok_or_else(|| "CLIP ONNX model produced no outputs".to_string())?;
    let (_, data) = output
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Unexpected CLIP ONNX output tensor: {e}"))?;
    Ok(data.to_vec())
}

#[cfg(all(test, target_os = "macos"))]
pub fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    let dot_product: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
    let norm_v1: f32 = v1.iter().map(|a| a * a).sum::<f32>().sqrt();
    let norm_v2: f32 = v2.iter().map(|a| a * a).sum::<f32>().sqrt();
    if norm_v1 == 0.0 || norm_v2 == 0.0 {
        0.0
    } else {
        dot_product / (norm_v1 * norm_v2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use crate::inference::CoreMLBackend;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_clip_cross_matching() {
        let mut manager = SessionManager::new();
        manager.register_backend(Box::new(CoreMLBackend));

        let img_model_path = PathBuf::from("resources/models/clips/vit-b-16.image.mlpackage");
        let txt_model_path = PathBuf::from("resources/models/clips/vit-b-16.text.mlpackage");
        let vocab_path = PathBuf::from("resources/models/clips/vocab.txt");

        let test_images = vec![("test_assets/cat.jpg", "一张猫的照片")];

        let candidates = vec![
            "一张猫的照片",
            "一只狗的照片",
            "风景图",
            "代码截图",
            "Chinese-CLIP测试",
        ];

        if !img_model_path.exists() || !txt_model_path.exists() || !vocab_path.exists() {
            println!("Skipping: Models or Vocab not found");
            return;
        }

        println!("\n--- CLIP Matching Results (Softmax Mode) ---");

        // 1. Precalculate text embeddings
        let mut text_embeddings = Vec::new();
        for &text in &candidates {
            let vec = run_clip_text_embedding(&manager, txt_model_path.clone(), &vocab_path, text)
                .expect("Failed to get text embedding");
            text_embeddings.push(vec);
        }

        for (img_path_str, _) in test_images {
            let img_path = PathBuf::from(img_path_str);
            if !img_path.exists() {
                continue;
            }

            let img_vec = run_clip_image_embedding(&manager, img_model_path.clone(), &img_path)
                .expect("Failed to get image embedding");

            // 2. Calculate similarities
            let mut similarities = Vec::new();
            for txt_vec in &text_embeddings {
                let sim = cosine_similarity(&img_vec, txt_vec);
                similarities.push(sim);
            }

            // 3. Calculate Softmax (Logit Scale = 100.0)
            let logit_scale = 100.0;
            let mut logits: Vec<f32> = similarities.iter().map(|&s| s * logit_scale).collect();
            let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

            let mut exp_sum = 0.0;
            for l in &mut logits {
                *l = (*l - max_logit).exp();
                exp_sum += *l;
            }

            let probs: Vec<f32> = logits.iter().map(|&l| l / exp_sum).collect();

            println!("\nImage: {:?}", img_path);
            let mut results = Vec::new();
            for (i, &text) in candidates.iter().enumerate() {
                results.push((text, similarities[i], probs[i]));
            }

            // Sort by probability
            results.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
            for (text, sim, prob) in results {
                println!("  - \"{}\": Sim={:.4}, Prob={:.4}", text, sim, prob);
            }

            // --- Benchmark Loop (Same as Python) ---
            println!("\n  [Benchmark] Running 50 iterations to trigger ANE...");
            let session = manager
                .get_or_load_session(
                    "clip-image-vit-b-16",
                    img_model_path.clone(),
                    Some("CoreML"),
                )
                .expect("Session failed");
            let start = std::time::Instant::now();
            for _ in 0..50 {
                let _ = session
                    .predict(InferenceInput::NamedTensor(
                        "image".to_string(),
                        img_vec.clone(), // Clone to simulate fresh input
                        vec![1, 3, 224, 224],
                    ))
                    .expect("Predict failed");
            }
            let duration = start.elapsed();
            println!(
                "  [Benchmark] 50 iterations took {:.2?}. Average: {:.2?} / iter",
                duration,
                duration / 50
            );
        }
    }
}
