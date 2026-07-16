use crate::compiler::{Config, Project, SyncMode};
use crate::runner::execution_context::OutputTarget;
use std::collections::HashMap;

/// Create a minimal `(Config, Project, OutputTarget)` triple for runner tests.
pub(crate) fn test_context() -> (Config, Project, OutputTarget) {
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
    (cfg, project, OutputTarget::Direct(Box::new(Vec::new())))
}
