use crate::arena::{Arena, Id};

pub type Symbol = Id<String>;

pub fn alloc_symbol(arena: &mut Arena<String>, name: String) -> Symbol {
    arena.alloc(name)
}
