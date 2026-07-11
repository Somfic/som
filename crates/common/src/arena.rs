use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

pub struct Arena<T> {
    items: Vec<T>,
}

impl<T> Index<Id<T>> for Arena<T> {
    type Output = T;
    fn index(&self, id: Id<T>) -> &T {
        self.items.get(id.id).unwrap()
    }
}

impl<T> IndexMut<Id<T>> for Arena<T> {
    fn index_mut(&mut self, index: Id<T>) -> &mut Self::Output {
        self.items.get_mut(index.id).unwrap()
    }
}

impl<T: Debug> Debug for Arena<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(
                self.items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (Id::<T>::new(i), item)),
            )
            .finish()
    }
}

pub struct Id<T> {
    _marker: PhantomData<T>,
    pub id: usize,
}

impl<T> Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.id)
    }
}

impl<T> Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Id({})", self.id)
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}
impl<T> Eq for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self._marker == other._marker && self.id == other.id
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> Id<T> {
    pub const fn new(id: usize) -> Self {
        Self {
            _marker: PhantomData,
            id,
        }
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self { items: vec![] }
    }

    pub fn alloc(&mut self, item: T) -> Id<T> {
        let id = Id::new(self.items.len());
        self.items.push(item);
        id
    }

    pub fn get(&self, id: &Id<T>) -> &T {
        self.items.get(id.id).unwrap()
    }

    pub fn get_mut(&mut self, id: &Id<T>) -> &mut T {
        self.items.get_mut(id.id).unwrap()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items.iter_mut()
    }

    pub fn iter_with_ids(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.items
            .iter()
            .enumerate()
            .map(|(i, item)| (Id::new(i), item))
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T> IntoIterator for Arena<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Arena<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Arena<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}

/// A generational key into a [`GenArena`]. Unlike [`Id`], a `GenId` carries a
/// generation, so a key that outlives its slot fails a lookup instead of
/// silently aliasing whatever now occupies that slot.
///
/// The `index`/`generation` fields are private on purpose: a `GenId` is an
/// opaque ticket. Store it, compare it, hash it — never reconstruct one from a
/// raw index.
pub struct GenId<T> {
    _marker: PhantomData<T>,
    index: u32,
    generation: u32,
}

impl<T> GenId<T> {
    const fn new(index: u32, generation: u32) -> Self {
        Self {
            _marker: PhantomData,
            index,
            generation,
        }
    }
}

impl<T> Display for GenId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.index)
    }
}

impl<T> Debug for GenId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GenId({}@{})", self.index, self.generation)
    }
}

impl<T> Clone for GenId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for GenId<T> {}
impl<T> Eq for GenId<T> {}

impl<T> PartialEq for GenId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T> Hash for GenId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

/// One backing slot. Both variants carry a generation so that a stale key is
/// rejected whether its slot was reused (generation moved on) or is still
/// vacant (not `Occupied`).
enum Entry<T> {
    Occupied { generation: u32, value: T },
    Free { generation: u32, next_free: Option<u32> },
}

/// A generational arena: like [`Arena`], but slots can be freed and reused, and
/// lookups are fallible. A [`GenId`] is valid iff its slot is `Occupied` *and*
/// the slot's generation matches the key's.
///
/// Freeing a slot bumps its generation and pushes it onto a free list; the next
/// `insert` reuses it. Any key handed out before the free carries the old
/// generation, so its lookups return `None` from then on.
pub struct GenArena<T> {
    items: Vec<Entry<T>>,
    free_head: Option<u32>,
    len: usize,
}

impl<T> Default for GenArena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> GenArena<T> {
    pub fn new() -> Self {
        Self {
            items: vec![],
            free_head: None,
            len: 0,
        }
    }

    /// Insert a value, reusing a freed slot if one is available. Returns a key
    /// valid until the value is removed.
    pub fn insert(&mut self, value: T) -> GenId<T> {
        self.len += 1;

        match self.free_head {
            Some(index) => {
                let slot = &mut self.items[index as usize];
                let (generation, next_free) = match slot {
                    Entry::Free {
                        generation,
                        next_free,
                    } => (*generation, *next_free),
                    Entry::Occupied { .. } => unreachable!("free list pointed at an occupied slot"),
                };
                *slot = Entry::Occupied { generation, value };
                self.free_head = next_free;
                GenId::new(index, generation)
            }
            None => {
                let index = self.items.len() as u32;
                self.items.push(Entry::Occupied {
                    generation: 0,
                    value,
                });
                GenId::new(index, 0)
            }
        }
    }

    /// Remove a value, returning it. A stale key (wrong generation, or a slot
    /// that's already free) is a no-op returning `None`.
    pub fn remove(&mut self, id: GenId<T>) -> Option<T> {
        // Validate before taking the mutable borrow needed for the swap.
        match self.items.get(id.index as usize) {
            Some(Entry::Occupied { generation, .. }) if *generation == id.generation => {}
            _ => return None,
        }

        let next_free = self.free_head;
        let slot = &mut self.items[id.index as usize];
        let generation = match slot {
            Entry::Occupied { generation, .. } => generation.wrapping_add(1),
            _ => unreachable!(),
        };

        let old = std::mem::replace(
            slot,
            Entry::Free {
                generation,
                next_free,
            },
        );
        self.free_head = Some(id.index);
        self.len -= 1;

        match old {
            Entry::Occupied { value, .. } => Some(value),
            _ => unreachable!(),
        }
    }

    pub fn get(&self, id: GenId<T>) -> Option<&T> {
        match self.items.get(id.index as usize)? {
            Entry::Occupied { generation, value } if *generation == id.generation => Some(value),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, id: GenId<T>) -> Option<&mut T> {
        match self.items.get_mut(id.index as usize)? {
            Entry::Occupied { generation, value } if *generation == id.generation => Some(value),
            _ => None,
        }
    }

    pub fn contains(&self, id: GenId<T>) -> bool {
        self.get(id).is_some()
    }

    /// Number of live (occupied) values.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter().filter_map(|slot| match slot {
            Entry::Occupied { value, .. } => Some(value),
            Entry::Free { .. } => None,
        })
    }

    pub fn iter_with_ids(&self) -> impl Iterator<Item = (GenId<T>, &T)> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| match slot {
                Entry::Occupied { generation, value } => {
                    Some((GenId::new(index as u32, *generation), value))
                }
                Entry::Free { .. } => None,
            })
    }
}

impl<T> Index<GenId<T>> for GenArena<T> {
    type Output = T;
    fn index(&self, id: GenId<T>) -> &T {
        self.get(id).expect("indexed GenArena with a stale key")
    }
}

impl<T> IndexMut<GenId<T>> for GenArena<T> {
    fn index_mut(&mut self, id: GenId<T>) -> &mut T {
        self.get_mut(id).expect("indexed GenArena with a stale key")
    }
}

impl<T: Debug> Debug for GenArena<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter_with_ids()).finish()
    }
}

#[cfg(test)]
mod gen_arena_tests {
    use super::*;

    #[test]
    fn insert_get_roundtrips() {
        let mut arena = GenArena::new();
        let a = arena.insert("hello");
        assert_eq!(arena.get(a), Some(&"hello"));
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn removed_key_is_dead() {
        let mut arena = GenArena::new();
        let a = arena.insert("hello");
        assert_eq!(arena.remove(a), Some("hello"));
        assert_eq!(arena.get(a), None);
        assert_eq!(arena.remove(a), None);
        assert!(arena.is_empty());
    }

    /// The ABA test: after a slot is reused, the *old* key must not resolve to
    /// the *new* occupant. This is the whole reason `GenArena` exists.
    #[test]
    fn stale_key_does_not_alias_reused_slot() {
        let mut arena = GenArena::new();

        let a = arena.insert("first");
        arena.remove(a);

        // Reuses slot 0 — same index as `a`, bumped generation.
        let b = arena.insert("second");

        assert_ne!(a, b);
        assert_eq!(arena.get(b), Some(&"second"));
        assert_eq!(arena.get(a), None, "stale key aliased the reused slot");
    }

    #[test]
    fn free_list_reuses_before_growing() {
        let mut arena = GenArena::new();
        let a = arena.insert(1);
        let _b = arena.insert(2);
        arena.remove(a);

        // Should reuse `a`'s slot rather than push a third.
        let c = arena.insert(3);
        assert_eq!(arena.len(), 2);
        assert_eq!(arena.get(c), Some(&3));
        assert_eq!(arena.get(a), None);
    }
}
