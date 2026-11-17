//! The names the language itself gives meaning to: built-in types and `@` words, and what
//! the compiler looks for in the standard library. A pass never spells one out on its own.

#[macro_use]
mod items;
mod builtins;
mod names;
mod primitives;

pub use builtins::Builtin;
pub use items::{FailureField, LangFunc, LangStruct, LangTrait, LocationField};
pub use names::*;
pub use primitives::{FloatTy, IntTy, Primitive};
