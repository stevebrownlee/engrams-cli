use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn head_sha(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let sha = stdout.trim();
        if !sha.is_empty() {
            return Some(sha.to_owned());
        }
    }
    None
}

pub fn origin_base(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["remote", "get-url", "origin"])
        .output()
        .context("git command unavailable")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "cannot derive PR URL: git command failed: {}; pass the full URL",
            stderr.trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout.trim();
    if url.is_empty() {
        anyhow::bail!("cannot derive PR URL: origin remote URL is empty; pass the full URL");
    }

    let url = url.strip_prefix("ssh://").unwrap_or(url);
    let url = url.strip_suffix(".git").unwrap_or(url);
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            if !host.contains('/') {
                return Ok(format!("https://{}/{}", host, path));
            }
        }
        if rest.starts_with("http://") || rest.starts_with("https://") {
            return Ok(rest.to_owned());
        }
        return Ok(format!("https://{}", rest));
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(url.to_owned())
    } else {
        Ok(format!("https://{}", url))
    }
}

pub fn staged_files() -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .output()
        .context("git command failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff failed: {}", stderr.trim());
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff failed: {}", stderr.trim());
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
pub fn toplevel(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git command failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-parse --show-toplevel failed: {}", stderr.trim());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().to_owned())
}

/// Files touched per commit, one `Vec<String>` per commit (newest first).
///
/// Runs `git log --no-merges --name-only --format=%x1e%H`, bounded to
/// `max_commits`, optionally restricted to `<since>..HEAD`. Returns an empty
/// vec when git fails (not a repo / no commits / bad range) so ingest is a
/// safe no-op off-repo.
pub fn commit_file_groups(
    root: &Path,
    since: Option<&str>,
    max_commits: usize,
) -> Result<Vec<Vec<String>>> {
    let max_str = max_commits.to_string();
    let range = since.map(|s| format!("{}..HEAD", s));
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).args([
        "log",
        "--no-merges",
        "--name-only",
        "--format=%x1e%H",
        "-n",
        &max_str,
    ]);
    if let Some(r) = &range {
        cmd.arg(r);
    }
    let output = cmd.output().context("git command failed")?;
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
