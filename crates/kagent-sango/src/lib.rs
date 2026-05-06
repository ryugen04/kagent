#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SangoServiceSummary {
    pub id: String,
    pub repo_id: Option<String>,
    pub status: String,
    pub port: Option<u16>,
}

pub trait SangoProvider {
    fn snapshot_json(&self) -> Result<String, String>;
}
