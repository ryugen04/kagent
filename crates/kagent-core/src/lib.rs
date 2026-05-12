use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatusKind {
    Starting,
    Streaming,
    ToolRunning,
    Running,
    Idle,
    NeedsInput,
    NeedsApproval,
    Blocked,
    Done,
    Failed,
    Exited,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionLevel {
    None,
    Info,
    NeedsUser,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingKind {
    Tracked,
    Inferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusSource {
    ExplicitEvent,
    Adapter,
    TerminalHeuristic,
    ProcessState,
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentStatus {
    pub kind: AgentStatusKind,
    pub source: StatusSource,
    pub confidence: f32,
    pub since: Option<String>,
    pub message: Option<String>,
}

impl AgentStatus {
    pub fn certain(kind: AgentStatusKind, source: StatusSource) -> Self {
        Self {
            kind,
            source,
            confidence: 1.0,
            since: None,
            message: None,
        }
    }

    pub fn uncertain(kind: AgentStatusKind, source: StatusSource, confidence: f32) -> Self {
        Self {
            kind,
            source,
            confidence,
            since: None,
            message: None,
        }
    }

    pub fn is_confident(&self) -> bool {
        self.confidence >= 0.8
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionSummary {
    pub id: String,
    pub agent_kind: String,
    pub session_name: String,
    pub status: AgentStatusKind,
    pub attention: AttentionLevel,
    pub tracking: TrackingKind,
    pub unread: bool,
    pub last_message: Option<String>,
    pub source_window_id: Option<String>,
    pub cwd: Option<String>,
    pub is_self: bool,
    pub is_active: bool,
    pub status_source: StatusSource,
    pub status_confidence_percent: u8,
    pub status_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLensSnapshot {
    pub project: ProjectContextSummary,
    pub sessions: Vec<AgentSessionSummary>,
    pub agent_contexts: Vec<AgentContextLink>,
}

impl AgentLensSnapshot {
    pub fn selected_agent_context(&self, session_id: &str) -> Option<SelectedAgentContext<'_>> {
        let session = self
            .sessions
            .iter()
            .find(|session| session.id == session_id)?;

        Some(self.context_for_session(session))
    }

    pub fn selected_agent_context_at(&self, index: usize) -> Option<SelectedAgentContext<'_>> {
        self.sessions
            .get(index)
            .map(|session| self.context_for_session(session))
    }

    fn context_for_session<'a>(
        &'a self,
        session: &'a AgentSessionSummary,
    ) -> SelectedAgentContext<'a> {
        let context_link = self
            .agent_contexts
            .iter()
            .find(|context| context.session_id == session.id);

        let worktree_set_id = context_link
            .and_then(|context| context.worktree_set_id.as_deref())
            .or(self.project.active_worktree_set_id.as_deref());

        let worktree_set = worktree_set_id
            .and_then(|id| {
                self.project
                    .worktree_sets
                    .iter()
                    .find(|worktree_set| worktree_set.id == id)
            })
            .or_else(|| {
                self.project
                    .worktree_sets
                    .iter()
                    .find(|worktree_set| worktree_set.active)
            });

        let repo_contexts = self
            .project
            .repos
            .iter()
            .filter(|repo| repo_matches_context(repo, worktree_set_id, context_link, &self.project))
            .collect();

        let service_contexts = self
            .project
            .services
            .iter()
            .filter(|service| service_matches_context(service, worktree_set_id, context_link))
            .collect();

        SelectedAgentContext {
            session,
            context_link,
            worktree_set,
            repo_contexts,
            service_contexts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContextSummary {
    pub name: String,
    pub root: String,
    pub active_worktree_set_id: Option<String>,
    pub worktree_sets: Vec<WorktreeSetSummary>,
    pub repos: Vec<RepoContextSummary>,
    pub services: Vec<ServiceContextSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSetSummary {
    pub id: String,
    pub active: bool,
    pub worktrees: Vec<WorktreeSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSummary {
    pub id: String,
    pub repo_id: String,
    pub worktree_set_id: String,
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoContextSummary {
    pub repo_id: String,
    pub worktree_id: Option<String>,
    pub worktree_set_id: Option<String>,
    pub path: String,
    pub default_branch: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub exists: bool,
    pub service_ids: Vec<String>,
    pub dirty: RepoDirtySummary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepoDirtySummary {
    pub files: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
}

impl RepoDirtySummary {
    pub fn is_dirty(&self) -> bool {
        self.files > 0 || self.staged > 0 || self.unstaged > 0 || self.untracked > 0
    }

    pub fn changed_files(&self) -> u32 {
        if self.files > 0 {
            self.files
        } else {
            self.staged + self.unstaged + self.untracked
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceContextSummary {
    pub service_id: String,
    pub instance_id: Option<String>,
    pub repo_id: Option<String>,
    pub worktree_set_id: Option<String>,
    pub service_type: String,
    pub shared: bool,
    pub status: String,
    pub health: ServiceHealthSummary,
    pub ports: Vec<ServicePortSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceHealthSummary {
    pub status: ServiceHealthStatus,
    pub checked_at: Option<String>,
    pub url: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl ServiceHealthStatus {
    pub fn needs_attention(self) -> bool {
        matches!(self, Self::Degraded | Self::Unhealthy)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServicePortSummary {
    pub name: String,
    pub base: u16,
    pub actual: u16,
    pub url: Option<String>,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextLink {
    pub session_id: String,
    pub worktree_set_id: Option<String>,
    pub repo_ids: Vec<String>,
    pub service_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedAgentContext<'a> {
    pub session: &'a AgentSessionSummary,
    pub context_link: Option<&'a AgentContextLink>,
    pub worktree_set: Option<&'a WorktreeSetSummary>,
    pub repo_contexts: Vec<&'a RepoContextSummary>,
    pub service_contexts: Vec<&'a ServiceContextSummary>,
}

impl SelectedAgentContext<'_> {
    pub fn impact_summary(&self) -> AgentImpactSummary {
        let dirty_repos = self
            .repo_contexts
            .iter()
            .filter(|repo| repo.dirty.is_dirty())
            .count();
        let dirty_files = self
            .repo_contexts
            .iter()
            .map(|repo| repo.dirty.changed_files())
            .sum();
        let unhealthy_services = self
            .service_contexts
            .iter()
            .filter(|service| service.health.status == ServiceHealthStatus::Unhealthy)
            .count();
        let degraded_services = self
            .service_contexts
            .iter()
            .filter(|service| service.health.status == ServiceHealthStatus::Degraded)
            .count();
        let closed_ports = self
            .service_contexts
            .iter()
            .flat_map(|service| service.ports.iter())
            .filter(|port| !port.open)
            .count();

        AgentImpactSummary {
            dirty_repos,
            dirty_files,
            unhealthy_services,
            degraded_services,
            closed_ports,
            severity: impact_severity(
                dirty_files,
                unhealthy_services,
                degraded_services,
                closed_ports,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentImpactSummary {
    pub dirty_repos: usize,
    pub dirty_files: u32,
    pub unhealthy_services: usize,
    pub degraded_services: usize,
    pub closed_ports: usize,
    pub severity: ImpactSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactSeverity {
    None,
    Info,
    Warning,
    Critical,
}

impl AgentSessionSummary {
    pub fn attention_rank(&self) -> u8 {
        match (self.unread, self.status, self.attention) {
            (true, AgentStatusKind::NeedsApproval | AgentStatusKind::NeedsInput, _) => 0,
            (_, AgentStatusKind::Failed | AgentStatusKind::Blocked, _) => 1,
            (false, AgentStatusKind::NeedsApproval | AgentStatusKind::NeedsInput, _) => 2,
            (
                _,
                AgentStatusKind::Streaming
                | AgentStatusKind::ToolRunning
                | AgentStatusKind::Running,
                _,
            ) => 3,
            (
                _,
                AgentStatusKind::Idle | AgentStatusKind::Starting | AgentStatusKind::Unknown,
                AttentionLevel::Error,
            ) => 1,
            (
                _,
                AgentStatusKind::Idle | AgentStatusKind::Starting | AgentStatusKind::Unknown,
                _,
            ) => 4,
            (_, AgentStatusKind::Done | AgentStatusKind::Exited, _) => 5,
        }
    }
}

pub fn sort_by_attention(sessions: &mut [AgentSessionSummary]) {
    sessions.sort_by(|left, right| {
        left.attention_rank()
            .cmp(&right.attention_rank())
            .then_with(|| left.session_name.cmp(&right.session_name))
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub fn compare_by_attention(left: &AgentSessionSummary, right: &AgentSessionSummary) -> Ordering {
    left.attention_rank()
        .cmp(&right.attention_rank())
        .then_with(|| left.session_name.cmp(&right.session_name))
        .then_with(|| left.id.cmp(&right.id))
}

fn repo_matches_context(
    repo: &RepoContextSummary,
    worktree_set_id: Option<&str>,
    context_link: Option<&AgentContextLink>,
    project: &ProjectContextSummary,
) -> bool {
    if !matches_worktree_set(repo.worktree_set_id.as_deref(), worktree_set_id, false) {
        return false;
    }

    let Some(context_link) = context_link else {
        return false;
    };

    if context_link
        .repo_ids
        .iter()
        .any(|repo_id| repo_id == &repo.repo_id)
    {
        return true;
    }

    if context_link.service_ids.is_empty() {
        return false;
    }

    project.services.iter().any(|service| {
        service
            .repo_id
            .as_deref()
            .is_some_and(|repo_id| repo_id == repo.repo_id)
            && service_matches_context(service, worktree_set_id, Some(context_link))
    })
}

fn service_matches_context(
    service: &ServiceContextSummary,
    worktree_set_id: Option<&str>,
    context_link: Option<&AgentContextLink>,
) -> bool {
    if !matches_worktree_set(
        service.worktree_set_id.as_deref(),
        worktree_set_id,
        service.shared,
    ) {
        return false;
    }

    let Some(context_link) = context_link else {
        return false;
    };

    if context_link
        .service_ids
        .iter()
        .any(|service_id| service_id == &service.service_id)
        || service.instance_id.as_ref().is_some_and(|instance_id| {
            context_link
                .service_ids
                .iter()
                .any(|service_id| service_id == instance_id)
        })
    {
        return true;
    }

    service.repo_id.as_ref().is_some_and(|repo_id| {
        context_link
            .repo_ids
            .iter()
            .any(|context_repo_id| context_repo_id == repo_id)
    })
}

fn matches_worktree_set(
    item_worktree_set_id: Option<&str>,
    selected_worktree_set_id: Option<&str>,
    shared: bool,
) -> bool {
    shared || selected_worktree_set_id.is_none() || item_worktree_set_id == selected_worktree_set_id
}

fn impact_severity(
    dirty_files: u32,
    unhealthy_services: usize,
    degraded_services: usize,
    closed_ports: usize,
) -> ImpactSeverity {
    if unhealthy_services > 0 || closed_ports > 0 {
        ImpactSeverity::Critical
    } else if dirty_files > 0 || degraded_services > 0 {
        ImpactSeverity::Warning
    } else {
        ImpactSeverity::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, status: AgentStatusKind, unread: bool) -> AgentSessionSummary {
        AgentSessionSummary {
            id: id.to_owned(),
            agent_kind: "codex".to_owned(),
            session_name: id.to_owned(),
            status,
            attention: AttentionLevel::None,
            tracking: TrackingKind::Tracked,
            unread,
            last_message: None,
            source_window_id: None,
            cwd: None,
            is_self: false,
            is_active: false,
            status_source: StatusSource::Manual,
            status_confidence_percent: 100,
            status_message: None,
        }
    }

    #[test]
    fn agent_status_tracks_confidence() {
        let status = AgentStatus::uncertain(
            AgentStatusKind::NeedsInput,
            StatusSource::TerminalHeuristic,
            0.6,
        );

        assert!(!status.is_confident());
        assert_eq!(status.kind, AgentStatusKind::NeedsInput);
        assert_eq!(status.source, StatusSource::TerminalHeuristic);
    }

    #[test]
    fn attention_sort_prioritizes_user_action() {
        let mut sessions = vec![
            session("done", AgentStatusKind::Done, false),
            session("running", AgentStatusKind::Running, false),
            session("approval", AgentStatusKind::NeedsApproval, true),
            session("failed", AgentStatusKind::Failed, false),
        ];

        sort_by_attention(&mut sessions);

        let ids: Vec<_> = sessions.iter().map(|session| session.id.as_str()).collect();
        assert_eq!(ids, vec!["approval", "failed", "running", "done"]);
    }

    #[test]
    fn selected_context_reports_dirty_repo_impact() {
        let snapshot = lens_snapshot(
            RepoDirtySummary {
                files: 4,
                staged: 1,
                unstaged: 2,
                untracked: 1,
            },
            ServiceHealthStatus::Healthy,
            true,
        );

        let context = snapshot
            .selected_agent_context("session-1")
            .expect("session context");
        let impact = context.impact_summary();

        assert_eq!(context.repo_contexts.len(), 1);
        assert_eq!(context.repo_contexts[0].repo_id, "app");
        assert_eq!(impact.dirty_repos, 1);
        assert_eq!(impact.dirty_files, 4);
        assert_eq!(impact.severity, ImpactSeverity::Warning);
    }

    #[test]
    fn selected_context_reports_service_health_impact() {
        let snapshot = lens_snapshot(
            RepoDirtySummary::default(),
            ServiceHealthStatus::Unhealthy,
            false,
        );

        let context = snapshot
            .selected_agent_context("session-1")
            .expect("session context");
        let impact = context.impact_summary();

        assert_eq!(context.service_contexts.len(), 1);
        assert_eq!(context.service_contexts[0].service_id, "web");
        assert_eq!(
            context.service_contexts[0].health.status,
            ServiceHealthStatus::Unhealthy
        );
        assert_eq!(impact.unhealthy_services, 1);
        assert_eq!(impact.closed_ports, 1);
        assert_eq!(impact.severity, ImpactSeverity::Critical);
    }

    fn lens_snapshot(
        dirty: RepoDirtySummary,
        health_status: ServiceHealthStatus,
        port_open: bool,
    ) -> AgentLensSnapshot {
        AgentLensSnapshot {
            project: ProjectContextSummary {
                name: "kagent".to_owned(),
                root: "/workspace/kagent".to_owned(),
                active_worktree_set_id: Some("main".to_owned()),
                worktree_sets: vec![WorktreeSetSummary {
                    id: "main".to_owned(),
                    active: true,
                    worktrees: vec![WorktreeSummary {
                        id: "app-main".to_owned(),
                        repo_id: "app".to_owned(),
                        worktree_set_id: "main".to_owned(),
                        path: "/workspace/kagent".to_owned(),
                        branch: Some("main".to_owned()),
                        head: Some("abc123".to_owned()),
                        exists: true,
                    }],
                }],
                repos: vec![RepoContextSummary {
                    repo_id: "app".to_owned(),
                    worktree_id: Some("app-main".to_owned()),
                    worktree_set_id: Some("main".to_owned()),
                    path: "/workspace/kagent".to_owned(),
                    default_branch: Some("main".to_owned()),
                    branch: Some("main".to_owned()),
                    head: Some("abc123".to_owned()),
                    exists: true,
                    service_ids: vec!["web".to_owned()],
                    dirty,
                }],
                services: vec![ServiceContextSummary {
                    service_id: "web".to_owned(),
                    instance_id: Some("web-main".to_owned()),
                    repo_id: Some("app".to_owned()),
                    worktree_set_id: Some("main".to_owned()),
                    service_type: "process".to_owned(),
                    shared: false,
                    status: "running".to_owned(),
                    health: ServiceHealthSummary {
                        status: health_status,
                        checked_at: Some("2026-05-07T04:00:00+09:00".to_owned()),
                        url: Some("http://127.0.0.1:3000".to_owned()),
                        last_error: if health_status == ServiceHealthStatus::Unhealthy {
                            Some("connection refused".to_owned())
                        } else {
                            None
                        },
                    },
                    ports: vec![ServicePortSummary {
                        name: "http".to_owned(),
                        base: 3000,
                        actual: 3000,
                        url: Some("http://127.0.0.1:3000".to_owned()),
                        open: port_open,
                    }],
                }],
            },
            sessions: vec![session("session-1", AgentStatusKind::Running, false)],
            agent_contexts: vec![AgentContextLink {
                session_id: "session-1".to_owned(),
                worktree_set_id: Some("main".to_owned()),
                repo_ids: vec!["app".to_owned()],
                service_ids: vec!["web".to_owned()],
            }],
        }
    }
}
