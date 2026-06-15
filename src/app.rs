use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::config::Config;
use crate::keychain::Keychain;

#[derive(Clone, Copy, PartialEq)]
pub enum Focus {
    Projects,
    Keys,
}

pub enum Modal {
    AddKey { key_name: String, key_value: String, step: AddStep },
    ConfirmDeleteKey { key: String },
    ConfirmDeleteProject { project: String },
    Message { text: String, level: MsgLevel },
}

pub enum AddStep {
    KeyName,
    KeyValue,
}

#[derive(Clone, Copy, PartialEq)]
pub enum MsgLevel {
    Info,
    Success,
    Error,
}

pub struct App {
    pub config: Config,
    pub projects: Vec<String>,
    pub selected_project: Option<usize>,
    pub selected_key: Option<usize>,
    pub focus: Focus,
    pub keys: Vec<String>,
    pub key_values: Vec<(String, String)>,
    pub copied: Option<String>,
    pub modal: Option<Modal>,
    pub project_list_offset: usize,
    pub key_list_offset: usize,
    pub status_msg: Option<(String, MsgLevel)>,
    pub running: bool,
}

impl App {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let projects = config.list_projects()?;
        Ok(Self {
            config,
            projects,
            selected_project: None,
            selected_key: None,
            focus: Focus::Projects,
            keys: Vec::new(),
            key_values: Vec::new(),
            copied: None,
            modal: None,
            project_list_offset: 0,
            key_list_offset: 0,
            status_msg: None,
            running: true,
        })
    }

    pub fn select_project(&mut self, index: usize) {
        if index < self.projects.len() {
            self.selected_project = Some(index);
            self.selected_key = None;
            self.key_list_offset = 0;
            self.load_keys();
        }
    }

    fn load_keys(&mut self) {
        if let Some(idx) = self.selected_project {
            if idx < self.projects.len() {
                let project = &self.projects[idx];
                self.keys = self.config.list_keys(project).unwrap_or_default();
                self.key_values = Keychain::list_values(project, &self.keys);
            }
        }
    }

    /// Refresh the full project list (e.g. after adding a new project).
    pub fn refresh_projects(&mut self) {
        self.projects = self.config.list_projects().unwrap_or_default();
        self.load_keys();
    }

    /// Refresh keys for current project.
    pub fn refresh_keys(&mut self) {
        self.load_keys();
    }

    /// Handle a single event — returns Some to trigger redraw, None to exit.
    pub fn handle_event(&mut self) -> anyhow::Result<bool> {
        let ev = event::read()?;

        // If a modal is open, route to modal handler
        if self.modal.is_some() {
            self.handle_modal_event(ev);
            return Ok(true);
        }

        match ev {
            Event::Key(ke) if ke.kind == KeyEventKind::Press => {
                self.handle_key(ke)?;
            }
            Event::Resize(..) => {}
            _ => {}
        }

        Ok(self.running)
    }

    fn handle_key(&mut self, ke: KeyEvent) -> anyhow::Result<()> {
        match ke.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.running = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.navigate_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.navigate_down();
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Projects => Focus::Keys,
                    Focus::Keys => Focus::Projects,
                };
            }
            KeyCode::Enter => {
                self.handle_enter();
            }
            KeyCode::Char('c') if ke.modifiers == KeyModifiers::CONTROL => {
                // Ctrl+C — copy selected
                self.copy_selected();
            }
            KeyCode::Char('c') => {
                self.copy_selected();
            }
            KeyCode::Char('d') => {
                self.delete_selected();
            }
            KeyCode::Char('a') => {
                self.add_secret();
            }
            KeyCode::Char('p') if self.focus == Focus::Keys => {
                // Jump to project list
                self.focus = Focus::Projects;
            }
            KeyCode::Char('D') => {
                // Shift+D — delete entire project
                self.delete_project();
            }
            _ => {}
        }
        Ok(())
    }

    fn navigate_up(&mut self) {
        match self.focus {
            Focus::Projects => {
                if let Some(idx) = self.selected_project {
                    if idx > 0 {
                        self.select_project(idx - 1);
                    }
                } else if !self.projects.is_empty() {
                    self.select_project(self.projects.len() - 1);
                }
            }
            Focus::Keys => {
                if let Some(idx) = self.selected_key {
                    if idx > 0 {
                        self.selected_key = Some(idx - 1);
                    }
                } else if !self.key_values.is_empty() {
                    self.selected_key = Some(self.key_values.len() - 1);
                }
            }
        }
    }

    fn navigate_down(&mut self) {
        match self.focus {
            Focus::Projects => {
                let max = self.projects.len().saturating_sub(1);
                let next = self.selected_project.map(|i| i + 1).unwrap_or(0);
                if next <= max {
                    self.select_project(next);
                }
            }
            Focus::Keys => {
                let max = self.key_values.len().saturating_sub(1);
                let next = self.selected_key.map(|i| i + 1).unwrap_or(0);
                if next <= max {
                    self.selected_key = Some(next);
                }
            }
        }
    }

    fn handle_enter(&mut self) {
        match self.focus {
            Focus::Projects => {
                // Already selected via navigate. Focus keys.
                if self.selected_project.is_some() && !self.key_values.is_empty() {
                    self.focus = Focus::Keys;
                    self.selected_key = Some(0);
                }
            }
            Focus::Keys => {
                self.copy_selected();
            }
        }
    }

    fn copy_selected(&mut self) {
        let val = match self.focus {
            Focus::Keys => {
                self.selected_key.and_then(|i| self.key_values.get(i).map(|(_, v)| v.clone()))
            }
            Focus::Projects => {
                self.selected_project.and_then(|i| {
                    let _project = &self.projects[i];
                    // Copy the project name
                    None
                })
            }
        };

        if let Some(val) = val {
            // Use pbcopy
            let mut cmd = std::process::Command::new("pbcopy");
            cmd.stdin(std::process::Stdio::piped());
            if let Ok(mut child) = cmd.spawn() {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(val.as_bytes());
                }
                let _ = child.wait();
            }
            self.copied = Some(val.chars().take(16).collect());
            self.set_status("Copied to clipboard!", MsgLevel::Success);
        }
    }

    fn delete_selected(&mut self) {
        match self.focus {
            Focus::Keys => {
                if let Some(i) = self.selected_key {
                    if let Some((key, _)) = self.key_values.get(i) {
                        self.modal = Some(Modal::ConfirmDeleteKey { key: key.clone() });
                    }
                }
            }
            Focus::Projects => {
                // Ctrl+d on a project — delete whole project
                if self.selected_project.is_some() {
                    self.delete_project();
                }
            }
        }
    }

    fn delete_project(&mut self) {
        if let Some(i) = self.selected_project {
            if i < self.projects.len() {
                let project = self.projects[i].clone();
                self.modal = Some(Modal::ConfirmDeleteProject { project });
            }
        }
    }

    fn add_secret(&mut self) {
        // Determine which project to add to
        let project_idx = match self.focus {
            Focus::Keys => self.selected_project,
            Focus::Projects => self.selected_project,
        };

        if project_idx.is_some() {
            self.modal = Some(Modal::AddKey {
                key_name: String::new(),
                key_value: String::new(),
                step: AddStep::KeyName,
            });
        } else {
            self.set_status("Select a project first", MsgLevel::Error);
        }
    }

    fn handle_modal_event(&mut self, ev: Event) {
        let Some(modal) = self.modal.take() else { return };
        let new_modal = match modal {
            Modal::AddKey { mut key_name, mut key_value, step } => {
                self.handle_add_key_modal(ev, step, &mut key_name, &mut key_value)
            }
            Modal::ConfirmDeleteKey { key } => {
                self.handle_delete_key_modal(ev, &key)
            }
            Modal::ConfirmDeleteProject { project } => {
                self.handle_delete_project_modal(ev, &project)
            }
            Modal::Message { text, level } => {
                if matches!(ev, Event::Key(ke) if ke.kind == KeyEventKind::Press) {
                    None
                } else {
                    Some(Modal::Message { text, level })
                }
            }
        };
        self.modal = new_modal;
    }

    fn handle_add_key_modal(
        &mut self,
        ev: Event,
        step: AddStep,
        key_name: &mut String,
        key_value: &mut String,
    ) -> Option<Modal> {
        let Event::Key(ke) = ev else { return Some(Modal::AddKey {
            key_name: key_name.clone(),
            key_value: key_value.clone(),
            step,
        })};
        if ke.kind != KeyEventKind::Press {
            return Some(Modal::AddKey { key_name: key_name.clone(), key_value: key_value.clone(), step });
        }

        match step {
            AddStep::KeyName => {
                match ke.code {
                    KeyCode::Enter => {
                        if key_name.is_empty() {
                            return None; // cancel
                        }
                        return Some(Modal::AddKey {
                            key_name: key_name.clone(),
                            key_value: String::new(),
                            step: AddStep::KeyValue,
                        });
                    }
                    KeyCode::Esc => return None,
                    KeyCode::Char(ch) => {
                        // Uppercase env-var chars only
                        if ch.is_alphanumeric() || ch == '_' {
                            key_name.push(ch.to_ascii_uppercase());
                        }
                    }
                    KeyCode::Backspace => {
                        key_name.pop();
                    }
                    _ => {}
                }
                Some(Modal::AddKey {
                    key_name: key_name.clone(),
                    key_value: key_value.clone(),
                    step,
                })
            }
            AddStep::KeyValue => {
                match ke.code {
                    KeyCode::Enter => {
                        if key_value.is_empty() {
                            return None;
                        }
                        // Save it
                        if let Some(i) = self.selected_project {
                            if i < self.projects.len() {
                                let project = self.projects[i].clone();
                                if Keychain::set(&project, key_name, key_value).is_ok() {
                                    let _ = self.config.add_key(&project, key_name);
                                    self.refresh_keys();
                                    self.set_status(
                                        &format!("Saved {}:{}", project, key_name),
                                        MsgLevel::Success,
                                    );
                                } else {
                                    self.set_status("Failed to save secret", MsgLevel::Error);
                                }
                            }
                        }
                        return None;
                    }
                    KeyCode::Esc => return None,
                    KeyCode::Char(ch) => {
                        key_value.push(ch);
                    }
                    KeyCode::Backspace => {
                        key_value.pop();
                    }
                    _ => {}
                }
                return Some(Modal::AddKey {
                    key_name: key_name.clone(),
                    key_value: key_value.clone(),
                    step,
                });
            }
        }
    }

    fn handle_delete_key_modal(&mut self, ev: Event, key: &str) -> Option<Modal> {
        let Event::Key(ke) = ev else { return Some(Modal::ConfirmDeleteKey { key: key.to_string() }) };
        if ke.kind != KeyEventKind::Press {
            return Some(Modal::ConfirmDeleteKey { key: key.to_string() });
        }
        match ke.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(i) = self.selected_project {
                    if i < self.projects.len() {
                        let project = &self.projects[i];
                        let _ = Keychain::delete(project, key);
                        let _ = self.config.remove_key(project, key);
                        self.refresh_keys();
                        self.selected_key = None;
                        self.set_status(&format!("Deleted {}", key), MsgLevel::Info);
                    }
                }
                None
            }
            KeyCode::Char('n') | KeyCode::Esc => None,
            _ => Some(Modal::ConfirmDeleteKey { key: key.to_string() }),
        }
    }

    fn handle_delete_project_modal(&mut self, ev: Event, project: &str) -> Option<Modal> {
        let Event::Key(ke) = ev else { return Some(Modal::ConfirmDeleteProject { project: project.to_string() }) };
        if ke.kind != KeyEventKind::Press {
            return Some(Modal::ConfirmDeleteProject { project: project.to_string() });
        }
        match ke.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                let keys = self.config.list_keys(project).unwrap_or_default();
                for key in &keys {
                    let _ = Keychain::delete(project, key);
                }
                let _ = self.config.remove_project(project);
                self.refresh_projects();
                self.selected_project = None;
                self.selected_key = None;
                self.key_values.clear();
                self.set_status(&format!("Removed project {}", project), MsgLevel::Info);
                None
            }
            KeyCode::Char('n') | KeyCode::Esc => None,
            _ => Some(Modal::ConfirmDeleteProject { project: project.to_string() }),
        }
    }

    fn set_status(&mut self, msg: &str, level: MsgLevel) {
        self.status_msg = Some((msg.to_string(), level));
    }

    /// Get selected project name.
    pub fn selected_project_name(&self) -> Option<&str> {
        self.selected_project.map(|i| &self.projects[i][..])
    }

    /// Get selected key name.
    pub fn selected_key_name(&self) -> Option<&str> {
        self.selected_key.and_then(|i| self.key_values.get(i).map(|(k, _)| k.as_str()))
    }
}
