#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyWindow {
    pub id: String,
    pub title: String,
    pub cwd: Option<String>,
}

pub trait KittyProvider {
    fn list_windows(&self) -> Result<Vec<KittyWindow>, String>;
    fn focus_window(&self, window_id: &str) -> Result<(), String>;
}
