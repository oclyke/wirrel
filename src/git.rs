//! The only I/O boundary: shells out to `git` and returns `RawCommit` data.

use std::io;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Added(String),
    Modified(String),
    Renamed { from: String, to: String },
}

/// Lines a commit added to and deleted from one file, keyed by the path the
/// file has after the commit. Binary files count as zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    pub path: String,
    pub added: u32,
    pub deleted: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCommit {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub changed: Vec<FileChange>,
    pub stats: Vec<FileStat>,
}

/// Load commit history oldest -> newest.
pub fn load(repo: &Path) -> io::Result<Vec<RawCommit>> {
    ensure_repo(repo)?;
    let meta = run(repo, &[
        "log",
        "--reverse",
        "-z",
        "--format=%H%x1f%aI%x1f%s%x1f%b",
    ])?;

    let mut commits = Vec::new();
    for record in meta.split('\0') {
        if record.trim().is_empty() {
            continue;
        }
        let mut f = record.splitn(4, '\x1f');
        let sha = f.next().unwrap_or("").to_string();
        let date = f.next().unwrap_or("").to_string();
        let subject = f.next().unwrap_or("").to_string();
        let body = f.next().unwrap_or("").trim_end().to_string();

        let changed = load_changes(repo, &sha)?;
        let stats = load_stats(repo, &sha)?;
        commits.push(RawCommit { sha, subject, body, date, changed, stats });
    }
    Ok(commits)
}

fn load_changes(repo: &Path, sha: &str) -> io::Result<Vec<FileChange>> {
    let out = run(repo, &[
        "diff-tree",
        "--root",
        "--no-commit-id",
        "-r",
        "-M",
        "-z",
        "--name-status",
        sha,
    ])?;

    // -z fields: `A\0path`, `M\0path`, `D\0path`, `R100\0from\0to`.
    let mut parts = out.split('\0').filter(|s| !s.is_empty());
    let mut changes = Vec::new();
    while let Some(status) = parts.next() {
        match status.chars().next().unwrap_or(' ') {
            'A' => {
                if let Some(p) = parts.next() {
                    changes.push(FileChange::Added(p.to_string()));
                }
            }
            'M' => {
                if let Some(p) = parts.next() {
                    changes.push(FileChange::Modified(p.to_string()));
                }
            }
            'R' | 'C' => {
                let from = parts.next().unwrap_or("").to_string();
                let to = parts.next().unwrap_or("").to_string();
                changes.push(FileChange::Renamed { from, to });
            }
            _ => {
                parts.next();
            }
        }
    }
    Ok(changes)
}

fn load_stats(repo: &Path, sha: &str) -> io::Result<Vec<FileStat>> {
    let out = run(repo, &[
        "diff-tree",
        "--root",
        "--no-commit-id",
        "-r",
        "-M",
        "-z",
        "--numstat",
        sha,
    ])?;

    // -z fields: `added\tdeleted\tpath`, or `added\tdeleted\t\0from\0to` for a
    // rename. Binary files report `-` for both counts.
    let mut parts = out.split('\0');
    let mut stats = Vec::new();
    while let Some(record) = parts.next() {
        if record.is_empty() {
            continue;
        }
        let mut f = record.splitn(3, '\t');
        let added = f.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let deleted = f.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let path = match f.next() {
            Some("") | None => {
                parts.next();
                parts.next().unwrap_or("")
            }
            Some(p) => p,
        };
        stats.push(FileStat { path: path.to_string(), added, deleted });
    }
    Ok(stats)
}

/// Paths tracked by git, repo-relative.
pub fn tracked_files(repo: &Path) -> io::Result<Vec<String>> {
    let out = run(repo, &["ls-files", "-z"])?;
    Ok(out.split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
}

/// The unified diff introduced by a commit (patch only, no commit header).
pub fn commit_diff(repo: &Path, sha: &str) -> io::Result<String> {
    run(repo, &["diff-tree", "-p", "--no-commit-id", "-r", "--root", sha])
}

fn ensure_repo(repo: &Path) -> io::Result<()> {
    if !repo.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("path does not exist: {}", repo.display()),
        ));
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--git-dir"])
        .output()?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "not a git repository: {}",
            repo.display()
        )));
    }
    Ok(())
}

fn run(repo: &Path, args: &[&str]) -> io::Result<String> {
    tracing::debug!(?args, "git");
    let out = Command::new("git").arg("-C").arg(repo).args(args).output()?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
