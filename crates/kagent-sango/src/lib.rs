use serde::Deserialize;
use std::{
    error::Error,
    fmt, io,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Snapshot {
    #[serde(flatten)]
    pub metadata: Metadata,
    pub project: Project,
    #[serde(default)]
    pub repos: Vec<Repo>,
    #[serde(default)]
    pub services: Vec<Service>,
    #[serde(default)]
    pub worktree_sets: Vec<WorktreeSet>,
    #[serde(default)]
    pub service_instances: Vec<ServiceInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Metadata {
    pub schema_version: u32,
    pub generated_at: String,
    pub project_root: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Project {
    pub name: String,
    pub root: String,
    pub active_worktree_set: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Repo {
    pub id: String,
    pub path: String,
    pub default_branch: String,
    #[serde(default)]
    pub services: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Service {
    pub id: String,
    #[serde(default)]
    pub repo_id: Option<String>,
    #[serde(rename = "type")]
    pub service_type: String,
    pub shared: bool,
    #[serde(default)]
    pub port_base: Option<u16>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorktreeSet {
    pub id: String,
    pub active: bool,
    #[serde(default)]
    pub repo_worktrees: Vec<RepoWorktree>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoWorktree {
    pub id: String,
    pub repo_id: String,
    pub worktree_set_id: String,
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub head: Option<String>,
    pub exists: bool,
    pub dirty: DirtySummary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct DirtySummary {
    #[serde(default)]
    pub files: u32,
    #[serde(default)]
    pub staged: u32,
    #[serde(default)]
    pub unstaged: u32,
    #[serde(default)]
    pub untracked: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServiceInstance {
    pub id: String,
    pub service_id: String,
    #[serde(default)]
    pub repo_id: Option<String>,
    pub worktree_set_id: String,
    #[serde(rename = "type")]
    pub service_type: String,
    pub shared: bool,
    pub status: String,
    pub health: Health,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub restart_count: Option<u32>,
    #[serde(default)]
    pub port_listening: Option<bool>,
    #[serde(default)]
    pub process_alive: Option<bool>,
    #[serde(default)]
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Port {
    pub name: String,
    pub base: u16,
    pub actual: u16,
    #[serde(default)]
    pub url: Option<String>,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Health {
    pub status: String,
    #[serde(default)]
    pub checked_at: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SangoServiceSummary {
    pub id: String,
    pub repo_id: Option<String>,
    pub status: String,
    pub port: Option<u16>,
}

impl From<&ServiceInstance> for SangoServiceSummary {
    fn from(instance: &ServiceInstance) -> Self {
        Self {
            id: instance.service_id.clone(),
            repo_id: instance.repo_id.clone(),
            status: instance.status.clone(),
            port: instance.ports.first().map(|port| port.actual),
        }
    }
}

pub trait SangoProvider {
    fn snapshot(&self) -> Result<Snapshot, SangoError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SangoCommandProvider {
    binary: PathBuf,
    root: PathBuf,
}

impl SangoCommandProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_binary(root, "sango")
    }

    pub fn with_binary(root: impl Into<PathBuf>, binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            root: root.into(),
        }
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn command_display(&self) -> String {
        command_display(&self.binary, &self.root)
    }
}

impl SangoProvider for SangoCommandProvider {
    fn snapshot(&self) -> Result<Snapshot, SangoError> {
        let command = self.command_display();
        let output = Command::new(&self.binary)
            .args(["snapshot", "--json", "--root"])
            .arg(&self.root)
            .output()
            .map_err(|source| SangoError::CommandSpawn {
                command: command.clone(),
                root: self.root.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(SangoError::CommandStatus {
                command,
                root: self.root.clone(),
                status_code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }

        parse_snapshot_bytes_with_context(
            &output.stdout,
            format!("command `{}` for root `{}`", command, self.root.display()),
        )
    }
}

#[derive(Debug)]
pub enum SangoError {
    Parse {
        context: String,
        source: serde_json::Error,
    },
    CommandSpawn {
        command: String,
        root: PathBuf,
        source: io::Error,
    },
    CommandStatus {
        command: String,
        root: PathBuf,
        status_code: Option<i32>,
        stderr: String,
    },
}

impl fmt::Display for SangoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { context, source } => {
                write!(
                    f,
                    "failed to parse sango snapshot JSON from {context}: {source}"
                )
            }
            Self::CommandSpawn {
                command,
                root,
                source,
            } => write!(
                f,
                "failed to invoke `{command}` for root `{}`: {source}",
                root.display()
            ),
            Self::CommandStatus {
                command,
                root,
                status_code,
                stderr,
            } => {
                write!(
                    f,
                    "`{command}` failed for root `{}` with status {}",
                    root.display(),
                    status_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "terminated by signal".to_owned())
                )?;
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for SangoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse { source, .. } => Some(source),
            Self::CommandSpawn { source, .. } => Some(source),
            Self::CommandStatus { .. } => None,
        }
    }
}

pub fn parse_snapshot_str(input: &str) -> Result<Snapshot, SangoError> {
    parse_snapshot_bytes_with_context(input.as_bytes(), "string input")
}

pub fn parse_snapshot_bytes(input: &[u8]) -> Result<Snapshot, SangoError> {
    parse_snapshot_bytes_with_context(input, "byte input")
}

fn parse_snapshot_bytes_with_context(
    input: &[u8],
    context: impl Into<String>,
) -> Result<Snapshot, SangoError> {
    serde_json::from_slice(input).map_err(|source| SangoError::Parse {
        context: context.into(),
        source,
    })
}

fn command_display(binary: &Path, root: &Path) -> String {
    format!(
        "{} snapshot --json --root {}",
        binary.display(),
        root.display()
    )
}
