use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, LineItem};
use crate::stgit::{self, PatchStatus};

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
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
