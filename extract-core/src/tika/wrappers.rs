use crate::errors::{Error, ExtractResult};
use crate::tika::jni_utils::{
    jni_check_exception, jni_jobject_to_string, jni_new_string_as_jvalue,
};
use crate::tika::vm;
use crate::{OfficeParserConfig, PdfParserConfig, TesseractOcrConfig};
use bytemuck::cast_slice_mut;
use jni::objects::{JObject, JValue};
use jni::sys::jsize;
use jni::JNIEnv;
use std::io::Read;

/// Wrapper for [`JObject`]s that contain `org.apache.commons.io.input.ReaderInputStream`
/// Implements [`Read`] and [`Drop] traits.
/// On drop, it calls the java close() method to properly clean the input stream
pub struct JReaderInputStream<'a> {
    internal: JObject<'a>,
}

impl<'a> JReaderInputStream<'a> {
    pub(crate) fn new(obj: JObject<'a>) -> Self {
        Self { internal: obj }
    }
}

impl<'a> Read for JReaderInputStream<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut env = vm().attach_current_thread().map_err(Error::JniError)?;

        // Create the java byte array
        let length = buf.len() as jsize;
        let jbyte_array = env
            .new_byte_array(length)
            .map_err(|_e| Error::JniEnvCall("Failed to create byte array"))?;

        // Call the Java Reader's `read` method
        let call_result = env.call_method(
            &self.internal,
            "read",
            "([BII)I",
            &[
                JValue::Object(&jbyte_array),
                JValue::Int(0),
                JValue::Int(length),
            ],
        );
        jni_check_exception(&mut env)?; // prints any exceptions thrown to stderr
        let num_read_bytes = call_result
            .map_err(Error::JniError)?
            .i()
            .map_err(Error::JniError)?;

        // Get the bytes from the Java byte array to the Rust byte array
        // don't know if this is a copy or just memory reference
        let buf_of_i8: &mut [i8] = cast_slice_mut(buf); // cast because java byte array is i8[]
        env.get_byte_array_region(jbyte_array, 0, buf_of_i8)
            .map_err(|_e| Error::JniEnvCall("Failed to get byte array region"))?;

        if num_read_bytes == -1 {
            // End of stream reached
            Ok(0)
        } else {
            Ok(num_read_bytes as usize)
        }
    }
}

impl<'a> Drop for JReaderInputStream<'a> {
    fn drop(&mut self) {
        if let Ok(mut env) = vm().attach_current_thread() {
            // Call the Java Reader's `close` method
            let _call_result = env.call_method(&self.internal, "close", "()V", &[]);
            jni_check_exception(&mut env).ok(); // ignore close result error by using .ok()
        }
    }
}

/// Wrapper for the Java class  `ai.yobix.StringResult`
/// Upon creation it parses the java StringResult object and saves the converted Rust string
pub(crate) struct JStringResult {
    pub(crate) content: String,
}

impl<'local> JStringResult {
    pub(crate) fn new(env: &mut JNIEnv<'local>, obj: JObject<'local>) -> ExtractResult<Self> {
        let is_error = env.call_method(&obj, "isError", "()Z", &[])?.z()?;

        if is_error {
            let status = env.call_method(&obj, "getStatus", "()B", &[])?.b()?;
            let msg_obj = env
                .call_method(&obj, "getErrorMessage", "()Ljava/lang/String;", &[])?
                .l()?;
            let msg = jni_jobject_to_string(env, msg_obj)?;
            match status {
                1 => Err(Error::IoError(msg)),
                2 => Err(Error::ParseError(msg)),
                _ => Err(Error::Unknown(msg)),
            }
        } else {
            let call_result_obj = env
                .call_method(&obj, "getContent", "()Ljava/lang/String;", &[])?
                .l()?;

            let content = jni_jobject_to_string(env, call_result_obj)?;

            Ok(Self { content })
        }
    }
}

/// Wrapper for the Java class  `ai.yobix.ReaderResult`
/// Upon creation it parses the java ReaderResult object and saves the java
/// `org.apache.commons.io.input.ReaderInputStream` object, which later can be used for reading
pub(crate) struct JReaderResult<'local> {
    pub(crate) java_reader: JObject<'local>,
}

impl<'local> JReaderResult<'local> {
    pub(crate) fn new(env: &mut JNIEnv<'local>, obj: JObject<'local>) -> ExtractResult<Self> {
        let is_error = env.call_method(&obj, "isError", "()Z", &[])?.z()?;

        if is_error {
            let status = env.call_method(&obj, "getStatus", "()B", &[])?.b()?;
            let msg_obj = env
                .call_method(&obj, "getErrorMessage", "()Ljava/lang/String;", &[])?
                .l()?;
            let msg = jni_jobject_to_string(env, msg_obj)?;
            match status {
                1 => Err(Error::IoError(msg)),
                2 => Err(Error::ParseError(msg)),
                _ => Err(Error::Unknown(msg)),
            }
        } else {
            let reader_obj = env
                .call_method(
                    &obj,
                    "getReader",
                    "()Lorg/apache/commons/io/input/ReaderInputStream;",
                    &[],
                )?
                .l()?;

            Ok(Self {
                java_reader: reader_obj,
            })
        }
    }
}

/// Wrapper for [`JObject`]s that contain `org.apache.tika.parser.pdf.PDFParserConfig`.
/// Looks up the class and method IDs on creation rather than for every method call.
pub(crate) struct JPDFParserConfig<'local> {
    pub(crate) internal: JObject<'local>,
}

impl<'local> JPDFParserConfig<'local> {
    /// Creates a new object instance of `JPDFParserConfig` in the java world
    /// keeps reference to the object and method IDs for later use
    pub(crate) fn new(env: &mut JNIEnv<'local>, config: &PdfParserConfig) -> ExtractResult<Self> {
        // Create the java object
        let class = env.find_class("org/apache/tika/parser/pdf/PDFParserConfig")?;
        let obj = env.new_object(&class, "()V", &[])?;

        // Call the setters
        // Make sure all of these methods are declared in jni-config.json file, otherwise
        // java method not found exception will be thrown
        env.call_method(
            &obj,
            "setExtractInlineImages",
            "(Z)V",
            &[JValue::from(config.extract_inline_images)],
        )?;
        env.call_method(
            &obj,
            "setExtractUniqueInlineImagesOnly",
            "(Z)V",
            &[JValue::from(config.extract_unique_inline_images_only)],
        )?;
        env.call_method(
            &obj,
            "setExtractMarkedContent",
            "(Z)V",
            &[JValue::from(config.extract_marked_content)],
        )?;
        env.call_method(
            &obj,
            "setExtractAnnotationText",
            "(Z)V",
            &[JValue::from(config.extract_annotation_text)],
        )?;
        // The PdfOcrStrategy enum names must match the Java org.apache.tika.parser.pdf
        // .PDFParserConfig$OCR_STRATEGY enum names
        let ocr_str_val = jni_new_string_as_jvalue(env, &config.ocr_strategy.to_string())?;
        env.call_method(
            &obj,
            "setOcrStrategy",
            "(Ljava/lang/String;)V",
            &[(&ocr_str_val).into()],
        )?;

        Ok(Self { internal: obj })
    }
}

/// Wrapper for [`JObject`]s that contain `org.apache.tika.parser.microsoft.OfficeParserConfig`.
pub(crate) struct JOfficeParserConfig<'local> {
    pub(crate) internal: JObject<'local>,
}

impl<'local> JOfficeParserConfig<'local> {
    /// Creates a new object instance of `JOfficeParserConfig` in the java world
    /// keeps reference to the object for later use
    pub(crate) fn new(
        env: &mut JNIEnv<'local>,
        config: &OfficeParserConfig,
    ) -> ExtractResult<Self> {
        // Create the java object
        let class = env.find_class("org/apache/tika/parser/microsoft/OfficeParserConfig")?;
        let obj = env.new_object(&class, "()V", &[])?;

        // Call the setters
        // Make sure all of these methods are declared in jni-config.json file, otherwise
        // java method not found exception will be thrown
        env.call_method(
            &obj,
            "setExtractMacros",
            "(Z)V",
            &[JValue::from(config.extract_macros)],
        )?;
        env.call_method(
            &obj,
            "setIncludeDeletedContent",
            "(Z)V",
            &[JValue::from(config.include_deleted_content)],
        )?;
        env.call_method(
            &obj,
            "setIncludeMoveFromContent",
            "(Z)V",
            &[JValue::from(config.include_move_from_content)],
        )?;
        env.call_method(
            &obj,
            "setIncludeShapeBasedContent",
            "(Z)V",
            &[JValue::from(config.include_shape_based_content)],
        )?;
        env.call_method(
            &obj,
            "setIncludeHeadersAndFooters",
            "(Z)V",
            &[JValue::from(config.include_headers_and_footers)],
        )?;
        env.call_method(
            &obj,
            "setIncludeMissingRows",
            "(Z)V",
            &[JValue::from(config.include_missing_rows)],
        )?;
        env.call_method(
            &obj,
            "setIncludeSlideNotes",
            "(Z)V",
            &[JValue::from(config.include_slide_notes)],
        )?;
        env.call_method(
            &obj,
            "setIncludeSlideMasterContent",
            "(Z)V",
            &[JValue::from(config.include_slide_master_content)],
        )?;
        env.call_method(
            &obj,
            "setConcatenatePhoneticRuns",
            "(Z)V",
            &[JValue::from(config.concatenate_phonetic_runs)],
        )?;
        env.call_method(
            &obj,
            "setExtractAllAlternativesFromMSG",
            "(Z)V",
            &[JValue::from(config.extract_all_alternatives_from_msg)],
        )?;

        Ok(Self { internal: obj })
    }
}

/// Wrapper for [`JObject`]s that contain `org.apache.tika.parser.ocr.TesseractOCRConfig`.
pub(crate) struct JTesseractOcrConfig<'local> {
    pub(crate) internal: JObject<'local>,
}
impl<'local> JTesseractOcrConfig<'local> {
    /// Creates a new object instance of `JTesseractOcrConfig` in the java world
    /// keeps reference to the object for later use
    pub(crate) fn new(
        env: &mut JNIEnv<'local>,
        config: &TesseractOcrConfig,
    ) -> ExtractResult<Self> {
        // Create the java object
        let class = env.find_class("org/apache/tika/parser/ocr/TesseractOCRConfig")?;
        let obj = env.new_object(&class, "()V", &[])?;

        // Call the setters
        // Make sure all of these methods are declared in jni-config.json file, otherwise
        // java method not found exception will be thrown
        env.call_method(&obj, "setDensity", "(I)V", &[JValue::from(config.density)])?;
        env.call_method(&obj, "setDepth", "(I)V", &[JValue::from(config.depth)])?;
        env.call_method(
            &obj,
            "setTimeoutSeconds",
            "(I)V",
            &[JValue::from(config.timeout_seconds)],
        )?;
        env.call_method(
            &obj,
            "setEnableImagePreprocessing",
            "(Z)V",
            &[JValue::from(config.enable_image_preprocessing)],
        )?;
        env.call_method(
            &obj,
            "setApplyRotation",
            "(Z)V",
            &[JValue::from(config.apply_rotation)],
        )?;

        let lang_string_val = jni_new_string_as_jvalue(env, &config.language)?;
        env.call_method(
            &obj,
            "setLanguage",
            "(Ljava/lang/String;)V",
            &[(&lang_string_val).into()],
        )?;

        Ok(Self { internal: obj })
    }
}
