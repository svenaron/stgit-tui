#[allow(dead_code)]
mod stgit;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, prelude::*};
use std::io;

use stgit::PatchStatus;

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
    pub marked: Vec<usize>,   // indices into state.patches
    pub expanded: Vec<usize>, // indices into state.patches that are expanded
    pub patch_files: std::collections::HashMap<usize, Vec<stgit::FileEntry>>,
    pub history_count: usize,
    pub show_unknown: bool,
    pub status_msg: String,
    should_quit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let history_count = 5;
        let state = stgit::load_state(history_count)?;
        let mut app = App {
            state,
            cursor: 0,
            lines: Vec::new(),
            marked: Vec::new(),
            expanded: Vec::new(),
            patch_files: std::collections::HashMap::new(),
            history_count,
            show_unknown: false,
            status_msg: String::new(),
            should_quit: false,
        };
        app.rebuild_lines();
        // Position cursor on Index header by default
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

        // Header
        lines.push(LineItem::Header);

        // History (reversed so oldest is first/top)
        for i in (0..self.state.history.len()).rev() {
            lines.push(LineItem::History(i));
        }

        // Applied patches (in stack order: bottom first, current/top last)
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

        // Index
        lines.push(LineItem::IndexHeader);
        for i in 0..self.state.index_files.len() {
            lines.push(LineItem::IndexFile(i));
        }

        // Work Tree
        lines.push(LineItem::WorkTreeHeader);
        for i in 0..self.state.worktree_files.len() {
            let f = &self.state.worktree_files[i];
            if !self.show_unknown && f.status == stgit::FileStatus::Untracked {
                continue;
            }
            lines.push(LineItem::WorkTreeFile(i));
        }

        // Unapplied patches
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

        // Footer
        lines.push(LineItem::Footer);

        self.lines = lines;
    }

    fn reload(&mut self) {
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

    fn current_line(&self) -> &LineItem {
        self.lines.get(self.cursor).unwrap_or(&LineItem::Header)
    }

    fn current_patch_index(&self) -> Option<usize> {
        match self.current_line().clone() {
            LineItem::Patch(i) => Some(i),
            LineItem::PatchFile(i, _) => Some(i),
            _ => None,
        }
    }

    fn effective_patches(&self) -> Vec<usize> {
        if !self.marked.is_empty() {
            self.marked.clone()
        } else if let Some(i) = self.current_patch_index() {
            vec![i]
        } else {
            vec![]
        }
    }

    fn patch_names(&self, indices: &[usize]) -> Vec<String> {
        indices
            .iter()
            .filter_map(|&i| self.state.patches.get(i))
            .map(|p| p.name.clone())
            .collect()
    }

    fn run_op(&mut self, result: Result<(bool, String, String)>) {
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

    fn handle_key(&mut self, key: KeyEvent) {
        self.status_msg.clear();

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Char('q')) => {
                self.should_quit = true;
            }
            // Navigation
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                if self.cursor + 1 < self.lines.len() {
                    self.cursor += 1;
                }
            }
            (KeyModifiers::NONE, KeyCode::Home) => {
                self.cursor = 0;
            }
            (KeyModifiers::NONE, KeyCode::End) => {
                self.cursor = self.lines.len().saturating_sub(1);
            }
            (KeyModifiers::NONE, KeyCode::PageUp) => {
                self.cursor = self.cursor.saturating_sub(20);
            }
            (KeyModifiers::NONE, KeyCode::PageDown) => {
                self.cursor = (self.cursor + 20).min(self.lines.len().saturating_sub(1));
            }

            // Reload
            (KeyModifiers::NONE, KeyCode::Char('g')) => {
                self.reload();
                self.status_msg = "Reloaded".to_string();
            }

            // Expand/collapse patch or open file
            (KeyModifiers::NONE, KeyCode::Enter) => {
                self.handle_enter();
            }

            // Toggle unknown files
            (KeyModifiers::NONE, KeyCode::Char('t')) => {
                self.show_unknown = !self.show_unknown;
                self.rebuild_lines();
            }

            // Mark/unmark
            (KeyModifiers::NONE, KeyCode::Char('m')) => {
                if let Some(i) = self.current_patch_index() {
                    if !self.marked.contains(&i) {
                        self.marked.push(i);
                    }
                    if self.cursor + 1 < self.lines.len() {
                        self.cursor += 1;
                    }
                }
            }
            (KeyModifiers::NONE, KeyCode::Char('u')) => {
                if let Some(i) = self.current_patch_index() {
                    self.marked.retain(|&x| x != i);
                }
            }

            // Refresh
            (KeyModifiers::NONE, KeyCode::Char('r')) => {
                let result = stgit::stg_refresh(None);
                self.run_op(result);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                if let Some(i) = self.current_patch_index() {
                    let name = self.state.patches[i].name.clone();
                    let result = stgit::stg_refresh(Some(&name));
                    self.run_op(result);
                }
            }

            // Goto
            (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                if let Some(i) = self.current_patch_index() {
                    let name = self.state.patches[i].name.clone();
                    let result = stgit::stg_goto(&name);
                    self.run_op(result);
                }
            }

            // Push next (> key)
            (_, KeyCode::Char('>')) | (KeyModifiers::SHIFT, KeyCode::Char('.')) => {
                let result = stgit::stg_push_one();
                self.run_op(result);
            }
            // Pop current (< key)
            (_, KeyCode::Char('<')) | (KeyModifiers::SHIFT, KeyCode::Char(',')) => {
                let result = stgit::stg_pop_current();
                self.run_op(result);
            }
            (KeyModifiers::SHIFT, KeyCode::Char('P')) => {
                let indices = self.effective_patches();
                if indices.is_empty() {
                    return;
                }
                let names = self.patch_names(&indices);
                let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                let first = &self.state.patches[indices[0]];
                let result = if first.status == PatchStatus::Unapplied {
                    stgit::stg_push(&refs)
                } else {
                    stgit::stg_pop(&refs)
                };
                self.run_op(result);
                self.marked.clear();
            }

            // Move patches
            (KeyModifiers::SHIFT, KeyCode::Char('M')) => {
                if self.marked.is_empty() {
                    self.status_msg = "Mark patches first with 'm'".to_string();
                    return;
                }
                let target = self.current_patch_index();
                let names = self.patch_names(&self.marked.clone());
                let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                let target_name = target.map(|i| self.state.patches[i].name.clone());
                let result = stgit::stg_sink(&refs, target_name.as_deref());
                self.run_op(result);
                self.marked.clear();
            }

            // New patch
            (KeyModifiers::SHIFT, KeyCode::Char('N')) => {
                let result = stgit::stg_new("New patch");
                self.run_op(result);
            }

            // Delete
            (KeyModifiers::SHIFT, KeyCode::Char('D')) => {
                let indices = self.effective_patches();
                if indices.is_empty() {
                    return;
                }
                let names = self.patch_names(&indices);
                let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                let result = stgit::stg_delete(&refs);
                self.run_op(result);
                self.marked.clear();
            }

            // Squash
            (KeyModifiers::SHIFT, KeyCode::Char('S')) => {
                if self.marked.len() < 2 {
                    self.status_msg = "Mark at least 2 patches to squash".to_string();
                    return;
                }
                let names = self.patch_names(&self.marked.clone());
                let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                let result = stgit::stg_squash(&refs);
                self.run_op(result);
                self.marked.clear();
            }

            // Stage/unstage
            (KeyModifiers::NONE, KeyCode::Char('i')) => match self.current_line().clone() {
                LineItem::WorkTreeFile(i) => {
                    let path = self.state.worktree_files[i].path.clone();
                    let result = stgit::git_stage(&path);
                    self.run_op(result);
                }
                LineItem::IndexFile(i) => {
                    let path = self.state.index_files[i].path.clone();
                    let result = stgit::git_unstage(&path);
                    self.run_op(result);
                }
                _ => {}
            },

            // Revert
            (KeyModifiers::SHIFT, KeyCode::Char('U')) => match self.current_line().clone() {
                LineItem::WorkTreeFile(i) => {
                    let path = self.state.worktree_files[i].path.clone();
                    let result = stgit::git_revert_worktree(&path);
                    self.run_op(result);
                }
                LineItem::IndexFile(i) => {
                    let path = self.state.index_files[i].path.clone();
                    let result = stgit::git_revert_index(&path);
                    self.run_op(result);
                }
                _ => {}
            },

            // Resolve conflict
            (KeyModifiers::SHIFT, KeyCode::Char('R')) => {
                if let LineItem::WorkTreeFile(i) = self.current_line().clone() {
                    let path = self.state.worktree_files[i].path.clone();
                    let result = stgit::git_resolve(&path);
                    self.run_op(result);
                }
            }

            // Undo/redo
            (KeyModifiers::CONTROL, KeyCode::Char('z')) => {
                let result = stgit::stg_undo(false);
                self.run_op(result);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
                let result = stgit::stg_redo();
                self.run_op(result);
            }

            // Show diff (placeholder)
            (KeyModifiers::NONE, KeyCode::Char('=')) => {
                self.status_msg = "Diff view not yet implemented".to_string();
            }

            // Help
            (KeyModifiers::NONE, KeyCode::Char('h') | KeyCode::Char('?')) => {
                self.status_msg = "q:quit g:reload r:refresh G:goto >/<:push/pop m/u:mark P:push/pop i:stage e:edit D:del S:squash".to_string();
            }

            _ => {}
        }
    }

    fn handle_enter(&mut self) {
        if let LineItem::Patch(i) = self.current_line().clone() {
            if self.expanded.contains(&i) {
                self.expanded.retain(|&x| x != i);
            } else {
                if !self.patch_files.contains_key(&i) {
                    let name = &self.state.patches[i].name;
                    match stgit::get_patch_files(name) {
                        Ok(files) => {
                            self.patch_files.insert(i, files);
                        }
                        Err(e) => {
                            self.status_msg = format!("Error: {e}");
                            return;
                        }
                    }
                }
                self.expanded.push(i);
            }
            self.rebuild_lines();
        }
    }
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new()?;

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if let Event::Key(key) = event::read()? {
            // Handle edit specially - need to leave TUI
            if key.code == KeyCode::Char('e') && key.modifiers == KeyModifiers::NONE {
                if let Some(i) = app.current_patch_index() {
                    let name = app.state.patches[i].name.clone();
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    terminal.show_cursor()?;

                    let result = stgit::stg_edit(&name);

                    enable_raw_mode()?;
                    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                    terminal.hide_cursor()?;
                    terminal.clear()?;

                    app.run_op(result);
                    continue;
                }
            }

            app.handle_key(key);
            if app.should_quit {
                break;
            }
        }
    }

    Ok(())
}
