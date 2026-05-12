use std::fmt::Write as _;

use kagent_core::{
    AgentContextLink, AgentLensSnapshot, AgentSessionSummary, AgentStatusKind, AttentionLevel,
    ImpactSeverity, ProjectContextSummary, SelectedAgentContext, ServiceHealthStatus, StatusSource,
    TrackingKind, sort_by_attention,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Buffer, Color, Line, Modifier, Span, Style, Stylize, Widget},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

const AGENT_TEXT_MAX_CHARS: usize = 48;
const PREVIEW_PAGE_LINES: usize = 16;
const PREVIEW_HALF_PAGE_LINES: usize = 8;

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

impl PreviewMode {
    pub fn next(&self) -> Self {
        match self {
            Self::Transcript => Self::RawTerminal,
            Self::RawTerminal => Self::LastScreen,
            Self::LastScreen => Self::Events,
            Self::Events => Self::Summary,
            Self::Summary => Self::Transcript,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneFocus {
    Agents,
    Preview,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLensTabView {
    pub id: String,
    pub title: String,
    pub is_active: bool,
    pub windows: Vec<AgentLensWindowView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLensWindowView {
    pub session_id: String,
    pub window_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub kind: String,
    pub is_active: bool,
    pub is_self: bool,
    pub foreground_cmdline: Vec<String>,
    pub repos: Vec<AgentLensRepoView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLensRepoView {
    pub id: String,
    pub path: String,
    pub branch: Option<String>,
    pub dirty_files: usize,
    pub pr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLensViewModel {
    pub snapshot: AgentLensSnapshot,
    pub live_tabs: Vec<AgentLensTabView>,
    session_order: Vec<String>,
    pub selected_index: usize,
    pub preview_mode: PreviewMode,
    pub preview_scroll: usize,
    pub focus: PaneFocus,
    pub context_size: PaneSize,
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
        let session_order = snapshot
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect();
        Self {
            snapshot,
            live_tabs: Vec::new(),
            session_order,
            selected_index: 0,
            preview_mode: PreviewMode::Transcript,
            preview_scroll: 0,
            focus: PaneFocus::Agents,
            context_size: PaneSize::Normal,
        }
    }

    pub fn from_live_snapshot(
        snapshot: AgentLensSnapshot,
        live_tabs: Vec<AgentLensTabView>,
    ) -> Self {
        let session_order = live_tabs
            .iter()
            .flat_map(|tab| {
                let mut windows = tab.windows.iter().collect::<Vec<_>>();
                windows.sort_by_key(|window| window_kind_rank(&window.kind));
                windows
            })
            .map(|window| window.session_id.clone())
            .collect();
        Self {
            snapshot,
            live_tabs,
            session_order,
            selected_index: 0,
            preview_mode: PreviewMode::Transcript,
            preview_scroll: 0,
            focus: PaneFocus::Agents,
            context_size: PaneSize::Normal,
        }
    }

    pub fn selected(&self) -> Option<&AgentSessionSummary> {
        let selected_id = self.session_order.get(self.selected_index)?;
        self.snapshot
            .sessions
            .iter()
            .find(|session| &session.id == selected_id)
    }

    pub fn selected_context(&self) -> Option<SelectedAgentContext<'_>> {
        let selected_id = self.session_order.get(self.selected_index)?;
        self.snapshot.selected_agent_context(selected_id)
    }

    pub fn select_next(&mut self) {
        if self.session_order.is_empty() {
            self.selected_index = 0;
        } else {
            let next_index = (self.selected_index + 1).min(self.session_order.len() - 1);
            if next_index != self.selected_index {
                self.preview_scroll = 0;
            }
            self.selected_index = next_index;
        }
    }

    pub fn select_previous(&mut self) {
        let next_index = self.selected_index.saturating_sub(1);
        if next_index != self.selected_index {
            self.preview_scroll = 0;
        }
        self.selected_index = next_index;
    }

    pub fn cycle_preview_mode(&mut self) {
        self.preview_mode = self.preview_mode.next();
        self.preview_scroll = 0;
    }

    pub fn focus_next_pane(&mut self) {
        self.focus = match self.focus {
            PaneFocus::Agents => PaneFocus::Preview,
            PaneFocus::Preview => PaneFocus::Context,
            PaneFocus::Context => PaneFocus::Agents,
        };
    }

    pub fn scroll_preview_page_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(PREVIEW_PAGE_LINES);
    }

    pub fn scroll_preview_page_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(PREVIEW_PAGE_LINES);
    }

    pub fn scroll_preview_half_page_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(PREVIEW_HALF_PAGE_LINES);
    }

    pub fn scroll_preview_half_page_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(PREVIEW_HALF_PAGE_LINES);
    }

    pub fn scroll_preview_top(&mut self) {
        self.preview_scroll = 0;
    }

    pub fn scroll_preview_bottom(&mut self) {
        self.preview_scroll = usize::MAX;
    }
}

pub struct AgentLensWidget<'a> {
    model: &'a AgentLensViewModel,
}

impl<'a> AgentLensWidget<'a> {
    pub fn new(model: &'a AgentLensViewModel) -> Self {
        Self { model }
    }
}

impl Widget for AgentLensWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let chunks = agent_lens_layout(vertical[0], self.model.context_size);
        render_agents_widget(self.model, chunks[0], buf);
        render_preview_widget(self.model, chunks[1], buf);
        if let Some(context_area) = chunks.get(2).copied() {
            render_context_widget(self.model, context_area, buf);
        }
        render_footer_widget(self.model, vertical[1], buf);
    }
}

fn agent_lens_layout(area: Rect, context_size: PaneSize) -> Vec<Rect> {
    if area.width >= 120 && context_size != PaneSize::Hidden {
        let context_width = match context_size {
            PaneSize::Compact => 24,
            PaneSize::Normal => 32,
            PaneSize::Expanded | PaneSize::Maximized => 42,
            PaneSize::Hidden => 0,
        };

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(44),
                Constraint::Min(40),
                Constraint::Length(context_width),
            ])
            .split(area)
            .to_vec()
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(40), Constraint::Min(40)])
            .split(area)
            .to_vec()
    }
}

fn render_agents_widget(model: &AgentLensViewModel, area: Rect, buf: &mut Buffer) {
    if !model.live_tabs.is_empty() {
        render_live_tabs_widget(model, area, buf);
        return;
    }

    let items = if model.snapshot.sessions.is_empty() {
        vec![ListItem::new(Line::from("(no agents)"))]
    } else {
        model
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .map(|(index, session)| {
                let selected = index == model.selected_index;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if session.attention == AttentionLevel::Error {
                    Style::default().fg(Color::Red)
                } else if session.attention == AttentionLevel::NeedsUser {
                    Style::default().fg(Color::Yellow)
                } else if matches!(
                    session.status,
                    AgentStatusKind::Done | AgentStatusKind::Exited
                ) {
                    Style::default().fg(Color::DarkGray)
                } else if session.agent_kind == "generic" {
                    Style::default().fg(Color::DarkGray)
                } else if matches!(
                    session.status,
                    AgentStatusKind::Running
                        | AgentStatusKind::Streaming
                        | AgentStatusKind::ToolRunning
                ) {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                let icon = status_icon(session);
                let badge_style = if session.agent_kind == "codex" {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let window_marker = format_window_marker(session);
                ListItem::new(vec![
                    Line::from(vec![
                        Span::raw(icon),
                        Span::raw(" "),
                        Span::styled(
                            session.session_name.clone(),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(format!("({})", session.agent_kind), badge_style),
                        Span::raw(" "),
                        Span::styled(window_marker, Style::default().fg(Color::Cyan)),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            compact_status_label(session),
                            status_style(session.status, session.attention),
                        ),
                        Span::raw(" "),
                        Span::raw(format_context_hint(session)),
                        Span::raw(" "),
                        Span::raw(format_status_confidence(session)),
                    ]),
                ])
                .style(style)
            })
            .collect()
    };

    let block = pane_block("Agents / Tabs", model.focus == PaneFocus::Agents);
    List::new(items).block(block).render(area, buf);
}

fn render_live_tabs_widget(model: &AgentLensViewModel, area: Rect, buf: &mut Buffer) {
    let selected_id = model.session_order.get(model.selected_index);
    let mut items = Vec::new();

    for tab in &model.live_tabs {
        let tab_style = if tab.is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD)
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled("▾ ", tab_style),
            Span::styled(tab.title.clone(), tab_style),
            Span::raw("  "),
            Span::raw(tab_agent_status(tab, model)),
            Span::styled(
                format!("  tab {}  {} win", tab.id, tab.windows.len()),
                tab_style,
            ),
        ])));

        let mut windows = tab.windows.clone();
        windows.sort_by_key(|window| window_kind_rank(&window.kind));
        for window in &windows {
            let selected = selected_id == Some(&window.session_id);
            let session = model
                .snapshot
                .sessions
                .iter()
                .find(|session| session.id == window.session_id);
            let row_style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                window_kind_style(&window.kind)
            };
            let marker = if selected { ">" } else { " " };
            let flags = format_live_window_flags(window);
            let mut lines = vec![Line::from(vec![
                Span::raw(marker),
                Span::raw(" "),
                Span::styled(
                    window_kind_icon(&window.kind),
                    window_kind_style(&window.kind),
                ),
                Span::raw(" #"),
                Span::raw(window.window_id.clone()),
                Span::raw(" "),
                Span::styled(window.kind.clone(), window_kind_style(&window.kind)),
                Span::raw(" "),
                Span::raw(flags),
                Span::raw(" "),
                Span::styled(
                    window.title.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ])];
            lines.extend(window_repo_lines(window, session));
            items.push(ListItem::new(lines).style(row_style));
        }
    }

    let block = pane_block("Agents / Kitty Tabs", model.focus == PaneFocus::Agents);
    List::new(items).block(block).render(area, buf);
}

fn render_preview_widget(model: &AgentLensViewModel, area: Rect, buf: &mut Buffer) {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Project: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(model.snapshot.project.name.clone()),
        Span::raw("  "),
        Span::raw(model.snapshot.project.root.clone()),
    ]));

    if let Some(context) = model.selected_context() {
        if let Some((tab, window)) = selected_live_window(model) {
            lines.extend(live_window_fact_lines(tab, window, context.session));
        }
        lines.push(Line::from(vec![
            Span::styled("Selected: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(context.session.session_name.clone()),
            Span::raw(format!(
                "  mode={}",
                format_preview_mode(&model.preview_mode)
            )),
        ]));
        lines.extend(preview_lines(&context, &model.preview_mode));
    } else {
        lines.push(Line::from("Selected: -"));
    }

    let body_height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(body_height.max(1));
    let scroll = model.preview_scroll.min(max_scroll);
    let title = format!(
        "Conversation Preview {}",
        preview_position_label(scroll, lines.len())
    );

    Paragraph::new(lines)
        .block(dynamic_pane_block(
            &title,
            model.focus == PaneFocus::Preview,
        ))
        .scroll((scroll.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

fn selected_live_window(
    model: &AgentLensViewModel,
) -> Option<(&AgentLensTabView, &AgentLensWindowView)> {
    let selected_id = model.session_order.get(model.selected_index)?;
    model.live_tabs.iter().find_map(|tab| {
        tab.windows
            .iter()
            .find(|window| &window.session_id == selected_id)
            .map(|window| (tab, window))
    })
}

fn live_window_fact_lines(
    tab: &AgentLensTabView,
    window: &AgentLensWindowView,
    session: &AgentSessionSummary,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Tab: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}  {}", tab.id, tab.title)),
        ]),
        Line::from(vec![
            Span::styled("Window: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("#{}  {}", window.window_id, window.title)),
        ]),
        Line::from(vec![
            Span::styled("Kind: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(window.kind.clone()),
            Span::raw("  "),
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                "{} {}% {}",
                format_status(session.status),
                session.status_confidence_percent,
                session.status_message.as_deref().unwrap_or("-")
            )),
        ]),
        Line::from(vec![
            Span::styled("Cwd: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(window.cwd.clone().unwrap_or_else(|| "-".to_owned())),
        ]),
        Line::from(vec![
            Span::styled("Cmd: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format_cmdline(&window.foreground_cmdline)),
        ]),
    ];

    if !window.repos.is_empty() {
        lines.push(Line::from("Repos:").bold());
        for repo in &window.repos {
            lines.push(Line::from(format!("  {}", format_repo_summary(repo))));
        }
    }

    lines.push(Line::from(""));
    lines
}

fn render_context_widget(model: &AgentLensViewModel, area: Rect, buf: &mut Buffer) {
    let mut lines = Vec::new();

    if let Some(context) = model.selected_context() {
        let impact = context.impact_summary();
        lines.push(Line::from(vec![
            Span::styled("Severity: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format_impact_severity(impact.severity),
                severity_style(impact.severity),
            ),
        ]));
        lines.push(Line::from(format!("Dirty repos: {}", impact.dirty_repos)));
        lines.push(Line::from(format!("Dirty files: {}", impact.dirty_files)));
        lines.push(Line::from(format!(
            "Service issues: unhealthy={} degraded={} closed_ports={}",
            impact.unhealthy_services, impact.degraded_services, impact.closed_ports
        )));
        lines.push(Line::from(""));
        lines.push(Line::from("Repos").bold());
        for repo in &context.repo_contexts {
            lines.push(Line::from(format!(
                "{} {} {}",
                repo.repo_id,
                repo.branch.as_deref().unwrap_or("-"),
                format_dirty(&repo.dirty)
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("Services").bold());
        for service in &context.service_contexts {
            lines.push(Line::from(format!(
                "{} {} {}",
                service.service_id,
                service.status,
                format_health(service.health.status)
            )));
        }
    } else {
        lines.push(Line::from("Severity: none"));
    }

    Paragraph::new(lines)
        .block(pane_block(
            "Context / Impact",
            model.focus == PaneFocus::Context,
        ))
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

fn preview_lines(context: &SelectedAgentContext<'_>, mode: &PreviewMode) -> Vec<Line<'static>> {
    match mode {
        PreviewMode::Transcript => preview_text_lines(
            "Transcript",
            context.session.last_message.as_deref().unwrap_or("-"),
        ),
        PreviewMode::RawTerminal => vec![Line::from("Raw Terminal"), Line::from("-")],
        PreviewMode::LastScreen => preview_text_lines(
            "Last Screen",
            context.session.last_message.as_deref().unwrap_or("-"),
        ),
        PreviewMode::Events => vec![
            Line::from("Events"),
            Line::from(format!(
                "status={} attention={} unread={}",
                format_status(context.session.status),
                format_attention(context.session.attention),
                context.session.unread
            )),
        ],
        PreviewMode::Summary => {
            let impact = context.impact_summary();
            vec![
                Line::from("Summary"),
                Line::from(format!("Session: {}", context.session.session_name)),
                Line::from(format!("Status: {}", format_status(context.session.status))),
                Line::from(format!("Dirty files: {}", impact.dirty_files)),
                Line::from(format!("Services: {}", context.service_contexts.len())),
            ]
        }
    }
}

fn preview_text_lines(label: &'static str, text: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(label)];
    let normalized = normalize_terminal_text(text);
    let body = normalized.lines().collect::<Vec<_>>();

    if body.is_empty() {
        lines.push(Line::from("-"));
        return lines;
    }

    lines.extend(body.into_iter().map(|line| Line::from(line.to_owned())));
    lines
}

fn normalize_terminal_text(text: &str) -> String {
    strip_ansi_sequences(&text.replace("\\n", "\n"))
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_ansi_sequences(text: &str) -> String {
    let mut output = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            output.push(ch);
            continue;
        }

        if chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }

    output
}

fn pane_block(title: &'static str, focused: bool) -> Block<'static> {
    dynamic_pane_block(title, focused)
}

fn dynamic_pane_block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    };

    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style)
}

fn render_footer_widget(model: &AgentLensViewModel, area: Rect, buf: &mut Buffer) {
    let line = Line::from(vec![
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Gray)),
        Span::raw(" Quit "),
        Span::styled(" j/k ", Style::default().fg(Color::Black).bg(Color::Gray)),
        Span::raw(" Move "),
        Span::styled(" Tab ", Style::default().fg(Color::Black).bg(Color::Gray)),
        Span::raw(" Preview "),
        Span::styled(
            " PgUp/PgDn ",
            Style::default().fg(Color::Black).bg(Color::Gray),
        ),
        Span::raw(" Scroll "),
        Span::styled(" g/G ", Style::default().fg(Color::Black).bg(Color::Gray)),
        Span::raw(" Top/Bottom "),
        Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Gray)),
        Span::raw(" Focus "),
        Span::raw(format!(
            " | {} | {}",
            format_preview_mode(&model.preview_mode),
            model
                .selected()
                .map(|session| session.session_name.as_str())
                .unwrap_or("-")
        )),
    ]);

    Paragraph::new(line)
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .render(area, buf);
}

fn status_icon(session: &AgentSessionSummary) -> &'static str {
    match (session.unread, session.status, session.tracking) {
        (true, AgentStatusKind::NeedsInput | AgentStatusKind::NeedsApproval, _) => "!",
        (false, AgentStatusKind::NeedsInput | AgentStatusKind::NeedsApproval, _) => "^",
        (
            _,
            AgentStatusKind::Streaming | AgentStatusKind::ToolRunning | AgentStatusKind::Running,
            _,
        ) => "o",
        (_, AgentStatusKind::Done, _) => "v",
        (_, AgentStatusKind::Failed | AgentStatusKind::Blocked, _) => "x",
        (_, _, TrackingKind::Inferred) => "?",
        _ => "-",
    }
}

fn compact_status_label(session: &AgentSessionSummary) -> String {
    let source = format_status_source(session.status_source);
    match session.status_message.as_deref() {
        Some(message) if !message.is_empty() => format!(
            "{} {}",
            format_status(session.status),
            truncate_chars(message, 24)
        ),
        _ => format!("{} {}", format_status(session.status), source),
    }
}

fn format_window_marker(session: &AgentSessionSummary) -> String {
    let window = session
        .source_window_id
        .as_deref()
        .map(|id| format!("#{id}"))
        .unwrap_or_else(|| "#-".to_owned());
    let mut markers = Vec::new();
    if session.is_self {
        markers.push("self");
    }
    if session.is_active {
        markers.push("active");
    }

    if markers.is_empty() {
        window
    } else {
        format!("{window} {}", markers.join(","))
    }
}

fn format_context_hint(session: &AgentSessionSummary) -> String {
    format_context_hint_for_cwd(session.cwd.as_deref())
}

fn format_context_hint_for_cwd(cwd: Option<&str>) -> String {
    cwd.and_then(cwd_basename)
        .map(|cwd| format!("@{cwd}"))
        .unwrap_or_else(|| "@-".to_owned())
}

fn cwd_basename(cwd: &str) -> Option<&str> {
    let without_scheme = cwd.strip_prefix("file://").unwrap_or(cwd);
    without_scheme
        .trim_end_matches('/')
        .rsplit('/')
        .find(|segment| !segment.is_empty())
}

fn format_status_confidence(session: &AgentSessionSummary) -> String {
    format!(
        "{}% {}",
        session.status_confidence_percent,
        format_tracking(session.tracking)
    )
}

fn format_cmdline(cmdline: &[String]) -> String {
    if cmdline.is_empty() {
        "-".to_owned()
    } else {
        truncate_chars(&cmdline.join(" "), 96)
    }
}

fn window_repo_lines(
    window: &AgentLensWindowView,
    session: Option<&AgentSessionSummary>,
) -> Vec<Line<'static>> {
    match (session, window.repos.is_empty()) {
        (Some(session), false) => window
            .repos
            .iter()
            .map(|repo| {
                Line::from(vec![
                    Span::raw("      "),
                    Span::styled(
                        status_glyph(session.status),
                        status_style(session.status, session.attention),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        short_status_label(session.status),
                        status_style(session.status, session.attention),
                    ),
                    Span::raw("  "),
                    Span::raw(format_repo_summary(repo)),
                ])
            })
            .collect(),
        (Some(session), true) => vec![Line::from(vec![
            Span::raw("      "),
            Span::styled(
                status_glyph(session.status),
                status_style(session.status, session.attention),
            ),
            Span::raw(" "),
            Span::styled(
                short_status_label(session.status),
                status_style(session.status, session.attention),
            ),
            Span::raw("  "),
            Span::raw(format_context_hint_for_cwd(window.cwd.as_deref())),
        ])],
        (None, _) => vec![Line::from("      ? unknown")],
    }
}

fn format_repo_summary(repo: &AgentLensRepoView) -> String {
    let branch = repo.branch.as_deref().unwrap_or("-");
    let dirty = if repo.dirty_files == 0 {
        "clean".to_owned()
    } else {
        format!("dirty {}", repo.dirty_files)
    };
    let pr = repo
        .pr
        .as_deref()
        .map(|pr| format!("  PR {pr}"))
        .unwrap_or_default();

    format!("{}  {}  {}{}", repo.id, branch, dirty, pr)
}

fn tab_agent_status(tab: &AgentLensTabView, model: &AgentLensViewModel) -> String {
    let mut waiting = 0;
    let mut running = 0;
    let mut done = 0;
    let mut failed = 0;
    let mut codex = 0;

    for window in &tab.windows {
        let Some(session) = model
            .snapshot
            .sessions
            .iter()
            .find(|session| session.id == window.session_id)
        else {
            continue;
        };

        if session.agent_kind != "codex" {
            continue;
        }

        codex += 1;
        match session.status {
            AgentStatusKind::NeedsInput | AgentStatusKind::NeedsApproval => waiting += 1,
            AgentStatusKind::Failed | AgentStatusKind::Blocked => failed += 1,
            AgentStatusKind::Done | AgentStatusKind::Exited => done += 1,
            AgentStatusKind::Running
            | AgentStatusKind::Streaming
            | AgentStatusKind::ToolRunning
            | AgentStatusKind::Starting => running += 1,
            AgentStatusKind::Idle | AgentStatusKind::Unknown => {}
        }
    }

    if codex == 0 {
        return "tools".to_owned();
    }

    let mut parts = Vec::new();
    if waiting > 0 {
        parts.push(format!("🔔{waiting}"));
    }
    if failed > 0 {
        parts.push(format!("✕{failed}"));
    }
    if running > 0 {
        parts.push(format!("…{running}"));
    }
    if done > 0 {
        parts.push(format!("✓{done}"));
    }
    if parts.is_empty() {
        parts.push(format!("?{codex}"));
    }

    parts.join(" ")
}

fn status_glyph(status: AgentStatusKind) -> &'static str {
    match status {
        AgentStatusKind::NeedsInput | AgentStatusKind::NeedsApproval => "🔔",
        AgentStatusKind::Failed | AgentStatusKind::Blocked => "✕",
        AgentStatusKind::Done | AgentStatusKind::Exited => "✓",
        AgentStatusKind::Running
        | AgentStatusKind::Streaming
        | AgentStatusKind::ToolRunning
        | AgentStatusKind::Starting => "…",
        AgentStatusKind::Idle => "·",
        AgentStatusKind::Unknown => "?",
    }
}

fn short_status_label(status: AgentStatusKind) -> &'static str {
    match status {
        AgentStatusKind::NeedsInput | AgentStatusKind::NeedsApproval => "wait",
        AgentStatusKind::Failed | AgentStatusKind::Blocked => "fail",
        AgentStatusKind::Done | AgentStatusKind::Exited => "done",
        AgentStatusKind::Running
        | AgentStatusKind::Streaming
        | AgentStatusKind::ToolRunning
        | AgentStatusKind::Starting => "run",
        AgentStatusKind::Idle => "idle",
        AgentStatusKind::Unknown => "unknown",
    }
}

fn window_kind_rank(kind: &str) -> u8 {
    match kind {
        "codex" => 0,
        "tool" => 1,
        "editor" => 2,
        "shell" => 3,
        _ => 4,
    }
}

fn window_kind_icon(kind: &str) -> &'static str {
    match kind {
        "codex" => "●",
        "tool" => "◆",
        "editor" => "■",
        "shell" => "·",
        _ => "?",
    }
}

fn window_kind_style(kind: &str) -> Style {
    match kind {
        "codex" => Style::default().fg(Color::Cyan),
        "tool" => Style::default().fg(Color::Blue),
        "editor" => Style::default().fg(Color::Magenta),
        "shell" => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Gray),
    }
}

fn format_live_window_flags(window: &AgentLensWindowView) -> String {
    let mut flags = Vec::new();
    if window.is_self {
        flags.push("self");
    }
    if window.is_active {
        flags.push("active");
    }

    if flags.is_empty() {
        "".to_owned()
    } else {
        flags.join(",")
    }
}

fn status_style(status: AgentStatusKind, attention: AttentionLevel) -> Style {
    if attention == AttentionLevel::Error
        || matches!(status, AgentStatusKind::Failed | AgentStatusKind::Blocked)
    {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if attention == AttentionLevel::NeedsUser
        || matches!(
            status,
            AgentStatusKind::NeedsApproval | AgentStatusKind::NeedsInput
        )
    {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if matches!(
        status,
        AgentStatusKind::Running | AgentStatusKind::Streaming | AgentStatusKind::ToolRunning
    ) {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn preview_position_label(scroll: usize, line_count: usize) -> String {
    if line_count == 0 {
        return "0/0".to_owned();
    }

    format!(
        "{}/{}",
        scroll.saturating_add(1).min(line_count),
        line_count
    )
}

fn severity_style(severity: ImpactSeverity) -> Style {
    match severity {
        ImpactSeverity::None => Style::default().fg(Color::Green),
        ImpactSeverity::Info => Style::default().fg(Color::Blue),
        ImpactSeverity::Warning => Style::default().fg(Color::Yellow),
        ImpactSeverity::Critical => Style::default().fg(Color::Red),
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

    if !model.live_tabs.is_empty() {
        let selected_id = model.session_order.get(model.selected_index);
        for tab in &model.live_tabs {
            writeln!(
                output,
                "{} {} {} tab={} windows={}",
                if tab.is_active { "*" } else { " " },
                tab.title,
                tab_agent_status(tab, model),
                tab.id,
                tab.windows.len()
            )
            .expect("write to string");

            let mut windows = tab.windows.clone();
            windows.sort_by_key(|window| window_kind_rank(&window.kind));
            for window in &windows {
                let marker = if selected_id == Some(&window.session_id) {
                    ">"
                } else {
                    " "
                };
                let session = model
                    .snapshot
                    .sessions
                    .iter()
                    .find(|session| session.id == window.session_id);
                writeln!(
                    output,
                    "{marker}   #{} {} {} {}",
                    window.window_id,
                    window.kind,
                    format_live_window_flags(window),
                    window.title
                )
                .expect("write to string");
                if let Some(session) = session {
                    if window.repos.is_empty() {
                        writeln!(
                            output,
                            "      {} {} {}",
                            status_glyph(session.status),
                            short_status_label(session.status),
                            format_context_hint_for_cwd(window.cwd.as_deref())
                        )
                        .expect("write to string");
                    } else {
                        for repo in &window.repos {
                            writeln!(
                                output,
                                "      {} {} {}",
                                status_glyph(session.status),
                                short_status_label(session.status),
                                format_repo_summary(repo)
                            )
                            .expect("write to string");
                        }
                    }
                }
            }
        }
        return;
    }

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
        let last_message = session
            .last_message
            .as_deref()
            .map(format_agent_list_message)
            .unwrap_or_else(|| "-".to_owned());

        writeln!(
            output,
            "{marker} {name} ({kind}) window={window} status={status} attention={attention} tracking={tracking} confidence={confidence}% {unread}",
            name = session.session_name,
            kind = session.agent_kind,
            window = format_window_marker(session),
            status = format_status(session.status),
            attention = format_attention(session.attention),
            tracking = format_tracking(session.tracking),
            confidence = session.status_confidence_percent,
        )
        .expect("write to string");
        writeln!(output, "  last: {last_message}").expect("write to string");
        writeln!(
            output,
            "  cwd: {} status_source={} status_message={}",
            session.cwd.as_deref().unwrap_or("-"),
            format_status_source(session.status_source),
            session.status_message.as_deref().unwrap_or("-")
        )
        .expect("write to string");
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

    if let Some((tab, window)) = selected_live_window(model) {
        writeln!(output, "Tab: {} {}", tab.id, tab.title).expect("write to string");
        writeln!(output, "Window: #{} {}", window.window_id, window.title)
            .expect("write to string");
        writeln!(output, "Kind: {}", window.kind).expect("write to string");
        writeln!(output, "Cwd: {}", window.cwd.as_deref().unwrap_or("-")).expect("write to string");
        writeln!(
            output,
            "Cmd: {}",
            format_cmdline(&window.foreground_cmdline)
        )
        .expect("write to string");
        writeln!(
            output,
            "Status: {} confidence={} reason={}",
            format_status(context.session.status),
            context.session.status_confidence_percent,
            context.session.status_message.as_deref().unwrap_or("-")
        )
        .expect("write to string");
        if !window.repos.is_empty() {
            for repo in &window.repos {
                writeln!(output, "Live repo: {}", format_repo_summary(repo))
                    .expect("write to string");
            }
        }
        output.push_str("Preview:\n");
        for line in normalize_terminal_text(context.session.last_message.as_deref().unwrap_or("-"))
            .lines()
            .take(40)
        {
            writeln!(output, "  {line}").expect("write to string");
        }
    }

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

fn format_agent_list_message(message: &str) -> String {
    let normalized = normalize_terminal_text(message);
    let first_line = normalized
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("-");
    truncate_chars(first_line, AGENT_TEXT_MAX_CHARS)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
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

fn format_status_source(source: StatusSource) -> &'static str {
    match source {
        StatusSource::ExplicitEvent => "event",
        StatusSource::Adapter => "adapter",
        StatusSource::TerminalHeuristic => "terminal",
        StatusSource::ProcessState => "process",
        StatusSource::Manual => "manual",
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
        ServiceHealthStatus, ServiceHealthSummary, ServicePortSummary, StatusSource, TrackingKind,
        WorktreeSetSummary, WorktreeSummary,
    };
    use ratatui::layout::Rect;
    use ratatui::prelude::Buffer;

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
                source_window_id: None,
                cwd: None,
                is_self: false,
                is_active: false,
                status_source: StatusSource::Manual,
                status_confidence_percent: 100,
                status_message: None,
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
                source_window_id: None,
                cwd: None,
                is_self: false,
                is_active: false,
                status_source: StatusSource::Manual,
                status_confidence_percent: 100,
                status_message: None,
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
> worker-3 (codex) window=#21 self,active status=needs-approval attention=needs-user tracking=tracked confidence=82% unread
  last: Approve cargo test?
  cwd: /workspace/kagent status_source=terminal status_message=approval-related prompt in selected screen
  context: worktree=main repos=app services=web,db
  reviewer (claude) window=#9 status=done attention=none tracking=inferred confidence=100% read
  last: Looks stable
  cwd: /workspace/kagent status_source=manual status_message=fixture done
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

    #[test]
    fn view_model_navigation_stays_in_bounds_and_cycles_preview() {
        let mut model = AgentLensViewModel::from_snapshot(agent_lens_snapshot());

        assert_eq!(
            model.selected().map(|session| session.id.as_str()),
            Some("worker-3")
        );

        model.select_next();
        assert_eq!(
            model.selected().map(|session| session.id.as_str()),
            Some("reviewer")
        );

        model.select_next();
        assert_eq!(
            model.selected().map(|session| session.id.as_str()),
            Some("reviewer")
        );

        model.select_previous();
        assert_eq!(
            model.selected().map(|session| session.id.as_str()),
            Some("worker-3")
        );

        model.cycle_preview_mode();
        assert_eq!(model.preview_mode, PreviewMode::RawTerminal);
    }

    #[test]
    fn preview_scroll_controls_reset_on_selection_and_mode_change() {
        let mut model = AgentLensViewModel::from_snapshot(agent_lens_snapshot());

        model.scroll_preview_page_down();
        model.scroll_preview_half_page_down();
        assert!(model.preview_scroll > 0);

        model.scroll_preview_half_page_up();
        assert_eq!(model.preview_scroll, PREVIEW_PAGE_LINES);

        model.select_next();
        assert_eq!(model.preview_scroll, 0);

        model.scroll_preview_bottom();
        assert_eq!(model.preview_scroll, usize::MAX);

        model.cycle_preview_mode();
        assert_eq!(model.preview_scroll, 0);
    }

    #[test]
    fn ratatui_widget_renders_agent_lens_panes() {
        let model = AgentLensViewModel::from_snapshot(agent_lens_snapshot());
        let mut buffer = Buffer::empty(Rect::new(0, 0, 132, 18));

        AgentLensWidget::new(&model).render(buffer.area, &mut buffer);

        let rendered = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Agents / Tabs"));
        assert!(rendered.contains("Conversation Preview"));
        assert!(rendered.contains("Context / Impact"));
        assert!(rendered.contains("PgUp/PgDn"));
        assert!(rendered.contains("worker-3"));
        assert!(rendered.contains("#21 self,active"));
        assert!(rendered.contains("Severity:"));
    }

    #[test]
    fn live_model_renders_hierarchical_tabs_and_window_facts() {
        let model = AgentLensViewModel::from_live_snapshot(agent_lens_snapshot(), live_tabs());

        let rendered = render_agent_lens_text(&model);

        assert!(rendered.contains("dotfiles 🔔1 tab=1 windows=2"));
        assert!(rendered.contains("#21 codex self,active worker-3"));
        assert!(rendered.contains("🔔 wait app  feature/lens  dirty 2"));
        assert!(rendered.contains("Tab: 1 dotfiles"));
        assert!(rendered.contains("Window: #21 worker-3"));
        assert!(rendered.contains("Kind: codex"));
        assert!(rendered.contains("Cmd: node /usr/bin/codex"));
        assert!(rendered.contains("Live repo: app  feature/lens  dirty 2"));
    }

    #[test]
    fn live_selection_order_matches_rendered_window_order() {
        let mut tabs = live_tabs();
        tabs[0].windows.reverse();
        let mut model = AgentLensViewModel::from_live_snapshot(agent_lens_snapshot(), tabs);

        assert_eq!(
            model.selected().map(|session| session.id.as_str()),
            Some("worker-3")
        );

        model.select_next();
        assert_eq!(
            model.selected().map(|session| session.id.as_str()),
            Some("reviewer")
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
                    source_window_id: Some("9".to_owned()),
                    cwd: Some("/workspace/kagent".to_owned()),
                    is_self: false,
                    is_active: false,
                    status_source: StatusSource::Manual,
                    status_confidence_percent: 100,
                    status_message: Some("fixture done".to_owned()),
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
                    source_window_id: Some("21".to_owned()),
                    cwd: Some("/workspace/kagent".to_owned()),
                    is_self: true,
                    is_active: true,
                    status_source: StatusSource::TerminalHeuristic,
                    status_confidence_percent: 82,
                    status_message: Some("approval-related prompt in selected screen".to_owned()),
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

    fn live_tabs() -> Vec<AgentLensTabView> {
        vec![AgentLensTabView {
            id: "1".to_owned(),
            title: "dotfiles".to_owned(),
            is_active: true,
            windows: vec![
                AgentLensWindowView {
                    session_id: "worker-3".to_owned(),
                    window_id: "21".to_owned(),
                    title: "worker-3".to_owned(),
                    cwd: Some("/workspace/kagent".to_owned()),
                    kind: "codex".to_owned(),
                    is_active: true,
                    is_self: true,
                    foreground_cmdline: vec!["node".to_owned(), "/usr/bin/codex".to_owned()],
                    repos: vec![AgentLensRepoView {
                        id: "app".to_owned(),
                        path: "/workspace/kagent".to_owned(),
                        branch: Some("feature/lens".to_owned()),
                        dirty_files: 2,
                        pr: None,
                    }],
                },
                AgentLensWindowView {
                    session_id: "reviewer".to_owned(),
                    window_id: "22".to_owned(),
                    title: "lazygit".to_owned(),
                    cwd: Some("/workspace/kagent".to_owned()),
                    kind: "tool".to_owned(),
                    is_active: false,
                    is_self: false,
                    foreground_cmdline: vec!["lazygit".to_owned()],
                    repos: vec![AgentLensRepoView {
                        id: "app".to_owned(),
                        path: "/workspace/kagent".to_owned(),
                        branch: Some("feature/lens".to_owned()),
                        dirty_files: 0,
                        pr: Some("#12".to_owned()),
                    }],
                },
            ],
        }]
    }
}
