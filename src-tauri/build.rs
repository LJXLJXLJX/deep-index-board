fn main() {
    #[cfg(target_os = "windows")]
    copy_onnxruntime_dlls();

    tauri_build::build()
}

#[cfg(target_os = "windows")]
fn copy_onnxruntime_dlls() {
    use std::{env, fs, path::PathBuf};

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let source_dir = manifest_dir.join("resources").join("native").join("onnxruntime");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_debug = out_dir
        .ancestors()
        .nth(3)
        .expect("unexpected Cargo OUT_DIR layout")
        .to_path_buf();

    for dll in ["onnxruntime.dll", "onnxruntime_providers_shared.dll"] {
        let source = source_dir.join(dll);
        if !source.exists() {
            println!("cargo:warning=Missing {}", source.display());
            continue;
        }

        for target_dir in [&target_debug, &target_debug.join("deps")] {
            fs::create_dir_all(target_dir).expect("failed to create target directory");
            fs::copy(&source, target_dir.join(dll)).expect("failed to copy ONNX Runtime DLL");
        }

        println!("cargo:rerun-if-changed={}", source.display());
    }
}
