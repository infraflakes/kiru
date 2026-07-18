use crate::compiler::{Config, Project, SyncMode};
use crate::runner::execution_context::OutputCallback;
use std::collections::HashMap;
use std::sync::Arc;

/// Create a minimal `(Config, Project, OutputCallback)` triple for runner tests.
/// The callback is a no-op: these tests assert control flow, not output text.
pub(crate) fn test_context() -> (Config, Project, OutputCallback) {
    let project = Project {
        url: "http://example.com".to_string(),
        dir: "test".to_string(),
        sync: SyncMode::Clone,
        branch: Some("main".to_string()),
        functions: HashMap::new(),
        runs: HashMap::new(),
    };
    let cfg = Config {
        projects: HashMap::new(),
    };
    (cfg, project, Arc::new(|_| {}))
}
