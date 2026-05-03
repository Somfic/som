mod arena;
mod diagnostics;
mod macros;
mod source;
mod span;
mod symbol;

pub use arena::*;
pub use diagnostics::*;
pub use macros::*;
pub use source::*;
pub use span::*;
pub use symbol::*;

pub use tracing::debug;
pub use tracing::error;
pub use tracing::info;
pub use tracing::trace;
pub use tracing::warn;

/// Install a tracing subscriber. Idempotent — safe to call from multiple tests.
///
/// Honors `RUST_LOG`; defaults to `info` when unset.
pub fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_test_writer()
        .try_init();
}
