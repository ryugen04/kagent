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
        }
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
}
