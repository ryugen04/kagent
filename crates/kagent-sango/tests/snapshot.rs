use kagent_sango::{SangoCommandProvider, SangoProvider, parse_snapshot_bytes, parse_snapshot_str};
use std::path::PathBuf;

const SNAPSHOT_JSON: &str = include_str!("fixtures/snapshot.json");

#[test]
fn parses_snapshot_fixture() {
    let snapshot = parse_snapshot_str(SNAPSHOT_JSON).expect("fixture should parse");

    assert_eq!(snapshot.metadata.schema_version, 1);
    assert_eq!(snapshot.metadata.generated_at, "2026-05-06T12:34:56Z");
    assert_eq!(snapshot.metadata.project_root, "/tmp/my-product");
    assert_eq!(snapshot.metadata.warnings, Vec::<String>::new());

    assert_eq!(snapshot.project.name, "my-product");
    assert_eq!(snapshot.project.root, "/tmp/my-product");
    assert_eq!(snapshot.project.active_worktree_set, "auth-refactor");

    assert_eq!(snapshot.repos.len(), 1);
    let repo = &snapshot.repos[0];
    assert_eq!(repo.id, "repo");
    assert_eq!(repo.default_branch, "main");
    assert_eq!(repo.services, vec!["api".to_owned(), "repo".to_owned()]);

    assert_eq!(snapshot.services.len(), 3);
    let api = snapshot
        .services
        .iter()
        .find(|service| service.id == "api")
        .expect("api service");
    assert_eq!(api.repo_id.as_deref(), Some("repo"));
    assert_eq!(api.service_type, "process");
    assert!(!api.shared);
    assert_eq!(api.port_base, Some(3000));
    assert_eq!(api.depends_on, vec!["db".to_owned()]);

    assert_eq!(snapshot.worktree_sets.len(), 1);
    let worktree_set = &snapshot.worktree_sets[0];
    assert_eq!(worktree_set.id, "auth-refactor");
    assert!(worktree_set.active);

    let repo_worktree = &worktree_set.repo_worktrees[0];
    assert_eq!(repo_worktree.id, "auth-refactor:repo");
    assert_eq!(
        repo_worktree.branch.as_deref(),
        Some("feature/auth-refactor")
    );
    assert_eq!(repo_worktree.head.as_deref(), Some("abc123"));
    assert!(repo_worktree.exists);
    assert_eq!(repo_worktree.dirty.files, 3);
    assert_eq!(repo_worktree.dirty.staged, 1);
    assert_eq!(repo_worktree.dirty.unstaged, 1);
    assert_eq!(repo_worktree.dirty.untracked, 1);

    let api_instance = snapshot
        .service_instances
        .iter()
        .find(|instance| instance.id == "auth-refactor:api")
        .expect("api instance");
    assert_eq!(api_instance.service_id, "api");
    assert_eq!(api_instance.repo_id.as_deref(), Some("repo"));
    assert_eq!(api_instance.worktree_set_id, "auth-refactor");
    assert_eq!(api_instance.status, "running");
    assert_eq!(api_instance.pid, Some(18302));
    assert_eq!(api_instance.restart_count, Some(2));
    assert_eq!(api_instance.port_listening, Some(true));
    assert_eq!(api_instance.process_alive, Some(true));
    assert_eq!(api_instance.ports[0].actual, 3100);
    assert_eq!(
        api_instance.ports[0].url.as_deref(),
        Some("http://localhost:3100")
    );
    assert_eq!(api_instance.health.status, "ok");
    assert_eq!(
        api_instance.health.url.as_deref(),
        Some("http://localhost:3100/health")
    );

    let shared_db = snapshot
        .service_instances
        .iter()
        .find(|instance| instance.id == "shared:db")
        .expect("shared db instance");
    assert!(shared_db.shared);
    assert_eq!(shared_db.repo_id, None);
    assert_eq!(shared_db.worktree_set_id, "shared");
    assert_eq!(shared_db.health.status, "unchecked");
}

#[test]
fn parses_snapshot_bytes_and_ignores_unknown_fields() {
    let snapshot = parse_snapshot_bytes(SNAPSHOT_JSON.as_bytes()).expect("fixture should parse");

    assert_eq!(snapshot.project.name, "my-product");
    assert_eq!(snapshot.service_instances.len(), 2);
}

#[test]
fn provider_trait_can_be_mocked() {
    struct FixtureProvider;

    impl SangoProvider for FixtureProvider {
        fn snapshot(&self) -> Result<kagent_sango::Snapshot, kagent_sango::SangoError> {
            parse_snapshot_str(SNAPSHOT_JSON)
        }
    }

    let snapshot = FixtureProvider.snapshot().expect("fixture provider");

    assert_eq!(snapshot.project.active_worktree_set, "auth-refactor");
}

#[test]
fn command_provider_error_includes_command_and_root_context() {
    let root = PathBuf::from("/tmp/my-product");
    let provider = SangoCommandProvider::with_binary(root.clone(), "/definitely/not/sango");

    let message = provider
        .snapshot()
        .expect_err("missing binary should fail")
        .to_string();

    assert!(message.contains("/definitely/not/sango snapshot --json --root /tmp/my-product"));
    assert!(message.contains("root `/tmp/my-product`"));
}
