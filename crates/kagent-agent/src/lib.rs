use kagent_core::AgentStatusKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    Codex,
    Claude,
    Generic,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredAgentSession {
    pub session_id: String,
    pub kind: AgentKind,
    pub status: AgentStatusKind,
    pub last_message: Option<String>,
}

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

pub fn infer_agent_session(
    session_id: &str,
    title: &str,
    cmdline: &[String],
    screen_text: &str,
) -> InferredAgentSession {
    InferredAgentSession {
        session_id: session_id.to_string(),
        kind: infer_agent_kind(title, cmdline),
        status: infer_status(screen_text),
        last_message: last_message(screen_text),
    }
}

pub fn infer_agent_kind(title: &str, cmdline: &[String]) -> AgentKind {
    if cmdline_matches(cmdline, "codex") || title_matches(title, "codex") {
        AgentKind::Codex
    } else if cmdline_matches(cmdline, "claude") || title_matches(title, "claude") {
        AgentKind::Claude
    } else {
        AgentKind::Generic
    }
}

pub fn infer_status(screen_text: &str) -> AgentStatusKind {
    let normalized = normalize(screen_text);

    if contains_any(
        &normalized,
        &[
            "needs approval",
            "approval required",
            "requires approval",
            "waiting for approval",
            "do you want to allow",
            "allow this command",
            "approve this command",
            "permission to run",
        ],
    ) {
        AgentStatusKind::NeedsApproval
    } else if contains_any(
        &normalized,
        &[
            "needs input",
            "waiting for input",
            "enter your response",
            "press enter to continue",
            "continue? yes/no",
            "continue? y/n",
            "reply with",
        ],
    ) {
        AgentStatusKind::NeedsInput
    } else if contains_any(&normalized, &["failed", "error:", "panic"]) {
        AgentStatusKind::Failed
    } else if contains_any(&normalized, &["done", "completed"]) {
        AgentStatusKind::Done
    } else {
        AgentStatusKind::Unknown
    }
}

pub fn last_message(screen_text: &str) -> Option<String> {
    screen_text
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn cmdline_matches(cmdline: &[String], needle: &str) -> bool {
    cmdline.iter().any(|arg| {
        let command = arg.rsplit('/').next().unwrap_or(arg);
        command == needle || command.starts_with(&format!("{needle}-"))
    })
}

fn title_matches(title: &str, needle: &str) -> bool {
    title
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case(needle))
}

fn normalize(text: &str) -> String {
    text.to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_codex_approval_prompt() {
        let session = infer_agent_session(
            "kitty-7",
            "project",
            &["/usr/bin/codex".to_string(), "exec".to_string()],
            "Review command\nDo you want to allow this command?\n",
        );

        assert_eq!(session.kind, AgentKind::Codex);
        assert_eq!(session.status, AgentStatusKind::NeedsApproval);
        assert_eq!(
            session.last_message,
            Some("Do you want to allow this command?".to_string())
        );
    }

    #[test]
    fn infers_claude_needs_input_prompt() {
        let session = infer_agent_session(
            "kitty-9",
            "claude main",
            &["claude".to_string()],
            "Task paused\nEnter your response to continue\n",
        );

        assert_eq!(session.kind, AgentKind::Claude);
        assert_eq!(session.status, AgentStatusKind::NeedsInput);
        assert_eq!(
            session.last_message,
            Some("Enter your response to continue".to_string())
        );
    }

    #[test]
    fn falls_back_to_generic_unknown() {
        let session = infer_agent_session("kitty-11", "shell", &["bash".to_string()], "working");

        assert_eq!(session.kind, AgentKind::Generic);
        assert_eq!(session.status, AgentStatusKind::Unknown);
        assert_eq!(session.last_message, Some("working".to_string()));
    }
}
