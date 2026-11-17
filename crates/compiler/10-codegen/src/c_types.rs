use som_mir::IntTy;

pub(crate) struct CInt {
    pub name: &'static str,
    /// The unsigned type as wide.
    pub unsigned: &'static str,
    /// The macros for its smallest and largest values. C has none past 64 bits.
    pub limits: Option<(&'static str, &'static str)>,
}

pub(crate) fn c_int(int: IntTy) -> CInt {
    let (name, unsigned, limits) = match int {
        IntTy::I8 => ("int8_t", "uint8_t", Some(("INT8_MIN", "INT8_MAX"))),
        IntTy::I16 => ("int16_t", "uint16_t", Some(("INT16_MIN", "INT16_MAX"))),
        IntTy::I32 => ("int32_t", "uint32_t", Some(("INT32_MIN", "INT32_MAX"))),
        IntTy::I64 => ("int64_t", "uint64_t", Some(("INT64_MIN", "INT64_MAX"))),
        IntTy::I128 => ("__int128", "unsigned __int128", None),
        IntTy::ISize => ("intptr_t", "uintptr_t", Some(("INTPTR_MIN", "INTPTR_MAX"))),
        IntTy::U8 => ("uint8_t", "uint8_t", Some(("0", "UINT8_MAX"))),
        IntTy::U16 => ("uint16_t", "uint16_t", Some(("0", "UINT16_MAX"))),
        IntTy::U32 => ("uint32_t", "uint32_t", Some(("0", "UINT32_MAX"))),
        IntTy::U64 => ("uint64_t", "uint64_t", Some(("0", "UINT64_MAX"))),
        IntTy::U128 => ("unsigned __int128", "unsigned __int128", None),
        IntTy::USize => ("uintptr_t", "uintptr_t", Some(("0", "UINTPTR_MAX"))),
    };
    CInt {
        name,
        unsigned,
        limits,
    }
}

/// The width every target agrees on: `isize` and `usize` are at least 32 bits.
pub(crate) fn portable_bits(int: IntTy) -> u32 {
    match int {
        IntTy::ISize | IntTy::USize => 32,
        _ => int.bits(),
    }
}

/// Whether C does arithmetic on `int` as an `int`, not its own type.
pub(crate) fn is_promoted(int: IntTy) -> bool {
    int.bits() < 32
}
