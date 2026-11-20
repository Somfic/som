/// An enum whose variants each stand for a fixed name.
#[macro_export]
macro_rules! named_enum {
    ($(#[$meta:meta])* $vis:vis enum $Name:ident { $($(#[$vmeta:meta])* $Variant:ident = $text:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        $vis enum $Name { $($(#[$vmeta])* $Variant),* }

        impl $Name {
            pub const ALL: &'static [$Name] = &[$($Name::$Variant),*];

            pub const fn name(self) -> &'static str {
                match self { $($Name::$Variant => $text),* }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($text => Some($Name::$Variant),)* _ => None }
            }
        }
    };
}

named_enum! {
    pub enum LangTrait {
        Iterator = "Iterator",
        IterRef = "IterRef",
        IterMut = "IterMut",
        IterOwned = "IterOwned",
        From = "From",
        Eq = "Eq",
        Add = "Add",
        Subtract = "Subtract",
        Multiply = "Multiply",
        Divide = "Divide",
        Compare = "Compare",
        Drop = "Drop",
        Copy = "Copy",
        Share = "Share",
        Index = "Index",
        IndexMut = "IndexMut",
        Display = "Display",
        Collect = "Collect",
    }
}

impl LangTrait {
    /// The method the language calls on the trait's behalf.
    pub fn method(self) -> Option<&'static str> {
        Some(match self {
            LangTrait::Iterator => "next",
            LangTrait::IterRef => "iter",
            LangTrait::IterMut => "iter_mut",
            LangTrait::IterOwned => "into_iter",
            LangTrait::From => "from",
            LangTrait::Eq => "eq",
            LangTrait::Add => "add",
            LangTrait::Subtract => "subtract",
            LangTrait::Multiply => "multiply",
            LangTrait::Divide => "divide",
            LangTrait::Compare => "compare",
            LangTrait::Drop => "drop",
            LangTrait::Share => "share",
            LangTrait::Index => "at",
            LangTrait::IndexMut => "at_mut",
            LangTrait::Display => "display",
            LangTrait::Copy | LangTrait::Collect => return None,
        })
    }

    /// The associated type the language reads: what an iterator gives, or makes.
    pub fn assoc_type(self) -> Option<&'static str> {
        Some(match self {
            LangTrait::Iterator | LangTrait::Index => "Item",
            LangTrait::IterRef | LangTrait::IterMut | LangTrait::IterOwned => "Iter",
            _ => return None,
        })
    }

    pub fn property(self) -> Option<&'static str> {
        match self {
            LangTrait::Display => Some("display_len"),
            LangTrait::Index => Some(crate::LEN),
            _ => None,
        }
    }

    pub fn is_operator(self) -> bool {
        matches!(
            self,
            LangTrait::Eq
                | LangTrait::Add
                | LangTrait::Subtract
                | LangTrait::Multiply
                | LangTrait::Divide
                | LangTrait::Compare
        )
    }

    pub fn operator_method(self) -> Option<&'static str> {
        self.method().filter(|_| self.is_operator())
    }

    /// Whether only the language calls its method, never a `value.method()` written out.
    pub fn called_implicitly(self) -> bool {
        self.is_operator() || matches!(self, LangTrait::Drop | LangTrait::Share)
    }
}

named_enum! {
    pub enum LangStruct {
        /// Holds a value that's never dropped on its own.
        Manual = "Manual",
        Rc = "Rc",
        String = "string",
        Failure = "Failure",
        Location = "Location",
    }
}

named_enum! {
    pub enum LangFunc {
        Halt = "halt",
        ShareOptional = "share_optional",
        TestStart = "test_start",
        TestPassed = "test_passed",
        TestSummary = "test_summary",
        /// What `let all: List<i32> = numbers.where(…)` calls to collect the values.
        CollectInto = "collect_into",
    }
}

named_enum! {
    pub enum LocationField {
        File = "file",
        Line = "line",
        Column = "column",
        Function = "function",
    }
}

named_enum! {
    pub enum FailureField {
        Message = "message",
        At = "at",
        Unreachable = "unreachable",
    }
}
