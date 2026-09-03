#[cfg(test)]
use crate::compile::compile_source;
#[cfg(test)]
use crate::ir::Ir;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Compile a string of kiru source into an `Ir` entirely in memory. All
/// compiler errors are surfaced via `unwrap` so tests fail loudly.
#[cfg(test)]
pub(crate) fn compile_str(src: &str) -> Ir {
    let name = format!("<test {}>", TEMP_COUNTER.fetch_add(1, Ordering::Relaxed));
    compile_source(&name, src).unwrap_or_else(|e| panic!("compile failed: {:?}", e))
}
