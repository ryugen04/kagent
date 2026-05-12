use kagent_kitty::{CommandKittyProvider, KittyProvider};
use kagent_lens::{
    LiveDashState, SANGO_SNAPSHOT_JSON, focus_selected_window, lens_snapshot_from_sango,
    live_dash_state, refresh_selected_preview, service_health_label,
};
use kagent_ui::{AgentLensViewModel, AgentLensWidget, render_agent_lens_text};
use std::io::{self, IsTerminal};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();

    if is_interactive_dash(&args) {
        if let Err(message) = run_dash_interactive() {
            eprintln!("{message}");
            std::process::exit(2);
        }
        return;
    }

    match command_output(&args) {
        Ok(output) => print!("{output}"),
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
        "quick-access" => quick_access_output(flags),
        "context" => context_output(flags),
        _ => Err(format!("unknown command: {command}")),
    }
}

fn dash_output(flags: &[String]) -> Result<String, String> {
    match flags {
        [] => Ok("kagent dash opens the live Agent Lens TUI\n".to_owned()),
        [flag] if flag == "--snapshot" => snapshot_output(),
        [flag, ..] => Err(format!("unknown dash flag: {flag}")),
    }
}

fn is_interactive_dash(args: &[String]) -> bool {
    args.is_empty() || matches!(args, [command] if command == "dash" || command == "quick-access")
}

fn quick_access_output(flags: &[String]) -> Result<String, String> {
    match flags {
        [] => Ok("kagent quick-access opens the live Agent Lens TUI\n".to_owned()),
        [flag, ..] => Err(format!("unknown quick-access flag: {flag}")),
    }
}

fn run_dash_interactive() -> Result<(), String> {
    let provider = CommandKittyProvider::default();
    let mut state = live_dash_state(&provider)?;

    if !io::stdout().is_terminal() {
        refresh_selected_preview(&provider, &mut state.model);
        print!("{}", render_agent_lens_text(&state.model));
        return Ok(());
    }

    enable_raw_mode().map_err(|error| error.to_string())?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|error| error.to_string())?;

    let result = run_terminal_loop(&provider, &mut state);

    disable_raw_mode().map_err(|error| error.to_string())?;
    execute!(io::stdout(), LeaveAlternateScreen).map_err(|error| error.to_string())?;

    result
}

fn run_terminal_loop(
    provider: &impl KittyProvider,
    state: &mut LiveDashState,
) -> Result<(), String> {
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|error| error.to_string())?;

    loop {
        refresh_selected_preview(provider, &mut state.model);
        terminal
            .draw(|frame| {
                frame.render_widget(AgentLensWidget::new(&state.model), frame.area());
            })
            .map_err(|error| error.to_string())?;

        if let Event::Key(key) = event::read().map_err(|error| error.to_string())? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('j') | KeyCode::Down => state.model.select_next(),
                KeyCode::Char('k') | KeyCode::Up => state.model.select_previous(),
                KeyCode::Tab => state.model.cycle_preview_mode(),
                KeyCode::Right | KeyCode::Char('l') => state.model.focus_next_pane(),
                KeyCode::PageDown => state.model.scroll_preview_page_down(),
                KeyCode::PageUp => state.model.scroll_preview_page_up(),
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    state.model.scroll_preview_half_page_down()
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    state.model.scroll_preview_half_page_up()
                }
                KeyCode::Char('g') => state.model.scroll_preview_top(),
                KeyCode::Char('G') => state.model.scroll_preview_bottom(),
                KeyCode::Enter => {
                    focus_selected_window(provider, state)?;
                    break;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn snapshot_output() -> Result<String, String> {
    let snapshot =
        kagent_sango::parse_snapshot_str(SANGO_SNAPSHOT_JSON).map_err(|error| error.to_string())?;
    let lens_snapshot = lens_snapshot_from_sango(&snapshot);
    let model = AgentLensViewModel::from_snapshot(lens_snapshot);

    Ok(render_agent_lens_text(&model))
}

fn context_output(flags: &[String]) -> Result<String, String> {
    match flags {
        [] => context_markdown_output(),
        [flag] if flag == "--markdown" => context_markdown_output(),
        [flag, ..] => Err(format!("unknown context flag: {flag}")),
    }
}

fn context_markdown_output() -> Result<String, String> {
    let snapshot =
        kagent_sango::parse_snapshot_str(SANGO_SNAPSHOT_JSON).map_err(|error| error.to_string())?;
    let lens_snapshot = lens_snapshot_from_sango(&snapshot);
    let mut output = String::new();

    output.push_str(&format!("# Project: {}\n\n", lens_snapshot.project.name));
    output.push_str(&format!("Root: {}\n\n", lens_snapshot.project.root));
    output.push_str(&format!(
        "Active worktree set: {}\n\n",
        lens_snapshot
            .project
            .active_worktree_set_id
            .as_deref()
            .unwrap_or("-")
    ));

    output.push_str("## Repos\n");
    for repo in &lens_snapshot.project.repos {
        output.push_str(&format!(
            "- {}: {}, dirty {} files\n",
            repo.repo_id,
            repo.branch.as_deref().unwrap_or("-"),
            repo.dirty.changed_files()
        ));
    }

    output.push_str("\n## Services\n");
    for service in &lens_snapshot.project.services {
        let port = service
            .ports
            .first()
            .map(|port| port.actual.to_string())
            .unwrap_or_else(|| "-".to_owned());
        output.push_str(&format!(
            "- {}: {}, health {}, port {}\n",
            service.service_id,
            service.status,
            service_health_label(service.health.status),
            port
        ));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_without_snapshot_describes_live_tui() {
        let output = command_output(&["dash".to_owned()]).expect("dash output");

        assert_eq!(output, "kagent dash opens the live Agent Lens TUI\n");
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

    #[test]
    fn quick_access_has_stable_non_tty_description() {
        let output = command_output(&["quick-access".to_owned()]).expect("quick-access output");

        assert_eq!(
            output,
            "kagent quick-access opens the live Agent Lens TUI\n"
        );
    }

    #[test]
    fn context_markdown_uses_sango_fixture_context() {
        let output = command_output(&["context".to_owned(), "--markdown".to_owned()])
            .expect("context output");

        assert!(output.contains("# Project: my-product"));
        assert!(output.contains("- repo: feature/auth-refactor, dirty 3 files"));
        assert!(output.contains("- api: running, health healthy, port 3100"));
    }
}
