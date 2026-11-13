use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// Items stored by id. Removing one leaves its slot empty, so every other id stays valid.
pub struct Arena<T> {
    items: Vec<Option<T>>,
    /// How many of `items` are `Some`.
    live: usize,
}

pub struct Id<T> {
    _marker: PhantomData<T>,
    pub id: usize,
}

/// How the ids of an item type are written: MIR locals as `_3`, blocks as `bb3`.
pub trait IdPrefix {
    const PREFIX: &'static str;
}

impl<T: IdPrefix> Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", T::PREFIX, self.id)
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

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
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
        Self {
            items: vec![],
            live: 0,
        }
    }

    pub fn alloc(&mut self, item: T) -> Id<T> {
        let id = Id::new(self.items.len());
        self.items.push(Some(item));
        self.live += 1;
        id
    }

    /// The item with `id`, or `None` if it was removed.
    pub fn get(&self, id: Id<T>) -> Option<&T> {
        self.items.get(id.id)?.as_ref()
    }

    pub fn get_mut(&mut self, id: Id<T>) -> Option<&mut T> {
        self.items.get_mut(id.id)?.as_mut()
    }

    /// Removes the item with `id`. Its id isn't reused.
    pub fn remove(&mut self, id: Id<T>) -> Option<T> {
        let item = self.items.get_mut(id.id)?.take()?;
        self.live -= 1;
        Some(item)
    }

    /// Keeps only the items `keep` returns true for.
    pub fn retain(&mut self, mut keep: impl FnMut(Id<T>, &T) -> bool) {
        for (i, slot) in self.items.iter_mut().enumerate() {
            if slot.as_ref().is_some_and(|item| !keep(Id::new(i), item)) {
                *slot = None;
                self.live -= 1;
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter().flatten()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items.iter_mut().flatten()
    }

    pub fn iter_with_ids(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| Some((Id::new(i), item.as_ref()?)))
    }

    /// The number of items, not counting removed ones.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// One past the highest id handed out, counting removed items: the length for a
    /// `Vec` indexed by id.
    pub fn slots(&self) -> usize {
        self.items.len()
    }
}

impl<T> Index<Id<T>> for Arena<T> {
    type Output = T;

    fn index(&self, id: Id<T>) -> &T {
        self.get(id).expect("no item with this id")
    }
}

impl<T> IndexMut<Id<T>> for Arena<T> {
    fn index_mut(&mut self, id: Id<T>) -> &mut T {
        self.get_mut(id).expect("no item with this id")
    }
}

impl<T> IntoIterator for Arena<T> {
    type Item = T;
    type IntoIter = std::iter::Flatten<std::vec::IntoIter<Option<T>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter().flatten()
    }
}

impl<'a, T> IntoIterator for &'a Arena<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Flatten<std::slice::Iter<'a, Option<T>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter().flatten()
    }
}

impl<'a, T> IntoIterator for &'a mut Arena<T> {
    type Item = &'a mut T;
    type IntoIter = std::iter::Flatten<std::slice::IterMut<'a, Option<T>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut().flatten()
    }
}

impl<T: Clone> Clone for Arena<T> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            live: self.live,
        }
    }
}
