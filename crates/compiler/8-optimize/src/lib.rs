//! MIR-to-MIR optimizations, one module per pass. [`optimize`] runs them in order.
//!
//! They run after borrow checking, so errors point at the code as it was written,
//! and code that's never called is still checked. Every pass leaves valid MIR
//! behind, so each can be run, and tested, on its own.
//!
//! | Pass      | Does                                                           |
//! |-----------|----------------------------------------------------------------|
//! | [`fold`]  | Works out constant expressions and branches at compile time    |
//! | [`dce`]   | Removes assignments nothing uses, and the locals they assigned |
//! | [`shake`] | Removes the functions and structs nothing reachable uses       |

pub mod dce;
pub mod fold;
pub mod shake;

pub use dce::dce;
pub use fold::fold;
pub use shake::shake;

use som_mir::Mir;

/// Runs every pass over `mir`, in order.
pub fn optimize(mir: &mut Mir) {
    // Folding first: a branch folded away can leave functions nothing calls any more.
    fold(mir);
    // Then what folding left unread, and what only that read. Before shaking: a struct
    // only dead locals had is unused now.
    dce(mir);
    shake(mir);
}
