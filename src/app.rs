use std::collections::HashMap;

use anyhow::Result;

use crate::stgit::{self, FileStatus, PatchStatus};

/// What action to perform when an input prompt is submitted
#[derive(Debug, Clone)]
pub enum InputAction {
    NewPatch,
    CreatePatchFromChanges,
    HistorySize,
    BranchCreate,
    ConfirmPush,
    ConfirmForcePush,
}

/// Where the diff came from — needed to know how to stage/unstage
#[derive(Debug, Clone)]
pub enum DiffSource {
    WorkTree { path: String },
    Index { path: String },
    Patch { name: String },
}

/// A parsed hunk from a unified diff
#[derive(Debug, Clone)]
pub struct DiffHunk {
    /// Line index in the full diff where this hunk starts (the @@ line)
    pub start_line: usize,
    /// Line index where this hunk ends (exclusive)
    pub end_line: usize,
}

/// State for the diff viewer with hunk awareness
#[derive(Debug)]
pub struct DiffViewState {
    pub lines: Vec<String>,
    /// The file header lines (diff --git, ---, +++) that precede hunks
    pub file_headers: Vec<(usize, usize)>, // (start, end) pairs for each file's headers
    pub hunks: Vec<DiffHunk>,
    pub scroll: usize,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
    pub title: String,
    pub source: DiffSource,
}

impl DiffViewState {
    pub fn from_diff(diff: &str, title: String, source: DiffSource) -> Self {
        let lines: Vec<String> = diff.lines().map(|l| l.to_string()).collect();
        let mut hunks = Vec::new();
        let mut file_headers: Vec<(usize, usize)> = Vec::new();
        let mut current_header_start: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            if line.starts_with("diff ") {
                // Start of a new file section
                current_header_start = Some(i);
            } else if line.starts_with("@@") {
                // End of file headers, start of a hunk
                if let Some(hs) = current_header_start.take() {
                    file_headers.push((hs, i));
                }
                let hunk_start = i;
                // Find where this hunk ends
                let hunk_end = lines[i + 1..]
                    .iter()
                    .position(|l| l.starts_with("@@") || l.starts_with("diff "))
                    .map(|p| i + 1 + p)
                    .unwrap_or(lines.len());
                hunks.push(DiffHunk {
                    start_line: hunk_start,
                    end_line: hunk_end,
                });
            }
        }

        DiffViewState {
            lines,
            file_headers,
            hunks,
            scroll: 0,
            cursor: 0,
            selection_anchor: None,
            title,
            source,
        }
    }

    /// Get the index of the hunk containing the cursor line
    pub fn current_hunk_index(&self) -> Option<usize> {
        self.hunks
            .iter()
            .position(|h| self.cursor >= h.start_line && self.cursor < h.end_line)
    }

    /// Get the selected line range (inclusive)
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|anchor| {
            let start = anchor.min(self.cursor);
            let end = anchor.max(self.cursor);
            (start, end)
        })
    }

    /// Build a diff fragment for a single hunk, including file headers
    pub fn hunk_diff(&self, hunk_idx: usize) -> Option<String> {
        let hunk = self.hunks.get(hunk_idx)?;
        let mut result = String::new();

        // Find the file headers that apply to this hunk
        for &(hs, he) in &self.file_headers {
            if hs <= hunk.start_line {
                result.clear(); // reset — use the most recent headers
                for line in &self.lines[hs..he] {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        // Add hunk lines
        for line in &self.lines[hunk.start_line..hunk.end_line] {
            result.push_str(line);
            result.push('\n');
        }

        Some(result)
    }

    /// Build a diff fragment for selected lines within a hunk
    pub fn selection_diff(&self) -> Option<String> {
        let (sel_start, sel_end) = self.selection_range()?;
        let hunk_idx = self.current_hunk_index()?;
        let hunk = &self.hunks[hunk_idx];

        let mut result = String::new();

        // File headers
        for &(hs, he) in &self.file_headers {
            if hs <= hunk.start_line {
                result.clear();
                for line in &self.lines[hs..he] {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        // Build a modified hunk that only includes selected +/- lines
        // The @@ line needs to be recalculated
        let hunk_header = &self.lines[hunk.start_line];

        // Parse the original @@ header to get the starting line number
        let (old_start, _old_count, new_start, _new_count) = parse_hunk_header(hunk_header);

        let mut new_lines = Vec::new();
        let mut old_line_count = 0u32;
        let mut new_line_count = 0u32;

        for i in (hunk.start_line + 1)..hunk.end_line {
            let line = &self.lines[i];
            let in_selection = i >= sel_start && i <= sel_end;
            let first_char = line.chars().next().unwrap_or(' ');

            match first_char {
                '+' => {
                    if in_selection {
                        new_lines.push(line.clone());
                        new_line_count += 1;
                    }
                    // If not selected, skip it entirely
                }
                '-' => {
                    if in_selection {
                        new_lines.push(line.clone());
                        old_line_count += 1;
                    } else {
                        // Convert to context line (keep the line but as unchanged)
                        new_lines.push(format!(" {}", &line[1..]));
                        old_line_count += 1;
                        new_line_count += 1;
                    }
                }
                _ => {
                    // Context line — always include
                    new_lines.push(line.clone());
                    old_line_count += 1;
                    new_line_count += 1;
                }
            }
        }

        // Write the new @@ header
        result.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start, old_line_count, new_start, new_line_count
        ));

        for line in &new_lines {
            result.push_str(line);
            result.push('\n');
        }

        Some(result)
    }
}

fn parse_hunk_header(header: &str) -> (u32, u32, u32, u32) {
    // Parse "@@ -old_start,old_count +new_start,new_count @@"
    let parts: Vec<&str> = header.split_whitespace().collect();
    let mut old_start = 1u32;
    let mut old_count = 1u32;
    let mut new_start = 1u32;
    let mut new_count = 1u32;

    if parts.len() >= 3 {
        if let Some(old) = parts[1].strip_prefix('-') {
            let nums: Vec<&str> = old.split(',').collect();
            old_start = nums[0].parse().unwrap_or(1);
            if nums.len() > 1 {
                old_count = nums[1].parse().unwrap_or(1);
            }
        }
        if let Some(new) = parts[2].strip_prefix('+') {
            let nums: Vec<&str> = new.split(',').collect();
            new_start = nums[0].parse().unwrap_or(1);
            if nums.len() > 1 {
                new_count = nums[1].parse().unwrap_or(1);
            }
        }
    }

    (old_start, old_count, new_start, new_count)
}

/// The current UI mode
#[derive(Debug)]
pub enum AppMode {
    Normal,
    DiffView(DiffViewState),
    Input {
        prompt: String,
        value: String,
        action: InputAction,
    },
    Help,
    BranchList {
        branches: Vec<String>,
        selected: usize,
    },
}

/// What kind of line the cursor is on
#[derive(Debug, Clone)]
pub enum LineItem {
    Header,
    History(usize),
    Patch(usize),
    PatchFile(usize, usize), // patch index, file index
    IndexHeader,
    IndexFile(usize),
    WorkTreeHeader,
    WorkTreeFile(usize),
    Footer,
}

pub struct App {
    pub state: stgit::StackState,
    pub cursor: usize,
    pub lines: Vec<LineItem>,
    pub marked: Vec<usize>,
    pub expanded: Vec<usize>,
    pub patch_files: HashMap<usize, Vec<stgit::FileEntry>>,
    pub history_count: usize,
    pub show_unknown: bool,
    pub status_msg: String,
    pub should_quit: bool,
    pub mode: AppMode,
}

impl App {
    pub fn new() -> Result<Self> {
        let history_count = 5;
        let state = stgit::load_state(history_count)?;
        let mut app = App {
            state,
            cursor: 0,
            lines: Vec::new(),
            marked: Vec::new(),
            expanded: Vec::new(),
            patch_files: HashMap::new(),
            history_count,
            show_unknown: false,
            status_msg: String::new(),
            should_quit: false,
            mode: AppMode::Normal,
        };
        app.rebuild_lines();
        app.cursor = app.find_index_header().unwrap_or(0);
        Ok(app)
    }

    fn find_index_header(&self) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| matches!(l, LineItem::IndexHeader))
    }

    pub fn rebuild_lines(&mut self) {
        let mut lines = Vec::new();

        lines.push(LineItem::Header);

        for i in (0..self.state.history.len()).rev() {
            lines.push(LineItem::History(i));
        }

        let applied: Vec<usize> = self
            .state
            .patches
            .iter()
            .enumerate()
            .filter(|(_, p)| p.status == PatchStatus::Applied || p.status == PatchStatus::Current)
            .map(|(i, _)| i)
            .collect();
        for &i in &applied {
            lines.push(LineItem::Patch(i));
            if self.expanded.contains(&i) {
                if let Some(files) = self.patch_files.get(&i) {
                    for fi in 0..files.len() {
                        lines.push(LineItem::PatchFile(i, fi));
                    }
                }
            }
        }

        lines.push(LineItem::IndexHeader);
        for i in 0..self.state.index_files.len() {
            lines.push(LineItem::IndexFile(i));
        }

        lines.push(LineItem::WorkTreeHeader);
        for i in 0..self.state.worktree_files.len() {
            let f = &self.state.worktree_files[i];
            if !self.show_unknown && f.status == FileStatus::Untracked {
                continue;
            }
            lines.push(LineItem::WorkTreeFile(i));
        }

        let unapplied: Vec<usize> = self
            .state
            .patches
            .iter()
            .enumerate()
            .filter(|(_, p)| p.status == PatchStatus::Unapplied)
            .map(|(i, _)| i)
            .collect();
        for &i in &unapplied {
            lines.push(LineItem::Patch(i));
            if self.expanded.contains(&i) {
                if let Some(files) = self.patch_files.get(&i) {
                    for fi in 0..files.len() {
                        lines.push(LineItem::PatchFile(i, fi));
                    }
                }
            }
        }

        lines.push(LineItem::Footer);
        self.lines = lines;
    }

    pub fn reload(&mut self) {
        match stgit::load_state(self.history_count) {
            Ok(state) => {
                self.state = state;
                self.patch_files.clear();
                self.expanded.clear();
                self.rebuild_lines();
                if self.cursor >= self.lines.len() {
                    self.cursor = self.lines.len().saturating_sub(1);
                }
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
            }
        }
    }

    pub fn current_line(&self) -> &LineItem {
        self.lines.get(self.cursor).unwrap_or(&LineItem::Header)
    }

    pub fn current_patch_index(&self) -> Option<usize> {
        match self.current_line() {
            LineItem::Patch(i) | LineItem::PatchFile(i, _) => Some(*i),
            _ => None,
        }
    }

    pub fn effective_patches(&self) -> Vec<usize> {
        if !self.marked.is_empty() {
            self.marked.clone()
        } else if let Some(i) = self.current_patch_index() {
            vec![i]
        } else {
            vec![]
        }
    }

    pub fn patch_names(&self, indices: &[usize]) -> Vec<String> {
        indices
            .iter()
            .filter_map(|&i| self.state.patches.get(i))
            .map(|p| p.name.clone())
            .collect()
    }

    pub fn run_op(&mut self, result: Result<(bool, String, String)>) {
        match result {
            Ok((true, stdout, _)) => {
                self.status_msg = stdout.lines().next().unwrap_or("OK").to_string();
                self.reload();
            }
            Ok((false, _, stderr)) => {
                self.status_msg = format!("Error: {}", stderr.lines().next().unwrap_or("failed"));
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
            }
        }
    }
}
