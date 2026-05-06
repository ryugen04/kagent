#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSummary {
    pub id: String,
    pub path: String,
    pub branch: Option<String>,
    pub dirty_files: usize,
}

pub trait GitProvider {
    fn repo_summary(&self, path: &str) -> Result<RepoSummary, String>;
}
