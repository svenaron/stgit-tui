use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, DiffSource, DiffViewState, InputAction, LineItem};
use crate::stgit::{self, PatchStatus};

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
        match &self.mode {
            AppMode::Normal => self.handle_normal_key(key),
            AppMode::DiffView { .. } => self.handle_diff_key(key),
            AppMode::Input { .. } => self.handle_input_key(key),
            AppMode::Help => self.handle_help_key(key),
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
            // Cursor movement
            KeyCode::Down | KeyCode::Char('j') => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    if dv.cursor + 1 < dv.lines.len() {
                        dv.cursor += 1;
                    }
                    Self::scroll_to_cursor(dv);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    dv.cursor = dv.cursor.saturating_sub(1);
                    Self::scroll_to_cursor(dv);
                }
            }
            KeyCode::PageDown => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    dv.cursor = (dv.cursor + 20).min(dv.lines.len().saturating_sub(1));
                    Self::scroll_to_cursor(dv);
                }
            }
            KeyCode::PageUp => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    dv.cursor = dv.cursor.saturating_sub(20);
                    Self::scroll_to_cursor(dv);
                }
            }
            KeyCode::Home => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    dv.cursor = 0;
                    dv.scroll = 0;
                }
            }
            KeyCode::End => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    dv.cursor = dv.lines.len().saturating_sub(1);
                    Self::scroll_to_cursor(dv);
                }
            }

            // Hunk navigation
            KeyCode::Char('n') => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    let current = dv.current_hunk_index();
                    let next = current.map(|i| i + 1).unwrap_or(0);
                    if let Some(hunk) = dv.hunks.get(next) {
                        dv.cursor = hunk.start_line;
                        dv.selection_anchor = None;
                        Self::scroll_to_cursor(dv);
                    }
                }
            }
            KeyCode::Char('p') => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    let current = dv.current_hunk_index().unwrap_or(0);
                    let prev = current.saturating_sub(1);
                    if let Some(hunk) = dv.hunks.get(prev) {
                        dv.cursor = hunk.start_line;
                        dv.selection_anchor = None;
                        Self::scroll_to_cursor(dv);
                    }
                }
            }

            // Selection toggle (v to start/stop selection)
            KeyCode::Char('v') => {
                if let AppMode::DiffView(dv) = &mut self.mode {
                    if dv.selection_anchor.is_some() {
                        dv.selection_anchor = None;
                    } else {
                        dv.selection_anchor = Some(dv.cursor);
                    }
                }
            }

            // Stage hunk or selection
            KeyCode::Char('s') => {
                self.diff_apply(false, false);
            }
            // Unstage hunk or selection
            KeyCode::Char('u') => {
                self.diff_apply(true, true);
            }
            // Revert hunk in worktree
            KeyCode::Char('r') => {
                self.diff_apply(false, true);
            }

            _ => {}
        }
    }

    fn scroll_to_cursor(dv: &mut DiffViewState) {
        // Keep cursor visible (assuming ~40 line viewport, we'll use scroll offset)
        if dv.cursor < dv.scroll {
            dv.scroll = dv.cursor;
        }
        // We don't know the viewport height here, but we'll ensure scroll <= cursor
        // The UI will handle the other direction
    }

    fn diff_apply(&mut self, cached: bool, reverse: bool) {
        let (diff_fragment, source) = if let AppMode::DiffView(dv) = &self.mode {
            let fragment = if dv.selection_anchor.is_some() {
                dv.selection_diff()
            } else {
                dv.current_hunk_index().and_then(|i| dv.hunk_diff(i))
            };
            (fragment, dv.source.clone())
        } else {
            return;
        };

        let Some(diff) = diff_fragment else {
            self.status_msg = "No hunk at cursor".to_string();
            return;
        };

        // Determine flags based on source and operation
        let (use_cached, use_reverse) = match &source {
            DiffSource::WorkTree { .. } => (cached, reverse),
            DiffSource::Index { .. } => (true, reverse),
            DiffSource::Patch { .. } => {
                self.status_msg = "Cannot stage/unstage patch hunks".to_string();
                return;
            }
        };

        let result = stgit::git_apply(&diff, use_cached, use_reverse);
        match result {
            Ok((true, _, _)) => {
                self.status_msg = "Applied".to_string();
                // Clear selection
                if let AppMode::DiffView(dv) = &mut self.mode {
                    dv.selection_anchor = None;
                }
                // Refresh the diff view
                self.refresh_diff_view();
            }
            Ok((false, _, stderr)) => {
                self.status_msg =
                    format!("Error: {}", stderr.lines().next().unwrap_or("apply failed"));
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
            }
        }
    }

    fn refresh_diff_view(&mut self) {
        let (source, old_cursor) = if let AppMode::DiffView(dv) = &self.mode {
            (dv.source.clone(), dv.cursor)
        } else {
            return;
        };

        let result = match &source {
            DiffSource::WorkTree { path } => {
                stgit::git_diff(path, false).map(|d| (d, format!("WorkTree: {path}")))
            }
            DiffSource::Index { path } => {
                stgit::git_diff(path, true).map(|d| (d, format!("Index: {path}")))
            }
            DiffSource::Patch { name } => {
                stgit::stg_diff(name).map(|d| (d, format!("Patch: {name}")))
            }
        };

        match result {
            Ok((diff, title)) => {
                if diff.trim().is_empty() {
                    self.mode = AppMode::Normal;
                    self.status_msg = "Diff is now empty".to_string();
                    self.reload();
                } else {
                    let mut dv = DiffViewState::from_diff(&diff, title, source);
                    dv.cursor = old_cursor.min(dv.lines.len().saturating_sub(1));
                    Self::scroll_to_cursor(&mut dv);
                    self.mode = AppMode::DiffView(dv);
                }
            }
            Err(e) => {
                self.mode = AppMode::Normal;
                self.status_msg = format!("Error refreshing: {e}");
            }
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Enter => {
                let (action, value) = if let AppMode::Input { action, value, .. } = &self.mode {
                    (action.clone(), value.clone())
                } else {
                    return;
                };
                self.mode = AppMode::Normal;
                self.submit_input(action, &value);
            }
            KeyCode::Backspace => {
                if let AppMode::Input {
                    value,
                    completion_idx,
                    filter_text,
                    ..
                } = &mut self.mode
                {
                    value.pop();
                    *completion_idx = None;
                    *filter_text = None;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let forward = key.code == KeyCode::Tab;
                if let AppMode::Input {
                    value,
                    completions,
                    completion_idx,
                    filter_text,
                    ..
                } = &mut self.mode
                {
                    // On first Tab, save what the user typed as the filter
                    let query = filter_text.get_or_insert_with(|| value.clone());
                    let query_lower = query.to_lowercase();

                    let matches: Vec<usize> = completions
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| {
                            query.is_empty() || c.to_lowercase().contains(&query_lower)
                        })
                        .map(|(i, _)| i)
                        .collect();

                    if !matches.is_empty() {
                        let pick = match completion_idx {
                            Some(current) => if forward {
                                matches.iter().find(|&&i| i > *current).or(matches.first())
                            } else {
                                matches
                                    .iter()
                                    .rev()
                                    .find(|&&i| i < *current)
                                    .or(matches.last())
                            }
                            .copied()
                            .unwrap(),
                            None => {
                                if forward {
                                    matches[0]
                                } else {
                                    *matches.last().unwrap()
                                }
                            }
                        };
                        *value = completions[pick].clone();
                        *completion_idx = Some(pick);
                    }
                }
            }
            KeyCode::Char(c) => {
                if let AppMode::Input {
                    value,
                    completion_idx,
                    filter_text,
                    ..
                } = &mut self.mode
                {
                    value.push(c);
                    *completion_idx = None;
                    *filter_text = None;
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
            InputAction::Rebase => {
                if !value.is_empty() {
                    let result = stgit::stg_rebase(Some(value));
                    self.run_op(result);
                }
            }
            InputAction::BranchSwitch => {
                if !value.is_empty() {
                    let result = stgit::stg_branch_switch(value);
                    self.run_op(result);
                }
            }
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
                self.mode = AppMode::input("Patch message: ", InputAction::NewPatch);
            }

            // Create patch from changes
            (KeyModifiers::NONE, KeyCode::Char('c')) => {
                self.mode =
                    AppMode::input("New patch message: ", InputAction::CreatePatchFromChanges);
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
                self.mode = AppMode::input(
                    format!("History size [{}]: ", self.history_count),
                    InputAction::HistorySize,
                );
            }

            // Show diff
            (KeyModifiers::NONE, KeyCode::Char('=')) => {
                self.open_diff_view();
            }

            // Switch branch
            (KeyModifiers::NONE, KeyCode::Char('b')) => {
                let branches = stgit::stg_branch_list().unwrap_or_default();
                self.mode = AppMode::input_with_completions(
                    "Switch branch: ",
                    "",
                    InputAction::BranchSwitch,
                    branches,
                );
            }

            // Fetch
            (KeyModifiers::NONE, KeyCode::Char('f')) => {
                self.status_msg = "Fetching...".to_string();
                let result = stgit::git_fetch();
                self.run_op(result);
            }

            // Push (with confirmation)
            (KeyModifiers::NONE, KeyCode::Char('p')) => {
                self.mode = AppMode::input("Push to remote? (y/n): ", InputAction::ConfirmPush);
            }
            (KeyModifiers::SHIFT, KeyCode::Char('F')) => {
                self.mode = AppMode::input("Force push? (y/n): ", InputAction::ConfirmForcePush);
            }

            // Rebase
            (KeyModifiers::SHIFT, KeyCode::Char('B')) => {
                // Get upstream as default, and all branches for completion
                let upstream = self.state.branch.upstream.clone().unwrap_or_default();
                let branches = stgit::git_branch_list().unwrap_or_default();
                self.mode = AppMode::input_with_completions(
                    "Rebase onto: ",
                    upstream,
                    InputAction::Rebase,
                    branches,
                );
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
                let source = DiffSource::Patch { name: name.clone() };
                stgit::stg_diff(&name).map(|d| (d, format!("Patch: {name}"), source))
            }
            LineItem::IndexFile(i) => {
                let path = self.state.index_files[i].path.clone();
                let source = DiffSource::Index { path: path.clone() };
                stgit::git_diff(&path, true).map(|d| (d, format!("Index: {path}"), source))
            }
            LineItem::WorkTreeFile(i) => {
                let path = self.state.worktree_files[i].path.clone();
                let source = DiffSource::WorkTree { path: path.clone() };
                stgit::git_diff(&path, false).map(|d| (d, format!("WorkTree: {path}"), source))
            }
            _ => return,
        };

        match result {
            Ok((diff, title, source)) => {
                if diff.trim().is_empty() {
                    self.status_msg = "No diff to show".to_string();
                } else {
                    self.mode = AppMode::DiffView(DiffViewState::from_diff(&diff, title, source));
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
