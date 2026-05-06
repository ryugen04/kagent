use kagent_core::AgentStatusKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEvent {
    pub session_id: String,
    pub status: AgentStatusKind,
    pub message: Option<String>,
}

pub trait AgentAdapter {
    fn detect_kind(&self, cmdline: &[String]) -> bool;
    fn classify_text(&self, text: &str) -> Vec<AgentEvent>;
}
