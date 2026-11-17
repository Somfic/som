//! Every name the compiler looks for in the standard library lives here, so a pass never
//! matches a std name as a string literal of its own.

#[macro_use]
mod items;
mod names;

pub use items::{FailureField, LangFunc, LangStruct, LangTrait, LocationField};
pub use names::*;
