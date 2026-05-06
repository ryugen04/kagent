use std::fmt::Write as _;

use kagent_core::{
    AgentContextLink, AgentLensSnapshot, AgentSessionSummary, AttentionLevel, ImpactSeverity,
    ProjectContextSummary, SelectedAgentContext, ServiceHealthStatus, TrackingKind,
    sort_by_attention,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneSize {
    Hidden,
    Compact,
    Normal,
    Expanded,
    Maximized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewMode {
    Transcript,
    RawTerminal,
    LastScreen,
    Events,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLensViewModel {
    pub snapshot: AgentLensSnapshot,
    pub selected_index: usize,
    pub preview_mode: PreviewMode,
}

impl AgentLensViewModel {
    pub fn new(mut sessions: Vec<AgentSessionSummary>) -> Self {
        sort_by_attention(&mut sessions);
        Self::from_snapshot(AgentLensSnapshot {
            project: empty_project_context(),
            sessions,
            agent_contexts: Vec::new(),
        })
    }

    pub fn from_snapshot(mut snapshot: AgentLensSnapshot) -> Self {
        sort_by_attention(&mut snapshot.sessions);
        Self {
            snapshot,
            selected_index: 0,
            preview_mode: PreviewMode::Transcript,
        }
    }

    pub fn selected(&self) -> Option<&AgentSessionSummary> {
        self.snapshot.sessions.get(self.selected_index)
    }

    pub fn selected_context(&self) -> Option<SelectedAgentContext<'_>> {
        self.snapshot.selected_agent_context_at(self.selected_index)
    }
}

pub fn render_agent_lens_text(model: &AgentLensViewModel) -> String {
    let mut output = String::new();

    render_agents_pane(model, &mut output);
    output.push('\n');
    render_conversation_preview_pane(model, &mut output);
    output.push('\n');
    render_context_impact_pane(model, &mut output);

    output
}

fn render_agents_pane(model: &AgentLensViewModel, output: &mut String) {
    output.push_str("== Agents ==\n");

    if model.snapshot.sessions.is_empty() {
        output.push_str("(no agents)\n");
        return;
    }

    for (index, session) in model.snapshot.sessions.iter().enumerate() {
        let marker = if index == model.selected_index {
            ">"
        } else {
            " "
        };
        let unread = if session.unread { "unread" } else { "read" };
        let context = model
            .snapshot
            .agent_contexts
            .iter()
            .find(|context| context.session_id == session.id);
        let last_message = session.last_message.as_deref().unwrap_or("-");

        writeln!(
            output,
            "{marker} {name} ({kind}) status={status} attention={attention} tracking={tracking} {unread}",
            name = session.session_name,
            kind = session.agent_kind,
            status = format_status(session.status),
            attention = format_attention(session.attention),
            tracking = format_tracking(session.tracking),
        )
        .expect("write to string");
        writeln!(output, "  last: {last_message}").expect("write to string");
        writeln!(output, "  context: {}", format_context_link(context)).expect("write to string");
    }
}

fn render_conversation_preview_pane(model: &AgentLensViewModel, output: &mut String) {
    output.push_str("== Conversation Preview ==\n");
    writeln!(
        output,
        "Project: {} root={} active_worktree_set={}",
        model.snapshot.project.name,
        model.snapshot.project.root,
        model
            .snapshot
            .project
            .active_worktree_set_id
            .as_deref()
            .unwrap_or("-")
    )
    .expect("write to string");

    let Some(context) = model.selected_context() else {
        output.push_str("Selected: -\n");
        return;
    };

    writeln!(
        output,
        "Selected: {} ({}) mode={}",
        context.session.session_name,
        context.session.agent_kind,
        format_preview_mode(&model.preview_mode)
    )
    .expect("write to string");

    if let Some(worktree_set) = context.worktree_set {
        writeln!(
            output,
            "Worktree set: {} active={} worktrees={}",
            worktree_set.id,
            worktree_set.active,
            worktree_set
                .worktrees
                .iter()
                .map(|worktree| format!(
                    "{}:{} branch={} head={} exists={}",
                    worktree.repo_id,
                    worktree.path,
                    worktree.branch.as_deref().unwrap_or("-"),
                    worktree.head.as_deref().unwrap_or("-"),
                    worktree.exists
                ))
                .collect::<Vec<_>>()
                .join(" | ")
        )
        .expect("write to string");
    } else {
        output.push_str("Worktree set: -\n");
    }

    if context.repo_contexts.is_empty() {
        output.push_str("Repo: -\n");
    } else {
        for repo in &context.repo_contexts {
            writeln!(
                output,
                "Repo: {} branch={} head={} dirty={} path={}",
                repo.repo_id,
                repo.branch.as_deref().unwrap_or("-"),
                repo.head.as_deref().unwrap_or("-"),
                format_dirty(&repo.dirty),
                repo.path
            )
            .expect("write to string");
        }
    }

    if context.service_contexts.is_empty() {
        output.push_str("Service: -\n");
    } else {
        for service in &context.service_contexts {
            writeln!(
                output,
                "Service: {} type={} status={} health={} port={}",
                service.service_id,
                service.service_type,
                service.status,
                format_health(service.health.status),
                format_ports(&service.ports)
            )
            .expect("write to string");
        }
    }
}

fn render_context_impact_pane(model: &AgentLensViewModel, output: &mut String) {
    output.push_str("== Context Impact ==\n");

    let Some(context) = model.selected_context() else {
        output.push_str("Severity: none\n");
        output.push_str("Dirty repos: 0\n");
        output.push_str("Dirty files: 0\n");
        output.push_str("Service issues: unhealthy=0 degraded=0 closed_ports=0\n");
        return;
    };

    let impact = context.impact_summary();
    writeln!(
        output,
        "Severity: {}",
        format_impact_severity(impact.severity)
    )
    .expect("write to string");
    writeln!(output, "Dirty repos: {}", impact.dirty_repos).expect("write to string");
    writeln!(output, "Dirty files: {}", impact.dirty_files).expect("write to string");
    writeln!(
        output,
        "Service issues: unhealthy={} degraded={} closed_ports={}",
        impact.unhealthy_services, impact.degraded_services, impact.closed_ports
    )
    .expect("write to string");
}

fn empty_project_context() -> ProjectContextSummary {
    ProjectContextSummary {
        name: "-".to_owned(),
        root: "-".to_owned(),
        active_worktree_set_id: None,
        worktree_sets: Vec::new(),
        repos: Vec::new(),
        services: Vec::new(),
    }
}

fn format_context_link(context: Option<&AgentContextLink>) -> String {
    let Some(context) = context else {
        return "unlinked".to_owned();
    };

    format!(
        "worktree={} repos={} services={}",
        context.worktree_set_id.as_deref().unwrap_or("-"),
        format_list(&context.repo_ids),
        format_list(&context.service_ids)
    )
}

fn format_list(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(",")
    }
}

fn format_dirty(dirty: &kagent_core::RepoDirtySummary) -> String {
    format!(
        "{} files staged={} unstaged={} untracked={}",
        dirty.changed_files(),
        dirty.staged,
        dirty.unstaged,
        dirty.untracked
    )
}

fn format_ports(ports: &[kagent_core::ServicePortSummary]) -> String {
    if ports.is_empty() {
        return "-".to_owned();
    }

    ports
        .iter()
        .map(|port| {
            let state = if port.open { "open" } else { "closed" };
            match &port.url {
                Some(url) => format!(
                    "{} base={} actual={} {} url={}",
                    port.name, port.base, port.actual, state, url
                ),
                None => format!(
                    "{} base={} actual={} {}",
                    port.name, port.base, port.actual, state
                ),
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn format_status(status: kagent_core::AgentStatusKind) -> &'static str {
    match status {
        kagent_core::AgentStatusKind::Starting => "starting",
        kagent_core::AgentStatusKind::Streaming => "streaming",
        kagent_core::AgentStatusKind::ToolRunning => "tool-running",
        kagent_core::AgentStatusKind::Running => "running",
        kagent_core::AgentStatusKind::Idle => "idle",
        kagent_core::AgentStatusKind::NeedsInput => "needs-input",
        kagent_core::AgentStatusKind::NeedsApproval => "needs-approval",
        kagent_core::AgentStatusKind::Blocked => "blocked",
        kagent_core::AgentStatusKind::Done => "done",
        kagent_core::AgentStatusKind::Failed => "failed",
        kagent_core::AgentStatusKind::Exited => "exited",
        kagent_core::AgentStatusKind::Unknown => "unknown",
    }
}

fn format_attention(attention: AttentionLevel) -> &'static str {
    match attention {
        AttentionLevel::None => "none",
        AttentionLevel::Info => "info",
        AttentionLevel::NeedsUser => "needs-user",
        AttentionLevel::Error => "error",
    }
}

fn format_tracking(tracking: TrackingKind) -> &'static str {
    match tracking {
        TrackingKind::Tracked => "tracked",
        TrackingKind::Inferred => "inferred",
    }
}

fn format_preview_mode(mode: &PreviewMode) -> &'static str {
    match mode {
        PreviewMode::Transcript => "transcript",
        PreviewMode::RawTerminal => "raw-terminal",
        PreviewMode::LastScreen => "last-screen",
        PreviewMode::Events => "events",
        PreviewMode::Summary => "summary",
    }
}

fn format_health(status: ServiceHealthStatus) -> &'static str {
    match status {
        ServiceHealthStatus::Healthy => "healthy",
        ServiceHealthStatus::Degraded => "degraded",
        ServiceHealthStatus::Unhealthy => "unhealthy",
        ServiceHealthStatus::Unknown => "unknown",
    }
}

fn format_impact_severity(severity: ImpactSeverity) -> &'static str {
    match severity {
        ImpactSeverity::None => "none",
        ImpactSeverity::Info => "info",
        ImpactSeverity::Warning => "warning",
        ImpactSeverity::Critical => "critical",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kagent_core::{
        AgentContextLink, AgentLensSnapshot, AgentStatusKind, AttentionLevel,
        ProjectContextSummary, RepoContextSummary, RepoDirtySummary, ServiceContextSummary,
        ServiceHealthStatus, ServiceHealthSummary, ServicePortSummary, TrackingKind,
        WorktreeSetSummary, WorktreeSummary,
    };

    #[test]
    fn view_model_selects_first_attention_item() {
        let model = AgentLensViewModel::new(vec![
            AgentSessionSummary {
                id: "done".to_owned(),
                agent_kind: "claude".to_owned(),
                session_name: "done".to_owned(),
                status: AgentStatusKind::Done,
                attention: AttentionLevel::None,
                tracking: TrackingKind::Tracked,
                unread: false,
                last_message: None,
            },
            AgentSessionSummary {
                id: "approval".to_owned(),
                agent_kind: "codex".to_owned(),
                session_name: "approval".to_owned(),
                status: AgentStatusKind::NeedsApproval,
                attention: AttentionLevel::NeedsUser,
                tracking: TrackingKind::Tracked,
                unread: true,
                last_message: Some("Approve command?".to_owned()),
            },
        ]);

        assert_eq!(
            model.selected().map(|session| session.id.as_str()),
            Some("approval")
        );
    }

    #[test]
    fn renderer_outputs_three_deterministic_panes() {
        let model = AgentLensViewModel::from_snapshot(agent_lens_snapshot());

        let rendered = render_agent_lens_text(&model);

        assert_eq!(
            rendered,
            "\
== Agents ==
> worker-3 (codex) status=needs-approval attention=needs-user tracking=tracked unread
  last: Approve cargo test?
  context: worktree=main repos=app services=web,db
  reviewer (claude) status=done attention=none tracking=inferred read
  last: Looks stable
  context: unlinked

== Conversation Preview ==
Project: kagent root=/workspace/kagent active_worktree_set=main
Selected: worker-3 (codex) mode=transcript
Worktree set: main active=true worktrees=app:/workspace/kagent branch=feature/lens head=abc123 exists=true
Repo: app branch=feature/lens head=abc123 dirty=2 files staged=1 unstaged=1 untracked=0 path=/workspace/kagent
Service: web type=process status=running health=healthy port=http base=3000 actual=3100 open url=http://localhost:3100
Service: db type=docker status=stopped health=unknown port=postgres base=5432 actual=5432 closed

== Context Impact ==
Severity: critical
Dirty repos: 1
Dirty files: 2
Service issues: unhealthy=0 degraded=0 closed_ports=1
"
        );
    }

    fn agent_lens_snapshot() -> AgentLensSnapshot {
        AgentLensSnapshot {
            project: ProjectContextSummary {
                name: "kagent".to_owned(),
                root: "/workspace/kagent".to_owned(),
                active_worktree_set_id: Some("main".to_owned()),
                worktree_sets: vec![WorktreeSetSummary {
                    id: "main".to_owned(),
                    active: true,
                    worktrees: vec![WorktreeSummary {
                        id: "main:app".to_owned(),
                        repo_id: "app".to_owned(),
                        worktree_set_id: "main".to_owned(),
                        path: "/workspace/kagent".to_owned(),
                        branch: Some("feature/lens".to_owned()),
                        head: Some("abc123".to_owned()),
                        exists: true,
                    }],
                }],
                repos: vec![RepoContextSummary {
                    repo_id: "app".to_owned(),
                    worktree_id: Some("main:app".to_owned()),
                    worktree_set_id: Some("main".to_owned()),
                    path: "/workspace/kagent".to_owned(),
                    default_branch: Some("main".to_owned()),
                    branch: Some("feature/lens".to_owned()),
                    head: Some("abc123".to_owned()),
                    exists: true,
                    service_ids: vec!["web".to_owned()],
                    dirty: RepoDirtySummary {
                        files: 2,
                        staged: 1,
                        unstaged: 1,
                        untracked: 0,
                    },
                }],
                services: vec![
                    ServiceContextSummary {
                        service_id: "web".to_owned(),
                        instance_id: Some("main:web".to_owned()),
                        repo_id: Some("app".to_owned()),
                        worktree_set_id: Some("main".to_owned()),
                        service_type: "process".to_owned(),
                        shared: false,
                        status: "running".to_owned(),
                        health: ServiceHealthSummary {
                            status: ServiceHealthStatus::Healthy,
                            checked_at: None,
                            url: Some("http://localhost:3100/health".to_owned()),
                            last_error: None,
                        },
                        ports: vec![ServicePortSummary {
                            name: "http".to_owned(),
                            base: 3000,
                            actual: 3100,
                            url: Some("http://localhost:3100".to_owned()),
                            open: true,
                        }],
                    },
                    ServiceContextSummary {
                        service_id: "db".to_owned(),
                        instance_id: Some("shared:db".to_owned()),
                        repo_id: None,
                        worktree_set_id: Some("shared".to_owned()),
                        service_type: "docker".to_owned(),
                        shared: true,
                        status: "stopped".to_owned(),
                        health: ServiceHealthSummary {
                            status: ServiceHealthStatus::Unknown,
                            checked_at: None,
                            url: None,
                            last_error: None,
                        },
                        ports: vec![ServicePortSummary {
                            name: "postgres".to_owned(),
                            base: 5432,
                            actual: 5432,
                            url: None,
                            open: false,
                        }],
                    },
                ],
            },
            sessions: vec![
                AgentSessionSummary {
                    id: "reviewer".to_owned(),
                    agent_kind: "claude".to_owned(),
                    session_name: "reviewer".to_owned(),
                    status: AgentStatusKind::Done,
                    attention: AttentionLevel::None,
                    tracking: TrackingKind::Inferred,
                    unread: false,
                    last_message: Some("Looks stable".to_owned()),
                },
                AgentSessionSummary {
                    id: "worker-3".to_owned(),
                    agent_kind: "codex".to_owned(),
                    session_name: "worker-3".to_owned(),
                    status: AgentStatusKind::NeedsApproval,
                    attention: AttentionLevel::NeedsUser,
                    tracking: TrackingKind::Tracked,
                    unread: true,
                    last_message: Some("Approve cargo test?".to_owned()),
                },
            ],
            agent_contexts: vec![AgentContextLink {
                session_id: "worker-3".to_owned(),
                worktree_set_id: Some("main".to_owned()),
                repo_ids: vec!["app".to_owned()],
                service_ids: vec!["web".to_owned(), "db".to_owned()],
            }],
        }
    }
}
