use std::{fs, path::Path, process::Command};

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GitCommandProvider;

impl GitProvider for GitCommandProvider {
    fn repo_summary(&self, path: &str) -> Result<RepoSummary, String> {
        let output = Command::new("git")
            .args(["-C", path, "status", "--short", "--branch"])
            .output()
            .map_err(|error| format!("failed to invoke git for `{path}`: {error}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(format!(
                "git status failed for `{path}` with status {}{}",
                output
                    .status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "terminated by signal".to_owned()),
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                }
            ));
        }

        repo_summary_from_status(path, &String::from_utf8_lossy(&output.stdout))
    }
}

impl GitCommandProvider {
    pub fn discover_repo_summaries(&self, path: &str) -> Vec<RepoSummary> {
        discover_repo_paths(path)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|repo_path| self.repo_summary(&repo_path).ok())
            .collect()
    }
}

pub fn discover_repo_paths(path: &str) -> Result<Vec<String>, String> {
    let root = Path::new(path);
    if is_git_repo(root) {
        return Ok(vec![root.to_string_lossy().into_owned()]);
    }

    let entries = fs::read_dir(root)
        .map_err(|error| format!("failed to read repo candidates under `{path}`: {error}"))?;
    let mut repos = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && is_git_repo(path))
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    repos.sort();
    Ok(repos)
}

pub fn repo_summary_from_status(path: &str, status: &str) -> Result<RepoSummary, String> {
    let branch = status.lines().find_map(parse_branch_line);
    let dirty_files = status
        .lines()
        .filter(|line| !line.starts_with("##") && !line.trim().is_empty())
        .count();

    Ok(RepoSummary {
        id: repo_id_from_path(path),
        path: path.to_owned(),
        branch,
        dirty_files,
    })
}

fn parse_branch_line(line: &str) -> Option<String> {
    let rest = line.strip_prefix("## ")?;
    let branch = rest
        .split("...")
        .next()
        .and_then(|value| value.split(' ').next())
        .filter(|value| !value.is_empty())?;

    Some(branch.to_owned())
}

fn repo_id_from_path(path: &str) -> String {
    path.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_owned()
}

fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_porcelain_branch_and_dirty_count() {
        let summary = repo_summary_from_status(
            "/workspace/app",
            "\
## feature/auth...origin/feature/auth
 M src/lib.rs
?? tests/new.rs
",
        )
        .expect("summary");

        assert_eq!(summary.id, "app");
        assert_eq!(summary.branch.as_deref(), Some("feature/auth"));
        assert_eq!(summary.dirty_files, 2);
    }

    #[test]
    fn clean_status_reports_zero_dirty_files() {
        let summary = repo_summary_from_status("/workspace/app", "## main\n").expect("summary");

        assert_eq!(summary.branch.as_deref(), Some("main"));
        assert_eq!(summary.dirty_files, 0);
    }

    #[test]
    fn discovers_cwd_repo_before_child_repos() {
        let root = unique_temp_dir("kagent-git-root-repo");
        fs::create_dir_all(root.join(".git")).expect("create .git");
        fs::create_dir_all(root.join("child").join(".git")).expect("create child .git");

        let paths = discover_repo_paths(root.to_str().expect("utf8 path")).expect("paths");

        assert_eq!(paths, vec![root.to_string_lossy().into_owned()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovers_one_level_child_repos() {
        let root = unique_temp_dir("kagent-git-child-repos");
        fs::create_dir_all(root.join("api").join(".git")).expect("create api .git");
        fs::create_dir_all(root.join("web").join(".git")).expect("create web .git");
        fs::create_dir_all(root.join("nested").join("deep").join(".git")).expect("deep ignored");

        let paths = discover_repo_paths(root.to_str().expect("utf8 path")).expect("paths");

        assert_eq!(
            paths,
            vec![
                root.join("api").to_string_lossy().into_owned(),
                root.join("web").to_string_lossy().into_owned(),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{name}-{suffix}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }
}
