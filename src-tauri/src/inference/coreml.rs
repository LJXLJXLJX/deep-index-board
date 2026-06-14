use crate::inference::traits::{
    InferenceBackend, InferenceInput, InferenceOutput, InferenceSession,
};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{msg_send, msg_send_id, ClassType};
use objc2_core_ml::{
    MLComputeUnits, MLFeatureProvider, MLModel, MLModelConfiguration, MLMultiArray,
    MLMultiArrayDataType,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
pub struct CoreMLBackend;

#[cfg(target_os = "macos")]
impl InferenceBackend for CoreMLBackend {
    fn name(&self) -> &str {
        "CoreML"
    }

    fn load_session(
        &self,
        model_path: PathBuf,
        model_id: String,
    ) -> Result<Box<dyn InferenceSession>, String> {
        objc2::rc::autoreleasepool(|_| {
            let path_str = model_path.to_str().ok_or("Invalid path")?;
            let path_ns = NSString::from_str(path_str);
            let url = unsafe { NSURL::fileURLWithPath_isDirectory(&path_ns, true) };

            let config = unsafe { MLModelConfiguration::new() };
            unsafe { config.setComputeUnits(MLComputeUnits::All) };

            let mut model_url = url;
            if path_str.ends_with(".mlpackage") {
                let compiled_url = unsafe { MLModel::compileModelAtURL_error(&model_url) }
                    .map_err(|e| format!("Compile Error: {}", e))?;
                model_url = compiled_url;
            }

            let model =
                unsafe { MLModel::modelWithContentsOfURL_configuration_error(&model_url, &config) }
                    .map_err(|e| format!("Load Error: {}", e))?;

            Ok(Box::new(CoreMLSession {
                model_id,
                _model: model,
                model_path,
                simulated_memory: 150 * 1024 * 1024,
            }) as Box<dyn InferenceSession>)
        })
    }
}

pub struct CoreMLSession {
    model_id: String,
    #[allow(dead_code)]
    model_path: PathBuf,
    _model: Retained<MLModel>,
    simulated_memory: usize,
}

unsafe impl Send for CoreMLSession {}
unsafe impl Sync for CoreMLSession {}

impl InferenceSession for CoreMLSession {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn predict(&self, input: InferenceInput) -> Result<InferenceOutput, String> {
        objc2::rc::autoreleasepool(|_| {
            let (input_name, data, shape) = match input {
                InferenceInput::Image(path) => {
                    let img = image::open(path).map_err(|e| e.to_string())?;
                    let pixels = crate::inference::utils::preprocess_image(&img, (224, 224));
                    ("image", pixels, vec![1, 3, 224, 224])
                }
                InferenceInput::Tensor(data, shape) => ("text", data, shape),
                InferenceInput::NamedTensor(ref name, data, shape) => (name.as_str(), data, shape),
                _ => return Err("Unsupported input".into()),
            };

            unsafe {
                // 1. 创建 MLMultiArray
                let ns_shape: Vec<Retained<NSNumber>> = shape
                    .iter()
                    .map(|&s| NSNumber::new_isize(s as isize))
                    .collect();
                let ns_shape_array = NSArray::from_id_slice(&ns_shape);

                let multi_array = MLMultiArray::initWithShape_dataType_error(
                    MLMultiArray::alloc(),
                    &ns_shape_array,
                    MLMultiArrayDataType::Float32,
                )
                .map_err(|e| e.to_string())?;

                let ptr: *mut std::ffi::c_void = msg_send![&*multi_array, dataPointer];
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut f32, data.len());

                // 2. 构造输入
                let cls_feat_val = objc2::runtime::AnyClass::get("MLFeatureValue").unwrap();
                let feat_val: Retained<NSObject> =
                    msg_send_id![cls_feat_val, featureValueWithMultiArray: &*multi_array];
                let input_name_ns = NSString::from_str(input_name);

                let cls_dict = objc2::runtime::AnyClass::get("NSDictionary").unwrap();
                let dict: Retained<NSObject> = msg_send_id![cls_dict, dictionaryWithObject: &*feat_val, forKey: &*input_name_ns];

                let cls_dict_provider =
                    objc2::runtime::AnyClass::get("MLDictionaryFeatureProvider").unwrap();
                let provider_res: Result<Retained<NSObject>, Retained<NSObject>> = msg_send_id![msg_send_id![cls_dict_provider, alloc], initWithDictionary: &*dict, error: _];
                let provider =
                    provider_res.map_err(|e| format!("Provider Init Failed: {:?}", e))?;

                // 3. 推理
                let provider_proto: Retained<ProtocolObject<dyn MLFeatureProvider>> =
                    std::mem::transmute(provider);
                let output: Retained<ProtocolObject<dyn MLFeatureProvider>> = self
                    ._model
                    .predictionFromFeatures_error(&provider_proto)
                    .map_err(|e| e.to_string())?;

                // 4. 获取输出
                let output_names = ["image_features", "text_features", "output"];
                for name in output_names {
                    let name_ns = NSString::from_str(name);
                    let val: Option<Retained<NSObject>> =
                        msg_send_id![&*output, featureValueForName: &*name_ns];
                    if let Some(v) = val {
                        let is_undefined: bool = msg_send![&*v, isUndefined];
                        if !is_undefined {
                            let arr_val: Option<Retained<MLMultiArray>> =
                                msg_send_id![&*v, multiArrayValue];
                            if let Some(arr) = arr_val {
                                let count = arr.count() as usize;
                                let mut res_vec = vec![0.0f32; count];
                                let ptr_out: *mut std::ffi::c_void = msg_send![&*arr, dataPointer];
                                std::ptr::copy_nonoverlapping(
                                    ptr_out as *const f32,
                                    res_vec.as_mut_ptr(),
                                    count,
                                );

                                let out_shape_ns: Retained<NSArray<NSNumber>> =
                                    msg_send_id![&*arr, shape];
                                let mut out_shape = Vec::new();
                                for i in 0..out_shape_ns.count() {
                                    let num = out_shape_ns.objectAtIndex(i);
                                    let s: i64 = msg_send![&*num, longLongValue];
                                    out_shape.push(s as usize);
                                }
                                return Ok(InferenceOutput::Tensors(vec![(res_vec, out_shape)]));
                            }
                        }
                    }
                }
                Err("Output not found".into())
            }
        })
    }

    fn memory_usage_bytes(&self) -> usize {
        self.simulated_memory
    }
}
