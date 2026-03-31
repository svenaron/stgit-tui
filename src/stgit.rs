use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchStatus {
    Applied,
    Current,
    Unapplied,
}

#[derive(Debug, Clone)]
pub struct Patch {
    pub name: String,
    pub description: String,
    pub status: PatchStatus,
    pub empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Unresolved,
}

impl FileStatus {
    pub fn label(&self) -> &str {
        match self {
            FileStatus::Modified => "Modified",
            FileStatus::Added => "Added",
            FileStatus::Deleted => "Deleted",
            FileStatus::Renamed => "Renamed",
            FileStatus::Copied => "Copied",
            FileStatus::Untracked => "Unknown",
            FileStatus::Unresolved => "Unresolved",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub status: FileStatus,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub upstream: Option<String>,
}

#[derive(Debug)]
pub struct StackState {
    pub branch: BranchInfo,
    pub history: Vec<String>,
    pub patches: Vec<Patch>,
    pub index_files: Vec<FileEntry>,
    pub worktree_files: Vec<FileEntry>,
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {cmd}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_cmd_ok(cmd: &str, args: &[&str]) -> Result<(bool, String, String)> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {cmd}"))?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

pub fn get_branch_info() -> Result<BranchInfo> {
    let name = run_cmd("git", &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let upstream = run_cmd(
        "git",
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
    Ok(BranchInfo { name, upstream })
}

pub fn get_history(count: usize) -> Result<Vec<String>> {
    // Show commits below the stack base
    let base = run_cmd("stg", &["id", "{base}"])
        .unwrap_or_default()
        .trim()
        .to_string();
    if base.is_empty() {
        return Ok(vec![]);
    }
    let output = run_cmd("git", &["log", "--format=%s", &format!("-{count}"), &base])?;
    Ok(output.lines().map(|l| l.to_string()).collect())
}

pub fn get_patches() -> Result<Vec<Patch>> {
    let mut patches = Vec::new();
    let series_output = run_cmd("stg", &["series", "--all", "--description"])?;
    for line in series_output.lines() {
        if line.is_empty() {
            continue;
        }
        let status_char = line.chars().next().unwrap_or(' ');
        let empty = line.chars().nth(1) == Some('0');
        let rest = if empty { &line[2..] } else { &line[1..] };
        let rest = rest.trim_start();

        let status = match status_char {
            '>' => PatchStatus::Current,
            '+' => PatchStatus::Applied,
            '-' => PatchStatus::Unapplied,
            _ => PatchStatus::Unapplied,
        };

        let (name, description) = match rest.find(" # ") {
            Some(pos) => (
                rest[..pos].trim_end().to_string(),
                rest[pos + 3..].to_string(),
            ),
            None => (rest.trim_end().to_string(), String::new()),
        };

        patches.push(Patch {
            name,
            description,
            status,
            empty,
        });
    }

    Ok(patches)
}

fn parse_status_code(x: u8, y: u8) -> Option<FileStatus> {
    match (x, y) {
        (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => Some(FileStatus::Unresolved),
        _ => None,
    }
}

pub fn get_index_files() -> Result<Vec<FileEntry>> {
    let output = run_cmd("git", &["diff", "--cached", "--name-status"])?;
    Ok(parse_name_status(&output))
}

pub fn get_worktree_files() -> Result<Vec<FileEntry>> {
    let output = run_cmd("git", &["diff", "--name-status"])?;
    let mut files = parse_name_status(&output);

    // Add untracked files
    let untracked = run_cmd("git", &["ls-files", "--others", "--exclude-standard"])?;
    for line in untracked.lines() {
        if !line.is_empty() {
            files.push(FileEntry {
                status: FileStatus::Untracked,
                path: line.to_string(),
            });
        }
    }

    // Check for unresolved conflicts
    let status_output = run_cmd("git", &["status", "--porcelain"])?;
    for line in status_output.lines() {
        if line.len() < 4 {
            continue;
        }
        let bytes = line.as_bytes();
        if let Some(FileStatus::Unresolved) = parse_status_code(bytes[0], bytes[1]) {
            let path = line[3..].to_string();
            // Remove any existing entry for this path and add as unresolved
            files.retain(|f| f.path != path);
            files.push(FileEntry {
                status: FileStatus::Unresolved,
                path,
            });
        }
    }

    Ok(files)
}

fn parse_name_status(output: &str) -> Vec<FileEntry> {
    let mut files = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let status = match line.chars().next() {
            Some('M') => FileStatus::Modified,
            Some('A') => FileStatus::Added,
            Some('D') => FileStatus::Deleted,
            Some('R') => FileStatus::Renamed,
            Some('C') => FileStatus::Copied,
            _ => FileStatus::Modified,
        };
        // Skip status char and tab
        let path = line.split_once('\t').map(|x| x.1).unwrap_or("").to_string();
        // For renames, the path contains "old\tnew"
        let path = if status == FileStatus::Renamed || status == FileStatus::Copied {
            path.splitn(2, '\t').last().unwrap_or(&path).to_string()
        } else {
            path
        };
        if !path.is_empty() {
            files.push(FileEntry { status, path });
        }
    }
    files
}

pub fn get_patch_files(patch_name: &str) -> Result<Vec<FileEntry>> {
    let output = run_cmd("stg", &["files", patch_name])?;
    let mut files = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let status = match line.chars().next() {
            Some('M') => FileStatus::Modified,
            Some('A') => FileStatus::Added,
            Some('D') => FileStatus::Deleted,
            Some('R') => FileStatus::Renamed,
            Some('C') => FileStatus::Copied,
            _ => FileStatus::Modified,
        };
        let path = line[1..].trim().to_string();
        if !path.is_empty() {
            files.push(FileEntry { status, path });
        }
    }
    Ok(files)
}

pub fn load_state(history_count: usize) -> Result<StackState> {
    let branch = get_branch_info()?;
    let history = get_history(history_count).unwrap_or_default();
    let patches = get_patches()?;
    let index_files = get_index_files()?;
    let worktree_files = get_worktree_files()?;

    Ok(StackState {
        branch,
        history,
        patches,
        index_files,
        worktree_files,
    })
}

// --- Operations ---

pub fn stg_refresh(patch: Option<&str>) -> Result<(bool, String, String)> {
    let mut args = vec!["refresh"];
    if let Some(p) = patch {
        args.push("-p");
        args.push(p);
    }
    run_cmd_ok("stg", &args)
}

pub fn stg_goto(patch: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["goto", patch])
}

pub fn stg_push(patches: &[&str]) -> Result<(bool, String, String)> {
    let mut args = vec!["push"];
    args.extend(patches);
    run_cmd_ok("stg", &args)
}

pub fn stg_pop(patches: &[&str]) -> Result<(bool, String, String)> {
    let mut args = vec!["pop"];
    args.extend(patches);
    run_cmd_ok("stg", &args)
}

pub fn stg_pop_current() -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["pop"])
}

pub fn stg_push_one() -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["push"])
}

pub fn stg_new(message: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["new", "-m", message])
}

pub fn stg_delete(patches: &[&str]) -> Result<(bool, String, String)> {
    let mut args = vec!["delete"];
    args.extend(patches);
    run_cmd_ok("stg", &args)
}

pub fn stg_squash(patches: &[&str]) -> Result<(bool, String, String)> {
    let mut args = vec!["squash"];
    args.extend(patches);
    run_cmd_ok("stg", &args)
}

pub fn stg_float(patches: &[&str]) -> Result<(bool, String, String)> {
    let mut args = vec!["float"];
    args.extend(patches);
    run_cmd_ok("stg", &args)
}

pub fn stg_sink(patches: &[&str], target: Option<&str>) -> Result<(bool, String, String)> {
    let mut args = vec!["sink"];
    if let Some(t) = target {
        args.push("-t");
        args.push(t);
    }
    args.extend(patches);
    run_cmd_ok("stg", &args)
}

pub fn stg_undo(hard: bool) -> Result<(bool, String, String)> {
    if hard {
        run_cmd_ok("stg", &["undo", "--hard"])
    } else {
        run_cmd_ok("stg", &["undo"])
    }
}

pub fn stg_redo() -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["redo"])
}

pub fn stg_repair() -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["repair"])
}

pub fn stg_commit(patches: &[&str]) -> Result<(bool, String, String)> {
    let mut args = vec!["commit"];
    args.extend(patches);
    run_cmd_ok("stg", &args)
}

pub fn stg_uncommit(count: usize) -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["uncommit", "-n", &count.to_string()])
}

pub fn stg_edit(patch: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["edit", "-e", patch])
}

pub fn git_stage(path: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["add", path])
}

pub fn git_unstage(path: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["reset", "HEAD", path])
}

pub fn git_revert_worktree(path: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["checkout", "--", path])
}

pub fn git_revert_index(path: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["checkout", "HEAD", "--", path])
}

pub fn git_diff(path: &str, cached: bool) -> Result<String> {
    let mut args = vec!["diff"];
    if cached {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);
    run_cmd("git", &args)
}

pub fn stg_diff(patch: &str) -> Result<String> {
    run_cmd("stg", &["show", patch])
}

pub fn git_resolve(path: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["add", path])
}

// --- Branch operations ---

pub fn stg_branch_list() -> Result<Vec<String>> {
    let output = run_cmd("stg", &["branch", "--list"])?;
    Ok(output
        .lines()
        .filter_map(|l| {
            let trimmed = l.trim();
            // stg branch --list format: "s branch | description"
            // where s is ' ' or '>' for current
            let name = trimmed.trim_start_matches('>').trim();
            let name = name.split('|').next().unwrap_or("").trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect())
}

pub fn stg_branch_switch(name: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["branch", name])
}

pub fn stg_branch_create(name: &str) -> Result<(bool, String, String)> {
    run_cmd_ok("stg", &["branch", "--create", name])
}

// --- Remote operations ---

pub fn git_fetch() -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["fetch"])
}

pub fn git_push() -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["push"])
}

pub fn git_push_force() -> Result<(bool, String, String)> {
    run_cmd_ok("git", &["push", "--force-with-lease"])
}

pub fn stg_rebase(target: Option<&str>) -> Result<(bool, String, String)> {
    match target {
        Some(t) => run_cmd_ok("stg", &["rebase", t]),
        None => run_cmd_ok("stg", &["rebase"]),
    }
}
