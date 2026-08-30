#[cfg(test)]
use crate::ir::Ir;
#[cfg(test)]
use crate::lower::lower_and_resolve;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Compile a string of kiru source into an `Ir` by writing it to a temp file.
/// All compiler errors are surfaced via `unwrap` so tests fail loudly.
#[cfg(test)]
pub(crate) fn compile_str(src: &str) -> Ir {
    let file = std::env::temp_dir().join(format!(
        "kiru_test_{}_{}.kiru",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&file, src).expect("write temp config");
    let ir = lower_and_resolve(&file, false).unwrap_or_else(|e| panic!("compile failed: {:?}", e));
    let _ = std::fs::remove_file(&file);
    ir
}
