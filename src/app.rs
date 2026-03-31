use std::collections::HashMap;

use anyhow::Result;

use crate::stgit::{self, FileStatus, PatchStatus};

/// What action to perform when an input prompt is submitted
#[derive(Debug, Clone)]
pub enum InputAction {
    NewPatch,
    CreatePatchFromChanges,
    HistorySize,
}

/// The current UI mode
#[derive(Debug)]
pub enum AppMode {
    Normal,
    DiffView {
        lines: Vec<String>,
        scroll: usize,
        title: String,
    },
    Input {
        prompt: String,
        value: String,
        action: InputAction,
    },
    Help,
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
