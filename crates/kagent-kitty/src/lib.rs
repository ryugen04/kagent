use std::path::PathBuf;
use std::process::Command;
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyWindow {
    pub id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub cmdline: Vec<String>,
    pub foreground_cmdline: Vec<String>,
    pub is_self: bool,
    pub is_active: bool,
    pub screen_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyTab {
    pub id: String,
    pub title: String,
    pub is_active: bool,
    pub windows: Vec<KittyWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyWindowKind {
    CodexAgent,
    Shell,
    Editor,
    Tool,
    Generic,
}

impl KittyWindowKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::CodexAgent => "codex",
            Self::Shell => "shell",
            Self::Editor => "editor",
            Self::Tool => "tool",
            Self::Generic => "generic",
        }
    }
}

pub trait KittyWindowLister {
    fn list_windows(&self) -> Result<Vec<KittyWindow>, String>;
}

pub trait KittyTabLister {
    fn list_tabs(&self) -> Result<Vec<KittyTab>, String>;
}

pub trait KittyScreenReader {
    fn screen_text(&self, window_id: &str) -> Result<String, String>;
}

pub trait KittyFocuser {
    fn focus_window(&self, window_id: &str) -> Result<(), String>;
}

pub trait KittyProvider: KittyWindowLister + KittyScreenReader + KittyFocuser {}

impl<T> KittyProvider for T where T: KittyWindowLister + KittyScreenReader + KittyFocuser {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandKittyProvider {
    program: String,
    remote_target: Option<String>,
    snapshot_json: Option<String>,
}

impl CommandKittyProvider {
    pub fn default_program() -> String {
        let local_kitty = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".local/bin/kitty"));
        if let Some(local_kitty) = local_kitty.filter(|path| path.exists()) {
            local_kitty.to_string_lossy().into_owned()
        } else {
            "kitty".to_owned()
        }
    }

    pub fn new(program: impl Into<String>) -> Self {
        let snapshot_json = std::env::var("KAGENT_KITTY_LS_JSON")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::env::var("KAGENT_KITTY_LS_JSON_PATH")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .and_then(|path| read_snapshot_file(&path))
            });

        Self {
            program: program.into(),
            remote_target: std::env::var("KAGENT_KITTY_TO")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    std::env::var("KITTY_LISTEN_ON")
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                }),
            snapshot_json,
        }
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let mut command = Command::new(&self.program);
        if let Some(target) = self.remote_target.as_deref() {
            if let Some((first, rest)) = args.split_first() {
                if *first == "@" {
                    command.arg("@").arg("--to").arg(target).args(rest);
                } else {
                    command.args(args);
                }
            }
        } else {
            command.args(args);
        }

        let output = command
            .output()
            .map_err(|error| format!("failed to run {}: {error}", self.program))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let detail = if stderr.is_empty() {
                format!("exit status {}", output.status)
            } else {
                stderr
            };
            return Err(format!("{} {:?} failed: {detail}", self.program, args));
        }

        String::from_utf8(output.stdout).map_err(|error| format!("invalid kitty UTF-8: {error}"))
    }
}

fn read_snapshot_file(path: &str) -> Option<String> {
    let file = Path::new(path);
    if !file.exists() {
        return None;
    }
    fs::read_to_string(file)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

impl Default for CommandKittyProvider {
    fn default() -> Self {
        Self::new(Self::default_program())
    }
}

impl KittyWindowLister for CommandKittyProvider {
    fn list_windows(&self) -> Result<Vec<KittyWindow>, String> {
        if let Some(snapshot) = self.snapshot_json.as_deref() {
            return parse_kitty_windows_json(snapshot);
        }
        parse_kitty_windows_json(&self.run(&["@", "ls"])?)
    }
}

impl KittyTabLister for CommandKittyProvider {
    fn list_tabs(&self) -> Result<Vec<KittyTab>, String> {
        if let Some(snapshot) = self.snapshot_json.as_deref() {
            return parse_kitty_tabs_json(snapshot);
        }
        parse_kitty_tabs_json(&self.run(&["@", "ls"])?)
    }
}

impl KittyScreenReader for CommandKittyProvider {
    fn screen_text(&self, window_id: &str) -> Result<String, String> {
        let matcher = format!("id:{window_id}");
        self.run(&["@", "get-text", "--match", &matcher])
    }
}

impl KittyFocuser for CommandKittyProvider {
    fn focus_window(&self, window_id: &str) -> Result<(), String> {
        let matcher = format!("id:{window_id}");
        self.run(&["@", "focus-window", "--match", &matcher])?;
        self.run(&[
            "@",
            "resize-os-window",
            "--action=show",
            "--match",
            &matcher,
        ])?;
        Ok(())
    }
}

pub fn focus_window_command(window_id: &str) -> Vec<String> {
    vec![
        CommandKittyProvider::default_program(),
        "@".to_string(),
        "focus-window".to_string(),
        "--match".to_string(),
        format!("id:{window_id}"),
    ]
}

pub fn parse_kitty_windows_json(input: &str) -> Result<Vec<KittyWindow>, String> {
    let value = JsonParser::new(input).parse()?;
    let mut windows = Vec::new();
    collect_windows(&value, &mut windows);
    Ok(windows)
}

pub fn parse_kitty_tabs_json(input: &str) -> Result<Vec<KittyTab>, String> {
    let value = JsonParser::new(input).parse()?;
    let mut tabs = Vec::new();
    collect_tabs(&value, &mut tabs);
    Ok(tabs)
}

pub fn classify_window(window: &KittyWindow) -> KittyWindowKind {
    let cmdline = if window.foreground_cmdline.is_empty() {
        &window.cmdline
    } else {
        &window.foreground_cmdline
    };

    if cmdline.iter().any(|arg| command_basename(arg) == "codex") {
        KittyWindowKind::CodexAgent
    } else if cmdline.iter().any(|arg| command_basename(arg) == "lazygit") {
        KittyWindowKind::Tool
    } else if cmdline.iter().any(|arg| {
        matches!(
            command_basename(arg),
            "nvim" | "vim" | "vi" | "nvim.appimage"
        )
    }) {
        KittyWindowKind::Editor
    } else if cmdline
        .iter()
        .any(|arg| matches!(command_basename(arg), "bash" | "zsh" | "fish" | "sh" | "nu"))
    {
        KittyWindowKind::Shell
    } else {
        KittyWindowKind::Generic
    }
}

pub fn render_tab_window_summary(tabs: &[KittyTab]) -> String {
    let mut output = String::new();
    for tab in tabs {
        output.push_str(&format!(
            "TAB id={} active={} title={} windows={}\n",
            tab.id,
            tab.is_active,
            tab.title,
            tab.windows.len()
        ));
        for window in &tab.windows {
            output.push_str(&format!(
                "  WIN id={} kind={} active={} self={} title={} cwd={}\n",
                window.id,
                classify_window(window).label(),
                window.is_active,
                window.is_self,
                window.title,
                window.cwd.as_deref().unwrap_or("-")
            ));
        }
    }
    output
}

fn collect_tabs(value: &JsonValue, tabs: &mut Vec<KittyTab>) {
    match value {
        JsonValue::Array(items) => {
            for item in items {
                collect_tabs(item, tabs);
            }
        }
        JsonValue::Object(fields) => {
            if let Some(JsonValue::Array(items)) = object_get(fields, "tabs") {
                for item in items {
                    if let Some(tab) = parse_tab(item) {
                        tabs.push(tab);
                    }
                }
                return;
            }

            for (_, field_value) in fields {
                collect_tabs(field_value, tabs);
            }
        }
        _ => {}
    }
}

fn parse_tab(value: &JsonValue) -> Option<KittyTab> {
    let JsonValue::Object(fields) = value else {
        return None;
    };

    let windows_value = object_get(fields, "windows")?;
    let JsonValue::Array(window_values) = windows_value else {
        return None;
    };

    let id = object_get(fields, "id")
        .and_then(value_as_string)
        .unwrap_or_default();
    let title = object_get(fields, "title")
        .and_then(value_as_string)
        .unwrap_or_default();
    let is_active = object_get(fields, "is_active")
        .and_then(value_as_bool)
        .unwrap_or(false);
    let windows = window_values.iter().filter_map(parse_window).collect();

    Some(KittyTab {
        id,
        title,
        is_active,
        windows,
    })
}

fn collect_windows(value: &JsonValue, windows: &mut Vec<KittyWindow>) {
    match value {
        JsonValue::Array(items) => {
            for item in items {
                collect_windows(item, windows);
            }
        }
        JsonValue::Object(fields) => {
            if let Some(JsonValue::Array(items)) = object_get(fields, "windows") {
                for item in items {
                    if let Some(window) = parse_window(item) {
                        windows.push(window);
                    }
                }
            }

            for (_, field_value) in fields {
                collect_windows(field_value, windows);
            }
        }
        _ => {}
    }
}

fn parse_window(value: &JsonValue) -> Option<KittyWindow> {
    let JsonValue::Object(fields) = value else {
        return None;
    };

    let id = value_as_string(object_get(fields, "id")?)?;
    let title = object_get(fields, "title")
        .and_then(value_as_string)
        .unwrap_or_default();
    let cwd = object_get(fields, "cwd")
        .or_else(|| object_get(fields, "current_working_directory"))
        .and_then(value_as_string);
    let cmdline = object_get(fields, "cmdline")
        .map(value_as_string_list)
        .unwrap_or_default();
    let foreground_cmdline = object_get(fields, "foreground_processes")
        .map(foreground_process_cmdline)
        .unwrap_or_default();
    let is_self = object_get(fields, "is_self")
        .and_then(value_as_bool)
        .unwrap_or(false);
    let is_active = object_get(fields, "is_active")
        .and_then(value_as_bool)
        .unwrap_or(false);
    let screen_text = object_get(fields, "screen_text").and_then(value_as_string);

    Some(KittyWindow {
        id,
        title,
        cwd,
        cmdline,
        foreground_cmdline,
        is_self,
        is_active,
        screen_text,
    })
}

fn object_get<'a>(fields: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    fields
        .iter()
        .find_map(|(field_key, value)| (field_key == key).then_some(value))
}

fn value_as_string(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Number(value) => Some(value.clone()),
        _ => None,
    }
}

fn value_as_string_list(value: &JsonValue) -> Vec<String> {
    match value {
        JsonValue::Array(items) => items.iter().filter_map(value_as_string).collect(),
        JsonValue::String(value) => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn foreground_process_cmdline(value: &JsonValue) -> Vec<String> {
    let JsonValue::Array(processes) = value else {
        return Vec::new();
    };

    processes
        .iter()
        .filter_map(|process| {
            let JsonValue::Object(fields) = process else {
                return None;
            };
            object_get(fields, "cmdline").map(value_as_string_list)
        })
        .find(|cmdline| !cmdline.is_empty())
        .unwrap_or_default()
}

fn value_as_bool(value: &JsonValue) -> Option<bool> {
    match value {
        JsonValue::Bool(value) => Some(*value),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

struct JsonParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn parse(mut self) -> Result<JsonValue, String> {
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos == self.input.len() {
            Ok(value)
        } else {
            Err("unexpected trailing JSON data".to_string())
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", JsonValue::Null),
            Some(b't') => self.parse_literal(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", JsonValue::Bool(false)),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(JsonValue::Number),
            Some(byte) => Err(format!("unexpected JSON byte {byte} at {}", self.pos)),
            None => Err("unexpected end of JSON".to_string()),
        }
    }

    fn parse_literal(&mut self, expected: &[u8], value: JsonValue) -> Result<JsonValue, String> {
        if self.input.get(self.pos..self.pos + expected.len()) == Some(expected) {
            self.pos += expected.len();
            Ok(value)
        } else {
            Err(format!("invalid JSON literal at {}", self.pos))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut value = String::new();

        while let Some(byte) = self.next() {
            match byte {
                b'"' => return Ok(value),
                b'\\' => value.push(self.parse_escape()?),
                byte if byte < 0x20 => return Err("unescaped control byte in string".to_string()),
                byte => value.push(byte as char),
            }
        }

        Err("unterminated JSON string".to_string())
    }

    fn parse_escape(&mut self) -> Result<char, String> {
        match self.next() {
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'/') => Ok('/'),
            Some(b'b') => Ok('\u{0008}'),
            Some(b'f') => Ok('\u{000c}'),
            Some(b'n') => Ok('\n'),
            Some(b'r') => Ok('\r'),
            Some(b't') => Ok('\t'),
            Some(b'u') => {
                let code = self.parse_hex4()?;
                char::from_u32(code).ok_or_else(|| "invalid unicode escape".to_string())
            }
            _ => Err("invalid JSON escape".to_string()),
        }
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        let mut value = 0;
        for _ in 0..4 {
            value = value * 16
                + match self.next() {
                    Some(byte @ b'0'..=b'9') => (byte - b'0') as u32,
                    Some(byte @ b'a'..=b'f') => (byte - b'a' + 10) as u32,
                    Some(byte @ b'A'..=b'F') => (byte - b'A' + 10) as u32,
                    _ => return Err("invalid unicode escape".to_string()),
                };
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<String, String> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.pos += 1;
        }

        std::str::from_utf8(&self.input[start..self.pos])
            .map(str::to_string)
            .map_err(|_| "invalid JSON number".to_string())
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();

        loop {
            self.skip_ws();
            if self.consume(b']') {
                return Ok(JsonValue::Array(items));
            }

            items.push(self.parse_value()?);
            self.skip_ws();

            if self.consume(b']') {
                return Ok(JsonValue::Array(items));
            }
            self.expect(b',')?;
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        let mut fields = Vec::new();

        loop {
            self.skip_ws();
            if self.consume(b'}') {
                return Ok(JsonValue::Object(fields));
            }

            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            fields.push((key, value));
            self.skip_ws();

            if self.consume(b'}') {
                return Ok(JsonValue::Object(fields));
            }
            self.expect(b',')?;
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!("expected byte {expected} at {}", self.pos))
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }
}

fn command_basename(arg: &str) -> &str {
    arg.rsplit('/').next().unwrap_or(arg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_control_window_json() {
        let input = r#"
        [
          {
            "id": 1,
            "tabs": [
              {
                "windows": [
                  {
                    "id": 7,
                    "title": "codex work",
                    "cwd": "file://host/workspace/project",
                    "cmdline": ["bash"],
                    "foreground_processes": [
                      {
                        "cmdline": ["codex", "exec"]
                      }
                    ],
                    "is_self": true,
                    "is_active": true,
                    "screen_text": "Need approval to run cargo test"
                  }
                ]
              }
            ]
          }
        ]
        "#;

        let windows = parse_kitty_windows_json(input).expect("valid kitty JSON");

        assert_eq!(
            windows,
            vec![KittyWindow {
                id: "7".to_string(),
                title: "codex work".to_string(),
                cwd: Some("file://host/workspace/project".to_string()),
                cmdline: vec!["bash".to_string()],
                foreground_cmdline: vec!["codex".to_string(), "exec".to_string()],
                is_self: true,
                is_active: true,
                screen_text: Some("Need approval to run cargo test".to_string()),
            }]
        );
    }

    #[test]
    fn builds_focus_window_command_without_running_it() {
        let command = focus_window_command("7");

        assert!(command[0].ends_with("kitty"));
        assert_eq!(
            &command[1..],
            [
                "@".to_string(),
                "focus-window".to_string(),
                "--match".to_string(),
                "id:7".to_string(),
            ]
        );
    }

    #[test]
    fn parses_tabs_and_classifies_real_shape_fixture() {
        let tabs = parse_kitty_tabs_json(real_shape_fixture()).expect("valid kitty tabs");

        assert_eq!(tabs.len(), 4);
        assert_eq!(tabs.iter().map(|tab| tab.windows.len()).sum::<usize>(), 10);
        assert_eq!(count_kind(&tabs, KittyWindowKind::CodexAgent), 6);
        assert_eq!(count_kind(&tabs, KittyWindowKind::Tool), 1);
        assert_eq!(count_kind(&tabs, KittyWindowKind::Editor), 1);
        assert_eq!(count_kind(&tabs, KittyWindowKind::Shell), 2);

        let summary = render_tab_window_summary(&tabs);
        assert!(summary.contains("TAB id=1 active=false title=dotfiles windows=4"));
        assert!(summary.contains("WIN id=22 kind=tool active=false self=false title=gg"));
        assert!(
            summary.contains("WIN id=23 kind=editor active=false self=false title=vi config.toml")
        );
    }

    fn count_kind(tabs: &[KittyTab], kind: KittyWindowKind) -> usize {
        tabs.iter()
            .flat_map(|tab| tab.windows.iter())
            .filter(|window| classify_window(window) == kind)
            .count()
    }

    fn real_shape_fixture() -> &'static str {
        r#"
        [
          {
            "tabs": [
              {
                "id": 1,
                "is_active": false,
                "title": "dotfiles",
                "windows": [
                  {"id": 9, "is_active": false, "is_self": false, "title": "example-app", "cwd": "/workspace/example-app", "foreground_processes": [{"cmdline": ["node", "/opt/node/bin/codex"]}]},
                  {"id": 12, "is_active": true, "is_self": false, "title": "dotfiles", "cwd": "/workspace/dotfiles", "foreground_processes": [{"cmdline": ["node", "/opt/node/bin/codex"]}]},
                  {"id": 14, "is_active": false, "is_self": false, "title": "example-app", "cwd": "/workspace/example-app", "foreground_processes": [{"cmdline": ["/bin/bash", "--posix"]}]},
                  {"id": 17, "is_active": false, "is_self": false, "title": "dotfiles", "cwd": "/workspace/dotfiles", "foreground_processes": [{"cmdline": ["node", "/opt/node/bin/codex"]}]}
                ]
              },
              {
                "id": 3,
                "is_active": false,
                "title": "sango",
                "windows": [
                  {"id": 18, "is_active": true, "is_self": false, "title": "sango", "cwd": "/workspace/sango", "foreground_processes": [{"cmdline": ["/bin/bash", "--posix"]}]}
                ]
              },
              {
                "id": 4,
                "is_active": false,
                "title": "codex-careflow",
                "windows": [
                  {"id": 19, "is_active": true, "is_self": false, "title": "codex-careflow", "cwd": "/workspace/codex-careflow", "foreground_processes": [{"cmdline": ["node", "/opt/node/bin/codex"]}]},
                  {"id": 20, "is_active": false, "is_self": false, "title": "dotfiles", "cwd": "/workspace/dotfiles", "foreground_processes": [{"cmdline": ["node", "/opt/node/bin/codex"]}]},
                  {"id": 23, "is_active": false, "is_self": false, "title": "vi config.toml ", "cwd": "/workspace/dotfiles", "foreground_processes": [{"cmdline": ["/usr/bin/nvim", "/workspace/config/config.toml"]}]}
                ]
              },
              {
                "id": 5,
                "is_active": true,
                "title": "⠸ kagent",
                "windows": [
                  {"id": 21, "is_active": true, "is_self": true, "title": "⠸ kagent", "cwd": "/workspace/kagent", "foreground_processes": [{"cmdline": ["node", "/opt/node/bin/codex"]}]},
                  {"id": 22, "is_active": false, "is_self": false, "title": "gg", "cwd": "/workspace/kagent", "foreground_processes": [{"cmdline": ["lazygit"]}]}
                ]
              }
            ]
          }
        ]
        "#
    }
}
