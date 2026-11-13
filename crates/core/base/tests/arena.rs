use som_base::{Arena, Id, IdPrefix};

#[test]
fn removing_an_item_keeps_the_other_ids() {
    let mut arena = Arena::new();
    let a = arena.alloc("a");
    let b = arena.alloc("b");
    let c = arena.alloc("c");
    assert_eq!(arena.remove(b), Some("b"));
    assert_eq!(arena[a], "a");
    assert_eq!(arena[c], "c");
    assert_eq!(arena.get(b), None);
    assert_eq!(arena.len(), 2);
}

#[test]
fn removed_ids_are_not_reused() {
    let mut arena = Arena::new();
    let a = arena.alloc(1);
    arena.remove(a);
    let b = arena.alloc(2);
    assert_ne!(a, b);
    assert_eq!(arena.get(a), None);
    assert_eq!(arena.remove(a), None, "removing twice does nothing");
    assert_eq!(arena.len(), 1);
}

#[test]
fn retain_skips_removed_items_when_iterating() {
    let mut arena = Arena::new();
    for i in 0..5 {
        arena.alloc(i);
    }
    arena.retain(|_, &n| n % 2 == 0);
    let kept: Vec<(usize, i32)> = arena.iter_with_ids().map(|(id, &n)| (id.id, n)).collect();
    assert_eq!(kept, [(0, 0), (2, 2), (4, 4)]);
    assert_eq!(arena.len(), 3);
}

struct Block;

impl IdPrefix for Block {
    const PREFIX: &'static str = "bb";
}

#[test]
fn ids_are_written_with_their_prefix() {
    assert_eq!(Id::<Block>::new(3).to_string(), "bb3");
}
