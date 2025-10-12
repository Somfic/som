use crate::prelude::TypeValue;
use std::sync::{Arc, Mutex};

// A thread-safe struct to store compilation result
pub struct CompiledCode {
    // This is unsafe to share between threads, but we'll ensure it's only accessed
    // from the main thread after compilation completes
    pub code: Option<*const u8>,
    pub return_type: Option<TypeValue>,
}

// We need to implement Send and Sync manually since raw pointers don't have them
unsafe impl Send for CompiledCode {}
unsafe impl Sync for CompiledCode {}

impl CompiledCode {
    pub fn new() -> Self {
        Self {
            code: None,
            return_type: None,
        }
    }

    pub fn set_code(&mut self, code: *const u8, return_type: TypeValue) {
        self.code = Some(code);
        self.return_type = Some(return_type);
    }
}

// A global to store the result of compilation
pub static COMPILED_CODE: once_cell::sync::Lazy<Arc<Mutex<CompiledCode>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(CompiledCode::new())));
