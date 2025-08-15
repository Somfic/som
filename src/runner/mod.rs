use crate::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub struct Runner {}

impl Runner {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(self, compiled: *const u8, return_type: &TypeValue) -> Result<i64> {
        match return_type {
            TypeValue::I32 => {
                let compiled_fn: extern "C" fn() -> i32 = unsafe { std::mem::transmute(compiled) };

                #[allow(clippy::redundant_closure)]
                let result = catch_unwind(AssertUnwindSafe(|| compiled_fn()));

                match result {
                    Ok(value) => Ok(value as i64), // Properly sign-extend i32 to i64
                    Err(_) => panic!("An error occurred while running the compiled function."),
                }
            }
            TypeValue::I64 => {
                let compiled_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(compiled) };

                #[allow(clippy::redundant_closure)]
                let result = catch_unwind(AssertUnwindSafe(|| compiled_fn()));

                match result {
                    Ok(value) => Ok(value),
                    Err(_) => panic!("An error occurred while running the compiled function."),
                }
            }
            TypeValue::Boolean => {
                let compiled_fn: extern "C" fn() -> i8 = unsafe { std::mem::transmute(compiled) };

                #[allow(clippy::redundant_closure)]
                let result = catch_unwind(AssertUnwindSafe(|| compiled_fn()));

                match result {
                    Ok(value) => Ok(value as i64), // Convert i8 boolean to i64
                    Err(_) => panic!("An error occurred while running the compiled function."),
                }
            }
            _ => {
                // For other types, try to run as i64 (fallback)
                let compiled_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(compiled) };

                #[allow(clippy::redundant_closure)]
                let result = catch_unwind(AssertUnwindSafe(|| compiled_fn()));

                match result {
                    Ok(value) => Ok(value),
                    Err(_) => panic!("An error occurred while running the compiled function."),
                }
            }
        }
    }
}
