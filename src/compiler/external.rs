use std::collections::HashMap;

use crate::{compiler::environment::DeclarationValue, prelude::*};
use cranelift::prelude::AbiParam;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

struct LibCCall {
    name: String,
    address: *const u8,
    signature: fn(&JITModule) -> cranelift::prelude::Signature,
}

fn void(module: &JITModule) -> AbiParam {
    AbiParam::new(module.isa().pointer_type())
}

fn i32() -> AbiParam {
    AbiParam::new(cranelift::prelude::types::I32)
}

fn i64() -> AbiParam {
    AbiParam::new(cranelift::prelude::types::I64)
}

pub fn init_codebase() -> (JITModule, HashMap<String, DeclarationValue>) {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();

    let extern_declarations = [
        LibCCall {
            name: "libc_getpid".into(),
            address: libc::getpid as *const u8,
            signature: |module| {
                let mut sig = module.make_signature();
                sig.returns.push(i32());
                sig
            },
        },
        LibCCall {
            name: "libc_write".into(),
            address: libc::write as *const u8,
            signature: |module| {
                let mut sig = module.make_signature();
                let ptr_t = module.isa().pointer_type();
                sig.params.push(i32());
                sig.params.push(i64());
                sig.params.push(i64());
                sig.returns.push(void(module));
                sig
            },
        },
        LibCCall {
            name: "libc_rand".into(),
            address: libc::rand as *const u8,
            signature: |module| {
                let mut sig = module.make_signature();
                sig.returns.push(i32());
                sig
            },
        },
        LibCCall {
            name: "libc_exit".into(),
            address: libc::exit as *const u8,
            signature: |module| {
                let mut sig = module.make_signature();
                sig.params.push(i32());
                sig.returns.push(void(module));
                sig
            },
        },
        LibCCall {
            name: "libc_puts".into(),
            address: libc::puts as *const u8,
            signature: |module| {
                let mut sig = module.make_signature();
                let ptr_t = module.isa().pointer_type();
                sig.params.push(AbiParam::new(ptr_t));
                sig.returns.push(i32());
                sig
            },
        },
    ];

    for decl in &extern_declarations {
        builder.symbol(&decl.name, decl.address);
    }

    let mut env = CompileEnvironment::new(HashMap::new());
    let mut codebase = JITModule::new(builder);

    for decl in extern_declarations {
        let sig = (decl.signature)(&codebase);
        let func_id = codebase
            .declare_function(&decl.name, Linkage::Import, &sig)
            .unwrap();
        env.declare_function(decl.name, func_id);
    }

    (codebase, env.declarations)
}
