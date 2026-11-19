//! Calling C while compiling, as Jai's `#run` does: an extern function is looked up in its
//! library (or, for C's own, in the compiler's process, which has the C library) and
//! called for real, through libffi.
//!
//! The interpreter's memory isn't real memory: a pointer is a place in an allocation, not an
//! address. So memory crosses into C through a bridge:
//!
//! - An allocation a call's arguments can reach (through pointers in it, too) is *pinned*: it
//!   gets a real buffer, at the same address for as long as the constant is being worked out.
//!   Before each call, every pinned allocation is copied into its buffer; after it, what C
//!   changed is copied back.
//! - A pointer to a local is passed as a copy of what it points to, written back after.
//! - An address C gives back (`getenv`'s text, `fopen`'s `FILE*`) is a [`Root::Foreign`]
//!   place, read and written as real memory. One inside a pinned buffer is the allocation's.
//!
//! What C keeps of a pinned buffer after the constant is worked out points to nothing; and a
//! constant's value can't hold an address from C, which only exists while compiling.

use crate::machine::Machine;
use crate::memory::{Byte, POINTER, decode, encode, layout};
use crate::{Fault, Loc, Root, Stop, Value, unsupported};
use libffi::middle::{Arg, Cif, CodePtr, Ret, Type};
use som_base::Span;
use som_mir::*;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// An allocation's real buffer, while it's pinned.
pub(crate) struct Pin {
    /// Words, so it's aligned for anything C reads from it.
    words: Vec<u64>,
    len: usize,
}

impl Pin {
    fn new(len: usize) -> Pin {
        Pin {
            words: vec![0; len.div_ceil(8).max(1)],
            len,
        }
    }

    fn address(&self) -> usize {
        self.words.as_ptr() as usize
    }

    fn bytes(&mut self) -> &mut [u8] {
        let len = self.len;
        // SAFETY: `words` holds at least `len` bytes.
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr().cast::<u8>(), len) }
    }
}

/// Why an extern function can't be called while compiling.
pub(crate) enum Missing {
    /// Its library is a static one, which can't be loaded.
    Static(PathBuf),
    /// Its library couldn't be loaded.
    Unloadable(PathBuf, String),
    /// It isn't in its library, or in the C library the compiler has.
    NotFound(Option<PathBuf>),
}

impl Missing {
    pub(crate) fn reason(self) -> String {
        match self {
            Missing::Static(path) => format!(
                "`{}` is a static library: only a shared one (`.dylib`, `.so`) can be \
                 loaded while compiling",
                path.display()
            ),
            Missing::Unloadable(path, why) => {
                format!("`{}` can't be loaded: {why}", path.display())
            }
            Missing::NotFound(Some(path)) => format!("it isn't in `{}`", path.display()),
            Missing::NotFound(None) => "it isn't in the C library the compiler has: say which \
                 library it's in with `extern \"name\"`"
                .to_string(),
        }
    }
}

/// The libraries loaded so far, never unloaded: what C gives back can outlive a constant.
fn loaded() -> &'static Mutex<HashMap<PathBuf, &'static libloading::Library>> {
    static LOADED: OnceLock<Mutex<HashMap<PathBuf, &'static libloading::Library>>> =
        OnceLock::new();
    LOADED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Where C function `name` is: in `library`, loaded the first time, or in the compiler's own
/// process.
fn symbol(name: &str, library: Option<&Path>) -> Result<*mut c_void, Missing> {
    if let Some(path) = library {
        if path.extension().is_some_and(|e| e == "a" || e == "lib") {
            return Err(Missing::Static(path.to_path_buf()));
        }
        let mut loaded = loaded().lock().unwrap_or_else(|e| e.into_inner());
        let lib = match loaded.get(path) {
            Some(&lib) => lib,
            None => {
                // SAFETY: loading a library runs its initializers: that's what calling into it
                // while compiling asks for.
                let lib = unsafe { libloading::Library::new(path) }
                    .map_err(|e| Missing::Unloadable(path.to_path_buf(), e.to_string()))?;
                let lib: &'static libloading::Library = Box::leak(Box::new(lib));
                loaded.insert(path.to_path_buf(), lib);
                lib
            }
        };
        // SAFETY: only the address is taken; it's called through libffi, by its signature.
        if let Ok(found) = unsafe { lib.get::<*mut c_void>(name) } {
            return Ok(*found);
        }
    }
    let this = libloading::os::unix::Library::this();
    // SAFETY: as above.
    match unsafe { this.get::<*mut c_void>(name) } {
        Ok(found) => Ok(*found),
        Err(_) => Err(Missing::NotFound(library.map(Path::to_path_buf))),
    }
}

/// How libffi passes a value of type `ty`, or `None` for one C can't take this way.
fn ffi_type(mir: &Mir, ty: TypeId) -> Option<Type> {
    let types = &mir.types;
    Some(match types.get(ty) {
        Ty::Bool => Type::u8(),
        Ty::Char => Type::u32(),
        Ty::Int(int) => match int {
            IntTy::I8 => Type::i8(),
            IntTy::U8 => Type::u8(),
            IntTy::I16 => Type::i16(),
            IntTy::U16 => Type::u16(),
            IntTy::I32 => Type::i32(),
            IntTy::U32 => Type::u32(),
            IntTy::I64 | IntTy::ISize => Type::i64(),
            IntTy::U64 | IntTy::USize => Type::u64(),
            IntTy::I128 | IntTy::U128 => return None,
        },
        Ty::Float(FloatTy::F32) => Type::f32(),
        Ty::Float(FloatTy::F64) => Type::f64(),
        // A slice's reference is two words, not a C pointer.
        Ty::Ref { to, .. } if matches!(types.get(*to), Ty::Slice(_) | Ty::Str) => return None,
        Ty::Pointer | Ty::Ptr(_) | Ty::CStr | Ty::Ref { .. } => Type::pointer(),
        // A struct by value: its fields, as C lays them out.
        Ty::Struct(..) => {
            let parts = layout(mir, ty)?.parts;
            if parts.is_empty() {
                return None;
            }
            // Without its `()` fields, which the C backend leaves out.
            let fields = (parts.iter())
                .filter(|&&(part, _)| !matches!(mir.types.get(part), Ty::Unit | Ty::FnItem(..)))
                .map(|&(part, _)| ffi_type(mir, part))
                .collect::<Option<Vec<_>>>()?;
            Type::structure(fields)
        }
        _ => return None,
    })
}

/// Bytes `bytes` as words, so libffi can read a value of any alignment from them.
fn words(bytes: &[u8]) -> Vec<u64> {
    let mut words = vec![0u64; bytes.len().div_ceil(8).max(2)];
    // SAFETY: `words` holds at least `bytes.len()` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), words.as_mut_ptr().cast::<u8>(), bytes.len())
    };
    words
}

impl Machine<'_, '_> {
    /// Calls extern function `func`, for real, with `args` of types `arg_tys`: what it
    /// returns.
    pub(crate) fn call_foreign(
        &mut self,
        func: FuncId,
        args: &[Value],
        arg_tys: &[TypeId],
        span: &Span,
    ) -> Result<Value, Stop> {
        let mir = self.state.mir;
        let function = &mir.funcs[func];
        let name = crate::c_name(function);
        let address = symbol(&name, function.library.as_deref())
            .map_err(|missing| Stop::Report(Fault::Foreign(name.clone(), missing), span.clone()))?;

        // Each argument: its type as written, or for one of a variadic's extras, as C
        // promotes it (`f32` to `f64`, what's narrower than an `int` to one).
        let fixed = function.params.len();
        let mut types = Vec::new();
        let mut values = Vec::new();
        for (i, (value, &ty)) in args.iter().zip(arg_tys).enumerate() {
            let ty = if i < fixed { function.params[i] } else { ty };
            let (ty, value) = match (mir.types.get(ty), value) {
                (Ty::Float(FloatTy::F32), _) if i >= fixed => (mir.types.f64, value.clone()),
                (Ty::Bool, Value::Bool(b)) if i >= fixed => (mir.types.i32, Value::Int(*b as i128)),
                (Ty::Int(IntTy::I8 | IntTy::I16 | IntTy::U8 | IntTy::U16), _) if i >= fixed => {
                    (mir.types.i32, value.clone())
                }
                _ => (ty, value.clone()),
            };
            let Some(ffi) = ffi_type(mir, ty) else {
                return unsupported("passing a value of this type to C", span);
            };
            types.push(ffi);
            values.push((ty, value));
        }
        let ret = function.ret;
        let ret_ffi = match mir.types.get(ret) {
            Ty::Unit => Type::void(),
            _ => match ffi_type(mir, ret) {
                Some(ffi) => ffi,
                None => return unsupported("a value of this type from C", span),
            },
        };

        // What's passed: each value's bytes, pointers as real addresses. A pointer to a local
        // is to a copy of what it points to.
        let mut buffers = Vec::new();
        let mut locals: Vec<(Loc, TypeId, Vec<u64>)> = Vec::new();
        for (ty, value) in &values {
            let bytes = match value {
                Value::Ref(
                    loc @ Loc {
                        root: Root::Local { .. },
                        ..
                    },
                ) => {
                    let pointee = match mir.types.get(*ty) {
                        Ty::Ptr(to) | Ty::Ref { to, .. } => *to,
                        Ty::CStr => mir.types.u8,
                        _ => return unsupported("a pointer to a local, passed to C", span),
                    };
                    let local = self.read(loc, pointee, span)?;
                    let Some(encoded) = encode(mir, &local, pointee) else {
                        return unsupported("a local of this type, passed to C", span);
                    };
                    let host = self.host_bytes(&encoded, span)?;
                    let copy = words(&host);
                    let address = copy.as_ptr() as usize;
                    locals.push((loc.clone(), pointee, copy));
                    address.to_le_bytes()[..POINTER].to_vec()
                }
                _ => {
                    let Some(encoded) = encode(mir, value, *ty) else {
                        return unsupported("passing a value of this type to C", span);
                    };
                    self.host_bytes(&encoded, span)?
                }
            };
            buffers.push(words(&bytes));
        }
        // (A local's copy stays where it is as `locals` grows: a `Vec`'s buffer doesn't move.)
        self.sync_in(span)?;

        let cif = match function.variadic {
            true => Cif::new_variadic(types, fixed, ret_ffi),
            false => Cif::new(types, ret_ffi),
        };
        let arg_refs: Vec<Arg> = buffers.iter().map(|b| Arg::new(&b[0])).collect();
        let mut result = [0u64; 8];
        // SAFETY: the signature is the extern declaration's, which says what the function
        // takes; that it's right is what declaring it promises, as when the program runs.
        unsafe {
            cif.call_return_into(CodePtr(address), &arg_refs, Ret::new(&mut result));
        }

        self.sync_out();
        for (loc, pointee, copy) in locals {
            let size = layout(mir, pointee).map_or(0, |l| l.size);
            // SAFETY: `copy` holds at least `size` bytes.
            let bytes = unsafe { std::slice::from_raw_parts(copy.as_ptr().cast::<u8>(), size) };
            let bytes: Vec<Byte> = bytes.iter().map(|&b| Byte::Data(b)).collect();
            let value = decode(mir, &bytes, pointee)
                .or_else(|_| unsupported("what C left in a local", span))?;
            self.write(&loc, pointee, value, span)?;
        }

        if matches!(mir.types.get(ret), Ty::Unit) {
            return Ok(Value::Unit);
        }
        let size = layout(mir, ret).map_or(0, |l| l.size);
        // SAFETY: `result` holds 64 bytes, more than any value libffi returns here.
        let bytes = unsafe { std::slice::from_raw_parts(result.as_ptr().cast::<u8>(), size) };
        let bytes: Vec<Byte> = bytes.iter().map(|&b| Byte::Data(b)).collect();
        decode(mir, &bytes, ret).or_else(|_| unsupported("a value of this type from C", span))
    }

    /// Bytes `bytes` as C sees them: a pointer's as the real address it's at, pinning the
    /// allocation it points into.
    pub(crate) fn host_bytes(&mut self, bytes: &[Byte], span: &Span) -> Result<Vec<u8>, Stop> {
        let mut host = Vec::with_capacity(bytes.len());
        for byte in bytes {
            host.push(match byte {
                Byte::Data(b) => *b,
                Byte::Uninit => 0,
                Byte::Ptr(loc, n) => self.address(loc, span)?.to_le_bytes()[*n as usize],
            });
        }
        Ok(host)
    }

    /// The real address of `loc`: pinning its allocation, or where C's memory is.
    pub(crate) fn address(&mut self, loc: &Loc, span: &Span) -> Result<usize, Stop> {
        match loc.root {
            Root::Foreign { address } => Ok(address),
            Root::Heap { alloc, offset } => {
                if !self.pinned.contains_key(&alloc) {
                    let len = match self.heap.get(alloc) {
                        Some(Some(bytes)) => bytes.len(),
                        _ => return unsupported("memory that's been freed, passed to C", span),
                    };
                    self.pinned.insert(alloc, Pin::new(len));
                }
                Ok(self.pinned[&alloc].address() + offset)
            }
            Root::Local { .. } => {
                unsupported("a pointer to a local, inside what's passed to C", span)
            }
        }
    }

    /// Copies every pinned allocation into its buffer, pinning those they point into too.
    fn sync_in(&mut self, span: &Span) -> Result<(), Stop> {
        let mut done = std::collections::HashSet::new();
        loop {
            let Some(&alloc) = self.pinned.keys().find(|a| !done.contains(*a)) else {
                return Ok(());
            };
            done.insert(alloc);
            let Some(Some(bytes)) = self.heap.get(alloc) else {
                self.pinned.remove(&alloc);
                continue;
            };
            let host = self.host_bytes(&bytes.clone(), span)?;
            let pin = self.pinned.get_mut(&alloc).expect("pinned");
            pin.bytes()[..host.len()].copy_from_slice(&host);
        }
    }

    /// Copies back what C changed in the pinned buffers. A pointer's bytes it didn't change
    /// are still the pointer; whatever else it wrote is data.
    fn sync_out(&mut self) {
        let allocs: Vec<usize> = self.pinned.keys().copied().collect();
        for alloc in allocs {
            let host = self
                .pinned
                .get_mut(&alloc)
                .expect("pinned")
                .bytes()
                .to_vec();
            let expected: Vec<Option<u8>> = match self.heap.get(alloc) {
                Some(Some(bytes)) => (bytes.iter())
                    .map(|b| match b {
                        Byte::Ptr(loc, n) => self
                            .known_address(loc)
                            .map(|a| a.to_le_bytes()[*n as usize]),
                        _ => None,
                    })
                    .collect(),
                _ => continue,
            };
            let Some(Some(bytes)) = self.heap.get_mut(alloc) else {
                continue;
            };
            for (i, byte) in bytes.iter_mut().enumerate() {
                let h = host[i];
                match byte {
                    Byte::Ptr(..) if expected[i] == Some(h) => {}
                    _ => *byte = Byte::Data(h),
                }
            }
        }
    }

    /// The real address of `loc`, if it has one already.
    fn known_address(&self, loc: &Loc) -> Option<usize> {
        match loc.root {
            Root::Foreign { address } => Some(address),
            Root::Heap { alloc, offset } => self.pinned.get(&alloc).map(|p| p.address() + offset),
            Root::Local { .. } => None,
        }
    }

    /// `loc` as the interpreter's own, where it's an address in a pinned buffer: C gave back
    /// a pointer into what it was given (`strchr`).
    pub(crate) fn unpinned(&self, loc: &Loc) -> Loc {
        let Root::Foreign { address } = loc.root else {
            return loc.clone();
        };
        for (&alloc, pin) in &self.pinned {
            let start = pin.address();
            if (start..start + pin.len.max(1)).contains(&address) {
                return Loc::heap(alloc, address - start);
            }
        }
        loc.clone()
    }
}

/// `n` bytes of C's memory at `address`.
///
/// # Safety
///
/// What C gave: reading it is what the program would do when it runs.
pub(crate) fn read_real(address: usize, n: usize) -> Vec<u8> {
    // SAFETY: see above.
    unsafe { std::slice::from_raw_parts(address as *const u8, n) }.to_vec()
}

/// Writes `bytes` to C's memory at `address`.
pub(crate) fn write_real(address: usize, bytes: &[u8]) {
    // SAFETY: as `read_real`.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len()) }
}
