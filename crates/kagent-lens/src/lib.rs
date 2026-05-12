use kagent_core::{
    AgentContextLink, AgentLensSnapshot, AgentSessionSummary, AgentStatusKind, AttentionLevel,
    ProjectContextSummary, RepoContextSummary, RepoDirtySummary, ServiceContextSummary,
    ServiceHealthStatus, ServiceHealthSummary, ServicePortSummary, StatusSource, TrackingKind,
    WorktreeSetSummary, WorktreeSummary,
};
use kagent_git::{GitCommandProvider, RepoSummary};
use kagent_kitty::{
    KittyFocuser, KittyScreenReader, KittyTab, KittyTabLister, KittyWindow, classify_window,
};
use kagent_ui::{AgentLensRepoView, AgentLensTabView, AgentLensViewModel, AgentLensWindowView};
use std::collections::{BTreeMap, BTreeSet};

pub const SANGO_SNAPSHOT_JSON: &str = r#"
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveDashState {
    pub model: AgentLensViewModel,
    pub self_session_ids: BTreeSet<String>,
}

pub fn live_dash_state(provider: &impl KittyTabLister) -> Result<LiveDashState, String> {
    match provider.list_tabs() {
        Ok(tabs) => {
            let self_session_ids = tabs
                .iter()
                .flat_map(|tab| tab.windows.iter())
                .filter(|window| window.is_self)
                .map(kitty_session_id)
                .collect();
            let snapshot = live_lens_snapshot(&tabs);
            let live_tabs = live_tab_views(&tabs);
            Ok(LiveDashState {
                self_session_ids,
                model: AgentLensViewModel::from_live_snapshot(snapshot, live_tabs),
            })
        }
        Err(error) => Ok(unavailable_dash_state(&error)),
    }
}

pub fn unavailable_dash_state(reason: &str) -> LiveDashState {
    let root = current_project_root();
    let project = live_project_context(&[], root.as_deref());
    LiveDashState {
        model: AgentLensViewModel::from_snapshot(AgentLensSnapshot {
            project,
            sessions: vec![AgentSessionSummary {
                id: "provider:error".to_owned(),
                agent_kind: "kitty".to_owned(),
                session_name: "kitty unavailable".to_owned(),
                status: AgentStatusKind::Failed,
                attention: AttentionLevel::Error,
                tracking: TrackingKind::Inferred,
                unread: true,
                last_message: Some(reason.to_owned()),
                source_window_id: None,
                cwd: root,
                is_self: false,
                is_active: false,
                status_source: StatusSource::ProcessState,
                status_confidence_percent: 100,
                status_message: Some("kitty remote control could not be queried".to_owned()),
            }],
            agent_contexts: Vec::new(),
        }),
        self_session_ids: BTreeSet::new(),
    }
}

pub fn live_lens_snapshot(tabs: &[KittyTab]) -> AgentLensSnapshot {
    AgentLensSnapshot {
        project: live_project_context(tabs, None),
        sessions: tabs
            .iter()
            .flat_map(|tab| tab.windows.iter().cloned())
            .map(session_from_kitty_window)
            .collect(),
        agent_contexts: live_agent_contexts(tabs),
    }
}

pub fn live_tab_views(tabs: &[KittyTab]) -> Vec<AgentLensTabView> {
    let git_provider = GitCommandProvider;
    tabs.iter()
        .map(|tab| AgentLensTabView {
            id: tab.id.clone(),
            title: tab.title.clone(),
            is_active: tab.is_active,
            windows: tab
                .windows
                .iter()
                .map(|window| AgentLensWindowView {
                    session_id: kitty_session_id(window),
                    window_id: window.id.clone(),
                    title: window.title.clone(),
                    cwd: normalized_window_cwd(window),
                    kind: classify_window(window).label().to_owned(),
                    is_active: window.is_active,
                    is_self: window.is_self,
                    foreground_cmdline: if window.foreground_cmdline.is_empty() {
                        window.cmdline.clone()
                    } else {
                        window.foreground_cmdline.clone()
                    },
                    repos: discover_window_repos(&git_provider, window)
                        .into_iter()
                        .map(repo_view)
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

pub fn repo_views(repos: Vec<RepoSummary>) -> Vec<AgentLensRepoView> {
    repos.into_iter().map(repo_view).collect()
}

pub fn session_from_kitty_window(window: KittyWindow) -> AgentSessionSummary {
    let agent_kind = classify_window(&window).label().to_owned();
    let session_name = session_name(&window);
    let last_message = Some(metadata_message(&window));
    let status = if agent_kind == "codex" {
        AgentStatusKind::Running
    } else {
        AgentStatusKind::Idle
    };
    let status_source = if agent_kind == "codex" {
        StatusSource::TerminalHeuristic
    } else {
        StatusSource::ProcessState
    };
    let status_confidence_percent = if agent_kind == "codex" { 45 } else { 85 };
    let status_message = Some(if agent_kind == "codex" {
        "codex foreground process; screen text not sampled yet".to_owned()
    } else {
        format!("{agent_kind} foreground process is not an agent")
    });

    let source_window_id = Some(window.id.clone());
    let cwd = normalized_window_cwd(&window);
    let is_self = window.is_self;
    let is_active = window.is_active;

    let attention = match status {
        AgentStatusKind::Failed | AgentStatusKind::Blocked => AttentionLevel::Error,
        AgentStatusKind::NeedsApproval | AgentStatusKind::NeedsInput => AttentionLevel::NeedsUser,
        AgentStatusKind::Running
        | AgentStatusKind::Streaming
        | AgentStatusKind::ToolRunning
        | AgentStatusKind::Starting => AttentionLevel::Info,
        _ => AttentionLevel::None,
    };

    AgentSessionSummary {
        id: kitty_session_id(&window),
        agent_kind,
        session_name,
        status,
        attention,
        tracking: TrackingKind::Inferred,
        unread: false,
        last_message,
        source_window_id,
        cwd,
        is_self,
        is_active,
        status_source,
        status_confidence_percent,
        status_message,
    }
}

fn live_project_context(tabs: &[KittyTab], fallback_root: Option<&str>) -> ProjectContextSummary {
    let git_provider = GitCommandProvider;
    let mut repos = collect_live_repos(&git_provider, tabs);
    if repos.is_empty() {
        if let Some(root) = fallback_root {
            for repo in git_provider.discover_repo_summaries(root) {
                repos.entry(repo.path.clone()).or_insert(repo);
            }
        }
    }
    let root = active_window_cwd(tabs)
        .or_else(|| fallback_root.map(str::to_owned))
        .or_else(current_project_root)
        .unwrap_or_else(|| ".".to_owned());
    let name = cwd_basename(&root).unwrap_or("kagent").to_owned();
    let worktree_set_id = "live".to_owned();

    ProjectContextSummary {
        name,
        root,
        active_worktree_set_id: Some(worktree_set_id.clone()),
        worktree_sets: vec![WorktreeSetSummary {
            id: worktree_set_id.clone(),
            active: true,
            worktrees: repos
                .values()
                .map(|repo| WorktreeSummary {
                    id: format!("live:{}", repo.id),
                    repo_id: repo.id.clone(),
                    worktree_set_id: worktree_set_id.clone(),
                    path: repo.path.clone(),
                    branch: repo.branch.clone(),
                    head: None,
                    exists: true,
                })
                .collect(),
        }],
        repos: repos
            .values()
            .map(|repo| RepoContextSummary {
                repo_id: repo.id.clone(),
                worktree_id: Some(format!("live:{}", repo.id)),
                worktree_set_id: Some(worktree_set_id.clone()),
                path: repo.path.clone(),
                default_branch: None,
                branch: repo.branch.clone(),
                head: None,
                exists: true,
                service_ids: Vec::new(),
                dirty: RepoDirtySummary {
                    files: repo.dirty_files as u32,
                    staged: 0,
                    unstaged: 0,
                    untracked: 0,
                },
            })
            .collect(),
        services: Vec::new(),
    }
}

fn live_agent_contexts(tabs: &[KittyTab]) -> Vec<AgentContextLink> {
    let git_provider = GitCommandProvider;
    tabs.iter()
        .flat_map(|tab| tab.windows.iter())
        .map(|window| AgentContextLink {
            session_id: kitty_session_id(window),
            worktree_set_id: Some("live".to_owned()),
            repo_ids: discover_window_repos(&git_provider, window)
                .into_iter()
                .map(|repo| repo.id)
                .collect(),
            service_ids: Vec::new(),
        })
        .collect()
}

fn collect_live_repos(
    git_provider: &GitCommandProvider,
    tabs: &[KittyTab],
) -> BTreeMap<String, RepoSummary> {
    let mut repos = BTreeMap::new();
    for repo in tabs
        .iter()
        .flat_map(|tab| tab.windows.iter())
        .flat_map(|window| discover_window_repos(git_provider, window))
    {
        repos.entry(repo.path.clone()).or_insert(repo);
    }
    repos
}

fn discover_window_repos(
    git_provider: &GitCommandProvider,
    window: &KittyWindow,
) -> Vec<RepoSummary> {
    normalized_window_cwd(window)
        .map(|cwd| git_provider.discover_repo_summaries(&cwd))
        .unwrap_or_default()
}

fn repo_view(repo: RepoSummary) -> AgentLensRepoView {
    AgentLensRepoView {
        id: repo.id,
        path: repo.path,
        branch: repo.branch,
        dirty_files: repo.dirty_files,
        pr: None,
    }
}

fn active_window_cwd(tabs: &[KittyTab]) -> Option<String> {
    tabs.iter()
        .flat_map(|tab| tab.windows.iter())
        .find(|window| window.is_self)
        .and_then(normalized_window_cwd)
        .or_else(|| {
            tabs.iter()
                .flat_map(|tab| tab.windows.iter())
                .find(|window| window.is_active)
                .and_then(normalized_window_cwd)
        })
}

fn normalized_window_cwd(window: &KittyWindow) -> Option<String> {
    window.cwd.as_deref().map(normalize_cwd)
}

fn normalize_cwd(cwd: &str) -> String {
    let Some(rest) = cwd.strip_prefix("file://") else {
        return cwd.to_owned();
    };

    if rest.starts_with('/') {
        rest.to_owned()
    } else {
        rest.find('/')
            .map(|index| rest[index..].to_owned())
            .unwrap_or_else(|| rest.to_owned())
    }
}

fn current_project_root() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

pub fn refresh_selected_preview(provider: &impl KittyScreenReader, model: &mut AgentLensViewModel) {
    let Some(session) = model.snapshot.sessions.get_mut(model.selected_index) else {
        return;
    };
    let Some(window_id) = session.id.strip_prefix("kitty:") else {
        return;
    };

    match provider.screen_text(window_id) {
        Ok(text) if !text.trim().is_empty() => {
            let inferred_status = infer_status_from_screen_text(&text, &session.agent_kind);
            session.last_message = Some(text);
            session.status = inferred_status.kind;
            session.attention = inferred_status.attention;
            session.status_source = StatusSource::TerminalHeuristic;
            session.status_confidence_percent = inferred_status.confidence_percent;
            session.status_message = Some(inferred_status.message);
        }
        Ok(_) => {}
        Err(error) => {
            session.last_message = Some(format!("preview unavailable: {error}"));
            session.status_message = Some("selected preview could not be refreshed".to_owned());
        }
    }
}

pub fn focus_selected_window(
    provider: &impl KittyFocuser,
    state: &LiveDashState,
) -> Result<(), String> {
    let Some(session) = state.model.selected() else {
        return Ok(());
    };

    if state.self_session_ids.contains(&session.id) {
        return Ok(());
    }

    let Some(window_id) = session.id.strip_prefix("kitty:") else {
        return Ok(());
    };

    provider.focus_window(window_id)
}

pub fn kitty_session_id(window: &KittyWindow) -> String {
    format!("kitty:{}", window.id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredStatus {
    pub kind: AgentStatusKind,
    pub attention: AttentionLevel,
    pub confidence_percent: u8,
    pub message: String,
}

pub fn infer_status_from_screen_text(text: &str, agent_kind: &str) -> InferredStatus {
    let normalized = text.to_lowercase();
    let lines = normalized.lines().collect::<Vec<_>>();
    let recent = tail_lines(&lines, 24);
    let terminal_tail = tail_lines(&lines, 8);

    if contains_any(
        recent,
        &[
            "needs approval",
            "approve command",
            "permission request",
            "approval required",
        ],
    ) {
        return inferred_status(
            AgentStatusKind::NeedsApproval,
            AttentionLevel::NeedsUser,
            82,
            "approval-related prompt in selected screen",
        );
    }

    if contains_any(
        recent,
        &[
            "conversation interrupted",
            "tell the model what to do",
            "what should",
            "waiting for input",
            "needs input",
        ],
    ) {
        return inferred_status(
            AgentStatusKind::NeedsInput,
            AttentionLevel::NeedsUser,
            78,
            "input prompt in selected screen",
        );
    }

    if contains_any(
        recent,
        &[
            "worked for",
            "task complete",
            "completed",
            "token usage:",
            "to continue this session",
        ],
    ) {
        return inferred_status(
            AgentStatusKind::Done,
            AttentionLevel::None,
            74,
            "completion marker in selected screen",
        );
    }

    if contains_terminal_failure(terminal_tail) {
        return inferred_status(
            AgentStatusKind::Failed,
            AttentionLevel::Error,
            72,
            "terminal-tail error marker in selected screen",
        );
    }

    if agent_kind == "codex" {
        inferred_status(
            AgentStatusKind::Running,
            AttentionLevel::Info,
            55,
            "codex session with no stronger selected-screen signal",
        )
    } else {
        inferred_status(
            AgentStatusKind::Idle,
            AttentionLevel::None,
            70,
            "non-agent foreground process",
        )
    }
}

pub fn lens_snapshot_from_sango(snapshot: &kagent_sango::Snapshot) -> AgentLensSnapshot {
    AgentLensSnapshot {
        project: project_context_from_sango(snapshot),
        sessions: mock_agent_sessions(),
        agent_contexts: mock_agent_contexts(snapshot),
    }
}

pub fn project_context_from_sango(snapshot: &kagent_sango::Snapshot) -> ProjectContextSummary {
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

pub fn service_health_label(status: ServiceHealthStatus) -> &'static str {
    match status {
        ServiceHealthStatus::Healthy => "healthy",
        ServiceHealthStatus::Degraded => "degraded",
        ServiceHealthStatus::Unhealthy => "unhealthy",
        ServiceHealthStatus::Unknown => "unknown",
    }
}

fn session_name(window: &KittyWindow) -> String {
    if !window.title.trim().is_empty() {
        return window.title.clone();
    }

    window
        .cwd
        .as_deref()
        .and_then(cwd_basename)
        .unwrap_or("kitty window")
        .to_owned()
}

fn metadata_message(window: &KittyWindow) -> String {
    let cwd = window.cwd.as_deref().unwrap_or("-");
    let cmdline = if window.foreground_cmdline.is_empty() {
        &window.cmdline
    } else {
        &window.foreground_cmdline
    };
    let cmdline = if cmdline.is_empty() {
        "-".to_owned()
    } else {
        cmdline.join(" ")
    };

    format!(
        "kitty window {} cwd={} active={} self={} cmd={}",
        window.id, cwd, window.is_active, window.is_self, cmdline
    )
}

fn cwd_basename(cwd: &str) -> Option<&str> {
    let without_scheme = cwd.strip_prefix("file://").unwrap_or(cwd);
    without_scheme
        .trim_end_matches('/')
        .rsplit('/')
        .find(|segment| !segment.is_empty())
}

fn tail_lines<'a>(lines: &'a [&str], count: usize) -> &'a [&'a str] {
    let start = lines.len().saturating_sub(count);
    &lines[start..]
}

fn inferred_status(
    kind: AgentStatusKind,
    attention: AttentionLevel,
    confidence_percent: u8,
    message: &str,
) -> InferredStatus {
    InferredStatus {
        kind,
        attention,
        confidence_percent,
        message: message.to_owned(),
    }
}

fn contains_any(lines: &[&str], needles: &[&str]) -> bool {
    lines
        .iter()
        .any(|line| needles.iter().any(|needle| line.contains(needle)))
}

fn contains_terminal_failure(lines: &[&str]) -> bool {
    lines
        .iter()
        .filter(|line| !is_ignored_terminal_warning(line))
        .any(|line| {
            line.contains("error:")
                || line.contains("failed")
                || line.contains("panic")
                || line.contains("not_found")
                || line.contains("permission denied")
        })
}

fn is_ignored_terminal_warning(line: &str) -> bool {
    line.contains("failed to refresh skills")
        || line.contains("failed to reload config")
        || line.contains("skills/list failed")
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
            source_window_id: None,
            cwd: None,
            is_self: false,
            is_active: false,
            status_source: StatusSource::Manual,
            status_confidence_percent: 100,
            status_message: Some("fixture session".to_owned()),
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
            source_window_id: None,
            cwd: None,
            is_self: false,
            is_active: false,
            status_source: StatusSource::Manual,
            status_confidence_percent: 60,
            status_message: Some("fixture inferred session".to_owned()),
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
    use std::cell::Cell;

    #[test]
    fn infers_codex_from_foreground_process_before_shell_cmdline() {
        let session = session_from_kitty_window(KittyWindow {
            id: "21".to_owned(),
            title: "kagent".to_owned(),
            cwd: Some("file://host/workspace/kagent".to_owned()),
            cmdline: vec!["bash".to_owned()],
            foreground_cmdline: vec!["/usr/local/bin/codex".to_owned()],
            is_self: true,
            is_active: true,
            screen_text: None,
        });

        assert_eq!(session.id, "kitty:21");
        assert_eq!(session.agent_kind, "codex");
        assert_eq!(session.status, AgentStatusKind::Running);
        assert!(session.last_message.unwrap().contains("self=true"));
    }

    #[test]
    fn self_window_focus_is_noop() {
        let focuser = CountingFocuser::default();
        let state = LiveDashState {
            model: AgentLensViewModel::from_snapshot(AgentLensSnapshot {
                project: project_context_from_sango(
                    &kagent_sango::parse_snapshot_str(SANGO_SNAPSHOT_JSON)
                        .expect("embedded snapshot"),
                ),
                sessions: vec![AgentSessionSummary {
                    id: "kitty:21".to_owned(),
                    agent_kind: "codex".to_owned(),
                    session_name: "kagent".to_owned(),
                    status: AgentStatusKind::Running,
                    attention: AttentionLevel::Info,
                    tracking: TrackingKind::Inferred,
                    unread: false,
                    last_message: None,
                    source_window_id: Some("21".to_owned()),
                    cwd: Some("/workspace/kagent".to_owned()),
                    is_self: true,
                    is_active: true,
                    status_source: StatusSource::TerminalHeuristic,
                    status_confidence_percent: 55,
                    status_message: None,
                }],
                agent_contexts: Vec::new(),
            }),
            self_session_ids: BTreeSet::from(["kitty:21".to_owned()]),
        };

        focus_selected_window(&focuser, &state).expect("focus noop succeeds");

        assert_eq!(focuser.calls.get(), 0);
    }

    #[test]
    fn live_provider_failure_reports_error_without_fixture_sessions() {
        let state = live_dash_state(&FailingLister).expect("error state");

        assert_eq!(state.model.snapshot.sessions.len(), 1);
        let session = &state.model.snapshot.sessions[0];
        assert_eq!(session.id, "provider:error");
        assert_eq!(session.status, AgentStatusKind::Failed);
        assert_eq!(session.attention, AttentionLevel::Error);
        assert_eq!(session.last_message.as_deref(), Some("kitty unavailable"));
        assert!(
            !state
                .model
                .snapshot
                .sessions
                .iter()
                .any(|session| session.id == "worker-3")
        );
    }

    #[test]
    fn live_snapshot_uses_kitty_windows_without_fixture_sessions() {
        let tabs = vec![KittyTab {
            id: "1".to_owned(),
            title: "live".to_owned(),
            is_active: true,
            windows: vec![KittyWindow {
                id: "21".to_owned(),
                title: "kagent".to_owned(),
                cwd: Some("file://host/workspace/kagent".to_owned()),
                cmdline: vec!["bash".to_owned()],
                foreground_cmdline: vec!["codex".to_owned()],
                is_self: true,
                is_active: true,
                screen_text: None,
            }],
        }];

        let snapshot = live_lens_snapshot(&tabs);

        assert_eq!(snapshot.project.name, "kagent");
        assert_eq!(snapshot.project.root, "/workspace/kagent");
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].id, "kitty:21");
        assert_eq!(
            snapshot.sessions[0].cwd.as_deref(),
            Some("/workspace/kagent")
        );
        assert_eq!(snapshot.agent_contexts.len(), 1);
        assert_eq!(snapshot.agent_contexts[0].session_id, "kitty:21");
        assert!(
            !snapshot
                .sessions
                .iter()
                .any(|session| session.id == "worker-3")
        );
    }

    #[test]
    fn status_heuristic_detects_selected_screen_states() {
        assert_eq!(
            infer_status_from_screen_text("Approve command?", "codex").kind,
            AgentStatusKind::NeedsApproval
        );
        assert_eq!(
            infer_status_from_screen_text(
                "Conversation interrupted - tell the model what to do",
                "codex"
            )
            .kind,
            AgentStatusKind::NeedsInput
        );
        assert_eq!(
            infer_status_from_screen_text("Error: permission denied", "codex").kind,
            AgentStatusKind::Failed
        );
        assert_eq!(
            infer_status_from_screen_text(
                "■ failed to refresh skills: skills/list failed in TUI: skills/list failed: failed to reload config: /workspace/config/config.toml:29:2: duplicate key\n\n› Improve documentation in @filename",
                "codex"
            )
            .kind,
            AgentStatusKind::Running
        );
        assert_eq!(
            infer_status_from_screen_text("Worked for 2m 01s", "codex").kind,
            AgentStatusKind::Done
        );
        assert_eq!(
            infer_status_from_screen_text(
                "Error: GitHub API error 404\n\nRan gh pr create\n\nWorked for 2m 01s\n\n■ failed to refresh skills: skills/list failed in TUI: failed to reload config",
                "codex"
            )
            .kind,
            AgentStatusKind::Done
        );
        assert_eq!(
            infer_status_from_screen_text(
                "Error: old failure in transcript\n\nRunning cargo test\nprogress continues\nmore output\nstill working\nwaiting\nalive\nno final failure",
                "codex"
            )
            .kind,
            AgentStatusKind::Running
        );
        assert_eq!(
            infer_status_from_screen_text("Running cargo test", "codex").kind,
            AgentStatusKind::Running
        );
    }

    #[derive(Default)]
    struct CountingFocuser {
        calls: Cell<u32>,
    }

    impl KittyFocuser for CountingFocuser {
        fn focus_window(&self, _window_id: &str) -> Result<(), String> {
            self.calls.set(self.calls.get() + 1);
            Ok(())
        }
    }

    struct FailingLister;

    impl KittyTabLister for FailingLister {
        fn list_tabs(&self) -> Result<Vec<KittyTab>, String> {
            Err("kitty unavailable".to_owned())
        }
    }
}
