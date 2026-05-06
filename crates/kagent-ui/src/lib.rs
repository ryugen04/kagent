use kagent_core::{AgentSessionSummary, sort_by_attention};

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
    pub sessions: Vec<AgentSessionSummary>,
    pub selected_index: usize,
    pub preview_mode: PreviewMode,
}

impl AgentLensViewModel {
    pub fn new(mut sessions: Vec<AgentSessionSummary>) -> Self {
        sort_by_attention(&mut sessions);
        Self {
            sessions,
            selected_index: 0,
            preview_mode: PreviewMode::Transcript,
        }
    }

    pub fn selected(&self) -> Option<&AgentSessionSummary> {
        self.sessions.get(self.selected_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kagent_core::{AgentStatusKind, AttentionLevel, TrackingKind};

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
}
