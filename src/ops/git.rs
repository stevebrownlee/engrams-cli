use anyhow::{Context, Result};
use std::process::Command;
use std::sync::LazyLock;

pub fn head_sha() -> Option<String> {
    static HEAD_SHA: LazyLock<Option<String>> = LazyLock::new(|| {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()?;
        if output.status.success() {
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sha.is_empty() {
                return Some(sha);
            }
        }
        None
    });
    (*HEAD_SHA).clone()
}

pub fn origin_base() -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .context("git command unavailable")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "cannot derive PR URL: git command failed: {}; pass the full URL",
            stderr
        );
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        anyhow::bail!("cannot derive PR URL: origin remote URL is empty; pass the full URL");
    }

    let mut normalized = url;
    if normalized.starts_with("ssh://") {
        normalized = normalized["ssh://".len()..].to_string();
    }
    if normalized.starts_with("git@") {
        normalized = normalized["git@".len()..].to_string();
        if let Some(colon_pos) = normalized.find(':') {
            if !normalized[..colon_pos].contains('/') {
                let host = &normalized[..colon_pos];
                let path = &normalized[colon_pos + 1..];
                normalized = format!("{}/{}", host, path);
            }
        }
    }
    if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
        normalized = format!("https://{}", normalized);
    }
    if normalized.ends_with(".git") {
        normalized.truncate(normalized.len() - 4);
    }
    Ok(normalized)
}

pub fn staged_files() -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .output()
        .context("git command failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("git diff failed: {}", stderr);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    Ok(files)
}

pub fn changed_since(sha: &str, paths: &[String]) -> Result<Vec<String>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["diff", "--name-only", sha, "HEAD", "--"];
    for p in paths {
        args.push(p);
    }
    let output = Command::new("git")
        .args(&args)
        .output()
        .context("git command failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("git diff failed: {}", stderr);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    Ok(files)
}

/// Absolute path of the repository root; errors when not inside a repo.
pub fn toplevel() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git command failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("git rev-parse --show-toplevel failed: {}", stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Files touched per commit, one `Vec<String>` per commit (newest first).
///
/// Runs `git log --no-merges --name-only --format=%x1e%H`, bounded to
/// `max_commits`, optionally restricted to `<since>..HEAD`. Returns an empty
/// vec when git fails (not a repo / no commits / bad range) so ingest is a
/// safe no-op off-repo.
pub fn commit_file_groups(since: Option<&str>, max_commits: usize) -> Result<Vec<Vec<String>>> {
    let mut args: Vec<String> = vec![
        "log".to_string(),
        "--no-merges".to_string(),
        "--name-only".to_string(),
        "--format=%x1e%H".to_string(),
        "-n".to_string(),
        max_commits.to_string(),
    ];
    if let Some(s) = since {
        args.push(format!("{}..HEAD", s));
    }
    let output = Command::new("git")
        .args(&args)
        .output()
        .context("git command failed")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut groups = Vec::new();
    for record in stdout.split('\x1e') {
        let mut lines = record.lines();
        let hash = lines.next().unwrap_or("").trim();
        if hash.is_empty() {
            continue;
        }
        let files: Vec<String> = lines
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        groups.push(files);
    }
    Ok(groups)
}
