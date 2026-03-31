use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, InputAction, LineItem};
use crate::stgit::{self, PatchStatus};

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
        match &self.mode {
            AppMode::Normal => self.handle_normal_key(key),
            AppMode::DiffView { .. } => self.handle_diff_key(key),
            AppMode::Input { .. } => self.handle_input_key(key),
            AppMode::Help => self.handle_help_key(key),
            AppMode::BranchList { .. } => self.handle_branch_list_key(key),
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
    }

    fn handle_diff_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let AppMode::DiffView { scroll, lines, .. } = &mut self.mode {
                    if *scroll + 1 < lines.len() {
                        *scroll += 1;
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let AppMode::DiffView { scroll, .. } = &mut self.mode {
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::PageDown => {
                if let AppMode::DiffView { scroll, lines, .. } = &mut self.mode {
                    *scroll = (*scroll + 20).min(lines.len().saturating_sub(1));
                }
            }
            KeyCode::PageUp => {
                if let AppMode::DiffView { scroll, .. } = &mut self.mode {
                    *scroll = scroll.saturating_sub(20);
                }
            }
            KeyCode::Home => {
                if let AppMode::DiffView { scroll, .. } = &mut self.mode {
                    *scroll = 0;
                }
            }
            KeyCode::End => {
                if let AppMode::DiffView { scroll, lines, .. } = &mut self.mode {
                    *scroll = lines.len().saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Enter => {
                // Extract action and value, then process
                let (action, value) = if let AppMode::Input { action, value, .. } = &self.mode {
                    (action.clone(), value.clone())
                } else {
                    return;
                };
                self.mode = AppMode::Normal;
                self.submit_input(action, &value);
            }
            KeyCode::Backspace => {
                if let AppMode::Input { value, .. } = &mut self.mode {
                    value.pop();
                }
            }
            KeyCode::Char(c) => {
                if let AppMode::Input { value, .. } = &mut self.mode {
                    value.push(c);
                }
            }
            _ => {}
        }
    }

    fn submit_input(&mut self, action: InputAction, value: &str) {
        match action {
            InputAction::NewPatch => {
                let msg = if value.is_empty() { "New patch" } else { value };
                let result = stgit::stg_new(msg);
                self.run_op(result);
            }
            InputAction::CreatePatchFromChanges => {
                let msg = if value.is_empty() { "New patch" } else { value };
                let result = stgit::stg_new(msg);
                self.run_op(result);
                // Also refresh to absorb current changes
                let result = stgit::stg_refresh(None);
                self.run_op(result);
            }
            InputAction::HistorySize => {
                if let Ok(n) = value.parse::<usize>() {
                    self.history_count = n;
                    self.reload();
                } else {
                    self.status_msg = "Invalid number".to_string();
                }
            }
            InputAction::BranchCreate => {
                if !value.is_empty() {
                    let result = stgit::stg_branch_create(value);
                    self.run_op(result);
                }
            }
            InputAction::ConfirmPush => {
                if value == "y" || value == "Y" {
                    let result = stgit::git_push();
                    self.run_op(result);
                }
            }
            InputAction::ConfirmForcePush => {
                if value == "y" || value == "Y" {
                    let result = stgit::git_push_force();
                    self.run_op(result);
                }
            }
        }
    }

    fn handle_branch_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let AppMode::BranchList { selected, .. } = &mut self.mode {
                    *selected = selected.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let AppMode::BranchList {
                    selected, branches, ..
                } = &mut self.mode
                {
                    if *selected + 1 < branches.len() {
                        *selected += 1;
                    }
                }
            }
            KeyCode::Enter => {
                let name = if let AppMode::BranchList {
                    selected, branches, ..
                } = &self.mode
                {
                    branches.get(*selected).cloned()
                } else {
                    None
                };
                self.mode = AppMode::Normal;
                if let Some(name) = name {
                    let result = stgit::stg_branch_switch(&name);
                    self.run_op(result);
                }
            }
            KeyCode::Char('n') => {
                self.mode = AppMode::Input {
                    prompt: "New branch name: ".to_string(),
                    value: String::new(),
                    action: InputAction::BranchCreate,
                };
            }
            _ => {}
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
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

            // Expand/collapse
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

            // New patch (with prompt)
            (KeyModifiers::SHIFT, KeyCode::Char('N')) => {
                self.mode = AppMode::Input {
                    prompt: "Patch message: ".to_string(),
                    value: String::new(),
                    action: InputAction::NewPatch,
                };
            }

            // Create patch from changes
            (KeyModifiers::NONE, KeyCode::Char('c')) => {
                self.mode = AppMode::Input {
                    prompt: "New patch message: ".to_string(),
                    value: String::new(),
                    action: InputAction::CreatePatchFromChanges,
                };
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

            // Commit/uncommit
            (KeyModifiers::SHIFT, KeyCode::Char('C')) => {
                self.handle_commit_uncommit();
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

            // Repair
            (KeyModifiers::NONE, KeyCode::Char('!')) => {
                let result = stgit::stg_repair();
                self.run_op(result);
            }

            // History size
            (KeyModifiers::SHIFT, KeyCode::Char('H')) => {
                self.mode = AppMode::Input {
                    prompt: format!("History size [{}]: ", self.history_count),
                    value: String::new(),
                    action: InputAction::HistorySize,
                };
            }

            // Show diff
            (KeyModifiers::NONE, KeyCode::Char('=')) => {
                self.open_diff_view();
            }

            // Branch list
            (KeyModifiers::NONE, KeyCode::Char('b')) => match stgit::stg_branch_list() {
                Ok(branches) => {
                    self.mode = AppMode::BranchList {
                        branches,
                        selected: 0,
                    };
                }
                Err(e) => {
                    self.status_msg = format!("Error: {e}");
                }
            },

            // Fetch
            (KeyModifiers::NONE, KeyCode::Char('f')) => {
                self.status_msg = "Fetching...".to_string();
                let result = stgit::git_fetch();
                self.run_op(result);
            }

            // Push (with confirmation)
            (KeyModifiers::NONE, KeyCode::Char('p')) => {
                self.mode = AppMode::Input {
                    prompt: "Push to remote? (y/n): ".to_string(),
                    value: String::new(),
                    action: InputAction::ConfirmPush,
                };
            }
            (KeyModifiers::SHIFT, KeyCode::Char('F')) => {
                self.mode = AppMode::Input {
                    prompt: "Force push? (y/n): ".to_string(),
                    value: String::new(),
                    action: InputAction::ConfirmForcePush,
                };
            }

            // Rebase
            (KeyModifiers::SHIFT, KeyCode::Char('B')) => {
                let result = stgit::stg_rebase(None);
                self.run_op(result);
            }

            // Help
            (KeyModifiers::NONE, KeyCode::Char('?')) => {
                self.mode = AppMode::Help;
            }
            (KeyModifiers::NONE, KeyCode::Char('h')) => {
                self.mode = AppMode::Help;
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

    fn open_diff_view(&mut self) {
        let result = match self.current_line().clone() {
            LineItem::Patch(i) | LineItem::PatchFile(i, _) => {
                let name = self.state.patches[i].name.clone();
                stgit::stg_diff(&name).map(|d| (d, format!("Patch: {name}")))
            }
            LineItem::IndexFile(i) => {
                let path = self.state.index_files[i].path.clone();
                stgit::git_diff(&path, true).map(|d| (d, format!("Index: {path}")))
            }
            LineItem::WorkTreeFile(i) => {
                let path = self.state.worktree_files[i].path.clone();
                stgit::git_diff(&path, false).map(|d| (d, format!("WorkTree: {path}")))
            }
            _ => return,
        };

        match result {
            Ok((diff, title)) => {
                let lines: Vec<String> = diff.lines().map(|l| l.to_string()).collect();
                if lines.is_empty() {
                    self.status_msg = "No diff to show".to_string();
                } else {
                    self.mode = AppMode::DiffView {
                        lines,
                        scroll: 0,
                        title,
                    };
                }
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
            }
        }
    }

    fn handle_commit_uncommit(&mut self) {
        match self.current_line().clone() {
            LineItem::Patch(i) => {
                let patch = &self.state.patches[i];
                if patch.status == PatchStatus::Applied || patch.status == PatchStatus::Current {
                    let indices = self.effective_patches();
                    let names = self.patch_names(&indices);
                    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                    let result = stgit::stg_commit(&refs);
                    self.run_op(result);
                    self.marked.clear();
                }
            }
            LineItem::History(i) => {
                // Uncommit: i is index from newest (0) to oldest
                let count = i + 1;
                let result = stgit::stg_uncommit(count);
                self.run_op(result);
            }
            _ => {}
        }
    }
}
