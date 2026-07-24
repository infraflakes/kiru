use crate::plan::{Plan, PlanProject, SyncMode};
use crate::runner::execution_context::OutputCallback;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Create a minimal `(Plan, PlanProject, OutputCallback)` triple for runner tests.
/// The callback is a no-op: these tests assert control flow, not output text.
pub(crate) fn test_context() -> (Plan, PlanProject, OutputCallback) {
    let project = PlanProject {
        url: "http://example.com".to_string(),
        dir: "test".to_string(),
        sync: SyncMode::Clone,
        branch: Some("main".to_string()),
        functions: BTreeMap::new(),
    };
    let cfg = Plan {
        projects: BTreeMap::new(),
        runs: BTreeMap::new(),
    };
    (cfg, project, Arc::new(|_| {}))
}
