use kagent_core::{
    AgentContextLink, AgentLensSnapshot, AgentSessionSummary, AgentStatusKind, AttentionLevel,
    ProjectContextSummary, RepoContextSummary, RepoDirtySummary, ServiceContextSummary,
    ServiceHealthStatus, ServiceHealthSummary, ServicePortSummary, TrackingKind,
    WorktreeSetSummary, WorktreeSummary,
};
use kagent_ui::{AgentLensViewModel, render_agent_lens_text};

const SANGO_SNAPSHOT_JSON: &str = r#"
{
  "schema_version": 1,
  "generated_at": "2026-05-06T12:34:56Z",
  "project_root": "/tmp/my-product",
  "warnings": [],
  "project": {
    "name": "my-product",
    "root": "/tmp/my-product",
    "active_worktree_set": "auth-refactor"
  },
  "repos": [
    {
      "id": "repo",
      "path": "/tmp/my-product/repo",
      "default_branch": "main",
      "services": [
        "api",
        "repo"
      ]
    }
  ],
  "services": [
    {
      "id": "api",
      "repo_id": "repo",
      "type": "process",
      "shared": false,
      "port_base": 3000,
      "depends_on": [
        "db"
      ]
    },
    {
      "id": "db",
      "type": "docker",
      "shared": true,
      "port_base": 5432
    },
    {
      "id": "repo",
      "repo_id": "repo",
      "type": "process",
      "shared": false
    }
  ],
  "worktree_sets": [
    {
      "id": "auth-refactor",
      "active": true,
      "repo_worktrees": [
        {
          "id": "auth-refactor:repo",
          "repo_id": "repo",
          "worktree_set_id": "auth-refactor",
          "path": "/tmp/my-product/worktrees/auth-refactor/repo",
          "branch": "feature/auth-refactor",
          "head": "abc123",
          "exists": true,
          "dirty": {
            "files": 3,
            "staged": 1,
            "unstaged": 1,
            "untracked": 1
          }
        }
      ]
    }
  ],
  "service_instances": [
    {
      "id": "auth-refactor:api",
      "service_id": "api",
      "repo_id": "repo",
      "worktree_set_id": "auth-refactor",
      "type": "process",
      "shared": false,
      "status": "running",
      "health": {
        "status": "ok",
        "checked_at": "2026-05-06T12:35:01Z",
        "url": "http://localhost:3100/health"
      },
      "pid": 18302,
      "ports": [
        {
          "name": "default",
          "base": 3000,
          "actual": 3100,
          "url": "http://localhost:3100",
          "open": true
        }
      ],
      "depends_on": [
        "db"
      ],
      "restart_count": 2,
      "port_listening": true,
      "process_alive": true,
      "verified_at": "2026-05-06T12:35:01Z"
    },
    {
      "id": "shared:db",
      "service_id": "db",
      "worktree_set_id": "shared",
      "type": "docker",
      "shared": true,
      "status": "stopped",
      "health": {
        "status": "unchecked"
      },
      "ports": [
        {
          "name": "default",
          "base": 5432,
          "actual": 5432,
          "open": false
        }
      ]
    }
  ]
}
"#;

fn main() {
    match command_output(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(output) => {
            print!("{output}");
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    }
}

fn command_output(args: &[String]) -> Result<String, String> {
    let command = args.first().map(|arg| arg.as_str()).unwrap_or("dash");
    let flags = if args.is_empty() { &[][..] } else { &args[1..] };

    match command {
        "dash" => dash_output(flags),
        "context" => {
            reject_flags("context", flags)?;
            Ok("kagent context skeleton\n".to_owned())
        }
        _ => Err(format!("unknown command: {command}")),
    }
}

fn dash_output(flags: &[String]) -> Result<String, String> {
    match flags {
        [] => Ok("kagent dashboard skeleton\n".to_owned()),
        [flag] if flag == "--snapshot" => snapshot_output(),
        [flag, ..] => Err(format!("unknown dash flag: {flag}")),
    }
}

fn snapshot_output() -> Result<String, String> {
    let snapshot =
        kagent_sango::parse_snapshot_str(SANGO_SNAPSHOT_JSON).map_err(|error| error.to_string())?;
    let lens_snapshot = lens_snapshot_from_sango(&snapshot);
    let model = AgentLensViewModel::from_snapshot(lens_snapshot);

    Ok(render_agent_lens_text(&model))
}

fn reject_flags(command: &str, flags: &[String]) -> Result<(), String> {
    if let Some(flag) = flags.first() {
        Err(format!("unknown {command} flag: {flag}"))
    } else {
        Ok(())
    }
}

fn lens_snapshot_from_sango(snapshot: &kagent_sango::Snapshot) -> AgentLensSnapshot {
    AgentLensSnapshot {
        project: project_context_from_sango(snapshot),
        sessions: mock_agent_sessions(),
        agent_contexts: mock_agent_contexts(snapshot),
    }
}

fn project_context_from_sango(snapshot: &kagent_sango::Snapshot) -> ProjectContextSummary {
    ProjectContextSummary {
        name: snapshot.project.name.clone(),
        root: snapshot.project.root.clone(),
        active_worktree_set_id: Some(snapshot.project.active_worktree_set.clone()),
        worktree_sets: snapshot
            .worktree_sets
            .iter()
            .map(|worktree_set| WorktreeSetSummary {
                id: worktree_set.id.clone(),
                active: worktree_set.active,
                worktrees: worktree_set
                    .repo_worktrees
                    .iter()
                    .map(|worktree| WorktreeSummary {
                        id: worktree.id.clone(),
                        repo_id: worktree.repo_id.clone(),
                        worktree_set_id: worktree.worktree_set_id.clone(),
                        path: worktree.path.clone(),
                        branch: worktree.branch.clone(),
                        head: worktree.head.clone(),
                        exists: worktree.exists,
                    })
                    .collect(),
            })
            .collect(),
        repos: repo_contexts_from_sango(snapshot),
        services: snapshot
            .service_instances
            .iter()
            .map(service_context_from_sango)
            .collect(),
    }
}

fn repo_contexts_from_sango(snapshot: &kagent_sango::Snapshot) -> Vec<RepoContextSummary> {
    snapshot
        .worktree_sets
        .iter()
        .flat_map(|worktree_set| {
            worktree_set.repo_worktrees.iter().map(|worktree| {
                let repo = snapshot
                    .repos
                    .iter()
                    .find(|repo| repo.id == worktree.repo_id);

                RepoContextSummary {
                    repo_id: worktree.repo_id.clone(),
                    worktree_id: Some(worktree.id.clone()),
                    worktree_set_id: Some(worktree.worktree_set_id.clone()),
                    path: worktree.path.clone(),
                    default_branch: repo.map(|repo| repo.default_branch.clone()),
                    branch: worktree.branch.clone(),
                    head: worktree.head.clone(),
                    exists: worktree.exists,
                    service_ids: repo
                        .map(|repo| repo.services.clone())
                        .unwrap_or_else(Vec::new),
                    dirty: RepoDirtySummary {
                        files: worktree.dirty.files,
                        staged: worktree.dirty.staged,
                        unstaged: worktree.dirty.unstaged,
                        untracked: worktree.dirty.untracked,
                    },
                }
            })
        })
        .collect()
}

fn service_context_from_sango(instance: &kagent_sango::ServiceInstance) -> ServiceContextSummary {
    ServiceContextSummary {
        service_id: instance.service_id.clone(),
        instance_id: Some(instance.id.clone()),
        repo_id: instance.repo_id.clone(),
        worktree_set_id: Some(instance.worktree_set_id.clone()),
        service_type: instance.service_type.clone(),
        shared: instance.shared,
        status: instance.status.clone(),
        health: ServiceHealthSummary {
            status: service_health_status(&instance.health.status),
            checked_at: instance.health.checked_at.clone(),
            url: instance.health.url.clone(),
            last_error: instance.health.last_error.clone(),
        },
        ports: instance
            .ports
            .iter()
            .map(|port| ServicePortSummary {
                name: port.name.clone(),
                base: port.base,
                actual: port.actual,
                url: port.url.clone(),
                open: port.open,
            })
            .collect(),
    }
}

fn mock_agent_sessions() -> Vec<AgentSessionSummary> {
    vec![
        AgentSessionSummary {
            id: "worker-3".to_owned(),
            agent_kind: "codex".to_owned(),
            session_name: "Worker 3".to_owned(),
            status: AgentStatusKind::NeedsApproval,
            attention: AttentionLevel::NeedsUser,
            tracking: TrackingKind::Tracked,
            unread: true,
            last_message: Some("Snapshot renderer ready for verification.".to_owned()),
        },
        AgentSessionSummary {
            id: "reviewer".to_owned(),
            agent_kind: "codex".to_owned(),
            session_name: "Reviewer".to_owned(),
            status: AgentStatusKind::Running,
            attention: AttentionLevel::Info,
            tracking: TrackingKind::Inferred,
            unread: false,
            last_message: Some("Watching the Agent Lens context.".to_owned()),
        },
    ]
}

fn mock_agent_contexts(snapshot: &kagent_sango::Snapshot) -> Vec<AgentContextLink> {
    let active_worktree_set = snapshot.project.active_worktree_set.clone();
    let repo_ids = snapshot
        .repos
        .iter()
        .map(|repo| repo.id.clone())
        .collect::<Vec<_>>();
    let service_ids = snapshot
        .services
        .iter()
        .map(|service| service.id.clone())
        .collect::<Vec<_>>();

    vec![AgentContextLink {
        session_id: "worker-3".to_owned(),
        worktree_set_id: Some(active_worktree_set),
        repo_ids,
        service_ids,
    }]
}

fn service_health_status(status: &str) -> ServiceHealthStatus {
    match status {
        "ok" | "healthy" | "pass" | "passing" => ServiceHealthStatus::Healthy,
        "degraded" | "warn" | "warning" => ServiceHealthStatus::Degraded,
        "unhealthy" | "fail" | "failed" | "error" => ServiceHealthStatus::Unhealthy,
        _ => ServiceHealthStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_without_snapshot_keeps_skeleton_output() {
        let output = command_output(&["dash".to_owned()]).expect("dash output");

        assert_eq!(output, "kagent dashboard skeleton\n");
    }

    #[test]
    fn dash_snapshot_uses_sango_fixture_context() {
        let output =
            command_output(&["dash".to_owned(), "--snapshot".to_owned()]).expect("snapshot output");

        assert!(output.contains("== Agents ==\n> Worker 3 (codex)"));
        assert!(output.contains(
            "Project: my-product root=/tmp/my-product active_worktree_set=auth-refactor"
        ));
        assert!(output.contains("Repo: repo branch=feature/auth-refactor head=abc123 dirty=3 files staged=1 unstaged=1 untracked=1 path=/tmp/my-product/worktrees/auth-refactor/repo"));
        assert!(output.contains("Service: api type=process status=running health=healthy port=default base=3000 actual=3100 open url=http://localhost:3100"));
        assert!(output.contains("Service: db type=docker status=stopped health=unknown port=default base=5432 actual=5432 closed"));
        assert!(output.contains("Severity: critical"));
    }

    #[test]
    fn unknown_dash_flag_is_an_error() {
        let error =
            command_output(&["dash".to_owned(), "--live".to_owned()]).expect_err("unknown flag");

        assert_eq!(error, "unknown dash flag: --live");
    }
}
