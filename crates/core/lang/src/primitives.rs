named_enum! {
    pub enum Primitive {
        I8 = "i8",
        I16 = "i16",
        I32 = "i32",
        I64 = "i64",
        I128 = "i128",
        ISize = "isize",
        U8 = "u8",
        U16 = "u16",
        U32 = "u32",
        U64 = "u64",
        U128 = "u128",
        USize = "usize",
        F32 = "f32",
        F64 = "f64",
        Bool = "bool",
        Char = "char",
        /// Text, a view of bytes that live elsewhere.
        Str = "str",
        /// A pointer to text ending in a zero byte, for C.
        CStr = "cstr",
    }
}

impl Primitive {
    pub fn int(self) -> Option<IntTy> {
        IntTy::from_name(self.name())
    }

    pub fn float(self) -> Option<FloatTy> {
        FloatTy::from_name(self.name())
    }
}

named_enum! {
    pub enum IntTy {
        I8 = "i8",
        I16 = "i16",
        I32 = "i32",
        I64 = "i64",
        I128 = "i128",
        ISize = "isize",
        U8 = "u8",
        U16 = "u16",
        U32 = "u32",
        U64 = "u64",
        U128 = "u128",
        USize = "usize",
    }
}

impl IntTy {
    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::ISize
        )
    }

    /// `isize` and `usize` count as 64 bits, the widest a pointer can be, so a value that
    /// fits on one target fits on all.
    pub fn bits(self) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 | IntTy::ISize | IntTy::USize => 64,
            IntTy::I128 | IntTy::U128 => 128,
        }
    }

    pub fn min(self) -> i128 {
        match self.is_signed() {
            true => i128::MIN >> (128 - self.bits()),
            false => 0,
        }
    }

    pub fn max(self) -> u128 {
        let magnitude = self.bits() - self.is_signed() as u32;
        u128::MAX >> (128 - magnitude)
    }
}

named_enum! {
    pub enum FloatTy {
        F32 = "f32",
        F64 = "f64",
    }
}
