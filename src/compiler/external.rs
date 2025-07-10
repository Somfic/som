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

pub fn libc_write() -> LibCCall {
    LibCCall {
        name: "libc_write".into(),
        address: libc::write as *const u8,
        signature: |module| {
            let mut sig = module.make_signature();
            let ptr_t = module.isa().pointer_type();
            sig.params
                .push(AbiParam::new(cranelift::prelude::types::I32)); // fd
            sig.params.push(AbiParam::new(ptr_t)); // buf ptr
            sig.params.push(AbiParam::new(ptr_t)); // length
            sig.returns.push(AbiParam::new(ptr_t));
            sig
        },
    }
}

pub fn libc_getpid() -> LibCCall {
    LibCCall {
        name: "libc_getpid".into(),
        address: libc::getpid as *const u8,
        signature: |module| {
            let mut sig = module.make_signature();
            sig.returns
                .push(AbiParam::new(cranelift::prelude::types::I32));
            sig
        },
    }
}

pub fn init_codebase() -> (JITModule, HashMap<String, DeclarationValue>) {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();

    let extern_declarations = [libc_write(), libc_getpid()];

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
