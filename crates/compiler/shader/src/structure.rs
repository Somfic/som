//! A function's blocks as structured control flow: `if`s and loops with `break` and
//! `continue`, which shading languages have, rather than jumps, which they don't. Not tied
//! to any one language.
//!
//! Blocks are laid out by dominators: a branch's two sides run until the block both reach
//! again (the branch's immediate post-dominator), and a loop is the blocks that jump back to
//! a block dominating them. A jump to the loop's start is a `continue`, to the one block
//! after it a `break`. som's `if`, `while`, `loop`, `for` and `match` give only such shapes;
//! anything else (a `break` out of two loops at once) is `None`, and the caller lays the
//! blocks out another way.

use som_mir::{Body, BlockId, TerminatorKind};
use std::collections::{HashMap, HashSet};

/// Structured control flow, over a function's blocks.
#[derive(Debug)]
pub(crate) enum Node {
    /// A block's statements: not what it ends with.
    Block(BlockId),
    /// The `Branch` block `on` ends with: its `then`, then its `otherwise`.
    If {
        on: BlockId,
        then: Vec<Node>,
        otherwise: Vec<Node>,
    },
    /// Over and over, until a `Break` or a `Return`.
    Loop(Vec<Node>),
    Break,
    Continue,
    /// What `Return` or `Unreachable` block `from` ends with.
    Return(BlockId),
}

/// `body`'s blocks as structured control flow, or `None` if they can't be.
pub(crate) fn structure(body: &Body) -> Option<Vec<Node>> {
    let graph = Graph::new(body)?;
    let mut s = Structurer {
        graph: &graph,
        emitted: HashSet::new(),
        loops: Vec::new(),
    };
    s.seq(graph.entry, None, false)
}

struct Loop {
    header: usize,
    exit: Option<usize>,
    body: HashSet<usize>,
}

struct Structurer<'g> {
    graph: &'g Graph,
    emitted: HashSet<usize>,
    loops: Vec<&'g Loop>,
}

enum Jump {
    /// To the next block in sequence.
    Fall,
    /// To where the sequence stops.
    Stop,
    Break,
    Continue,
}

impl Structurer<'_> {
    /// From block `b` until `stop`, or until control leaves. With `in_header`, `b` is the
    /// header of the loop just entered: its body starts with it.
    fn seq(&mut self, mut b: usize, stop: Option<usize>, mut in_header: bool) -> Option<Vec<Node>> {
        let g = self.graph;
        let mut out = Vec::new();
        loop {
            if !in_header {
                if Some(b) == stop {
                    return Some(out);
                }
                if let Some(entered) = g.loops.get(&b) {
                    self.loops.push(entered);
                    let inner = self.seq(b, Some(b), true);
                    self.loops.pop();
                    out.push(Node::Loop(inner?));
                    match entered.exit {
                        Some(exit) => {
                            b = exit;
                            continue;
                        }
                        None => return Some(out),
                    }
                }
            }
            in_header = false;
            // Each block once: a second time means this isn't a shape to structure.
            if !self.emitted.insert(b) {
                return None;
            }
            out.push(Node::Block(g.ids[b]));
            match &g.kinds[b] {
                End::Return => {
                    out.push(Node::Return(g.ids[b]));
                    return Some(out);
                }
                End::Goto(to) => match self.jump(*to, stop)? {
                    Jump::Fall => b = *to,
                    Jump::Stop => return Some(out),
                    Jump::Break => {
                        out.push(Node::Break);
                        return Some(out);
                    }
                    Jump::Continue => {
                        out.push(Node::Continue);
                        return Some(out);
                    }
                },
                End::Branch(then, otherwise) => {
                    // Both sides run until they meet again; where that's outside the loop
                    // they're in, each leaves it on its own.
                    let inside = |m: usize| self.loops.last().is_none_or(|l| l.body.contains(&m));
                    let merge = match g.ipdom[b] {
                        Some(m) if inside(m) => Some(m),
                        _ => stop,
                    };
                    let then = self.side(*then, merge)?;
                    let otherwise = self.side(*otherwise, merge)?;
                    out.push(Node::If {
                        on: g.ids[b],
                        then,
                        otherwise,
                    });
                    match merge {
                        Some(m) if merge != stop => b = m,
                        _ => return Some(out),
                    }
                }
            }
        }
    }

    /// One side of a branch, to `merge`: a jump straight there is nothing.
    fn side(&mut self, to: usize, merge: Option<usize>) -> Option<Vec<Node>> {
        match self.jump(to, merge)? {
            Jump::Stop => Some(Vec::new()),
            Jump::Break => Some(vec![Node::Break]),
            Jump::Continue => Some(vec![Node::Continue]),
            Jump::Fall => self.seq(to, merge, false),
        }
    }

    fn jump(&self, to: usize, stop: Option<usize>) -> Option<Jump> {
        if Some(to) == stop {
            return Some(Jump::Stop);
        }
        if let Some(innermost) = self.loops.last() {
            if to == innermost.header {
                return Some(Jump::Continue);
            }
            if Some(to) == innermost.exit {
                return Some(Jump::Break);
            }
        }
        // Out of a loop further out: WGSL's `break` and `continue` only leave the innermost.
        let outer = self.loops.iter().rev().skip(1);
        if outer.into_iter().any(|l| to == l.header || Some(to) == l.exit) {
            return None;
        }
        Some(Jump::Fall)
    }
}

/// What a block ends with, by the indexes of the blocks it goes to.
enum End {
    Goto(usize),
    Branch(usize, usize),
    Return,
}

/// The reachable blocks, numbered from 0 in the order they're reached, with their loops and
/// post-dominators.
struct Graph {
    ids: Vec<BlockId>,
    kinds: Vec<End>,
    entry: usize,
    /// Each loop, by its header.
    loops: HashMap<usize, Loop>,
    /// Each block's immediate post-dominator: `None` past a `return`, or where it never ends.
    ipdom: Vec<Option<usize>>,
}

impl Graph {
    /// `None` for a loop with more than one way out.
    fn new(body: &Body) -> Option<Graph> {
        // Reachable blocks, in the order a depth-first walk reaches them.
        let entry = BlockId::new(0);
        let mut index: HashMap<BlockId, usize> = HashMap::new();
        let mut ids = Vec::new();
        let mut stack = vec![entry];
        while let Some(id) = stack.pop() {
            if index.contains_key(&id) {
                continue;
            }
            index.insert(id, ids.len());
            ids.push(id);
            let mut next = body.blocks[id].terminator.kind.successors();
            next.reverse();
            stack.extend(next);
        }
        let kinds: Vec<End> = (ids.iter())
            .map(|&id| match body.blocks[id].terminator.kind {
                TerminatorKind::Goto(to) => End::Goto(index[&to]),
                TerminatorKind::Branch {
                    then, otherwise, ..
                } => End::Branch(index[&then], index[&otherwise]),
                TerminatorKind::Return | TerminatorKind::Unreachable => End::Return,
            })
            .collect();
        // Never reached: not a way out of anything, so not where branches meet.
        let unreachable: Vec<bool> = (ids.iter())
            .map(|&id| matches!(body.blocks[id].terminator.kind, TerminatorKind::Unreachable))
            .collect();
        let n = ids.len();
        let succs: Vec<Vec<usize>> = (kinds.iter())
            .map(|k| match *k {
                End::Goto(to) => vec![to],
                End::Branch(a, b) => vec![a, b],
                End::Return => Vec::new(),
            })
            .collect();
        let mut preds = vec![Vec::new(); n];
        for (from, to) in succs.iter().enumerate() {
            for &t in to {
                preds[t].push(from);
            }
        }

        let idom = dominators(n, 0, &succs, &preds);
        let dominates = |a: usize, mut b: usize| loop {
            if a == b {
                return true;
            }
            match idom[b] {
                Some(up) if up != b => b = up,
                _ => return false,
            }
        };

        // Loops: the blocks jumping back to one dominating them, and what reaches those.
        let mut bodies: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (from, to) in succs.iter().enumerate() {
            for &header in to {
                if !dominates(header, from) {
                    continue;
                }
                let body = bodies.entry(header).or_insert_with(|| HashSet::from([header]));
                let mut stack = vec![from];
                while let Some(b) = stack.pop() {
                    if body.insert(b) {
                        stack.extend(preds[b].iter().copied());
                    }
                }
            }
        }
        let mut loops = HashMap::new();
        for (header, body) in bodies {
            let exits: HashSet<usize> = (body.iter())
                .flat_map(|&b| succs[b].iter().copied())
                .filter(|s| !body.contains(s))
                .collect();
            // Two ways out: no one `break` is both. Left to the caller's other layout.
            if exits.len() > 1 {
                return None;
            }
            loops.insert(
                header,
                Loop {
                    header,
                    exit: exits.into_iter().next(),
                    body,
                },
            );
        }

        // Post-dominators: dominators of the graph turned around, from one exit after every
        // `return`.
        let exit = n;
        let mut rsuccs = vec![Vec::new(); n + 1];
        let mut rpreds = vec![Vec::new(); n + 1];
        for (from, to) in succs.iter().enumerate() {
            if to.is_empty() && !unreachable[from] {
                rsuccs[exit].push(from);
                rpreds[from].push(exit);
            }
            for &t in to {
                rsuccs[t].push(from);
                rpreds[from].push(t);
            }
        }
        let ipdom = dominators(n + 1, exit, &rsuccs, &rpreds);
        let ipdom = (0..n).map(|b| ipdom[b].filter(|&p| p != exit)).collect();

        Some(Graph {
            ids,
            kinds,
            entry: 0,
            loops,
            ipdom,
        })
    }
}

/// Each node's immediate dominator, from `entry` (Cooper, Harvey and Kennedy's iteration):
/// `entry`'s is itself, and an unreachable node's `None`.
fn dominators(n: usize, entry: usize, succs: &[Vec<usize>], preds: &[Vec<usize>]) -> Vec<Option<usize>> {
    // Reverse postorder from `entry`.
    let mut order = Vec::new();
    let mut seen = vec![false; n];
    let mut stack = vec![(entry, 0)];
    seen[entry] = true;
    while let Some((node, child)) = stack.pop() {
        if let Some(&next) = succs[node].get(child) {
            stack.push((node, child + 1));
            if !seen[next] {
                seen[next] = true;
                stack.push((next, 0));
            }
        } else {
            order.push(node);
        }
    }
    order.reverse();
    let mut rank = vec![usize::MAX; n];
    for (i, &node) in order.iter().enumerate() {
        rank[node] = i;
    }
    let mut idom: Vec<Option<usize>> = vec![None; n];
    idom[entry] = Some(entry);
    let intersect = |idom: &[Option<usize>], mut a: usize, mut b: usize| {
        while a != b {
            while rank[a] > rank[b] {
                a = idom[a].expect("processed");
            }
            while rank[b] > rank[a] {
                b = idom[b].expect("processed");
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &node in order.iter().skip(1) {
            let mut new = None;
            for &p in &preds[node] {
                if idom[p].is_none() {
                    continue;
                }
                new = Some(match new {
                    None => p,
                    Some(current) => intersect(&idom, p, current),
                });
            }
            if new.is_some() && idom[node] != new {
                idom[node] = new;
                changed = true;
            }
        }
    }
    idom
}
