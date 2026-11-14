//! Home and the projects open in the window: switching between them,
//! starting, saving, closing, deleting one.

use super::{App, COLS, Closing, OpenProject, ROWS, Workspace};
use crate::project::{self, Project, TabRecipe, When};
use crate::sidebar::Live;
use crate::start::{self, Start};
use crate::tab::{Tab, TabId};

impl App {
    /// A tab out of sight ended (`exit` in it, or its command closed the shell).
    /// Its project's last tab: the project closes, its file stays as it was.
    pub(super) fn remove_parked_tab(&mut self, id: TabId) {
        for (w, workspace) in self.parked.iter_mut().enumerate() {
            let Some(index) = workspace.tabs.iter().position(|tab| tab.id == id) else {
                continue;
            };
            workspace.tabs.remove(index);
            if workspace.tabs.is_empty() {
                self.parked.remove(w);
            } else if index < workspace.active || workspace.active == workspace.tabs.len() {
                workspace.active -= 1;
            }
            self.request_redraw(); // the sidebar may show it
            return;
        }
    }

    /// The last tab on screen ended. A project's: back home, the home that was
    /// left or a new one; its file isn't written, it stays as it was, not a
    /// project of no tabs (see `save_project`).
    /// Home's: the project you were in last comes on screen. A tab closed is a
    /// small thing: it doesn't end the other projects' shells. Home comes back,
    /// a new one, when you go there. Nothing else open: Kel closes, as a
    /// terminal does with its last tab.
    /// Returns `false` when Kel should close.
    pub(super) fn leave_empty(&mut self) -> bool {
        if self.project.is_some() {
            let Ok(home) = self.home_workspace() else {
                return false;
            };
            self.enter(home);
            return true;
        }
        // In home, only projects are out of sight; the last one left is last.
        let Some(workspace) = self.parked.pop() else {
            return false;
        };
        self.enter(workspace);
        true
    }

    /// Home's tabs: the ones left out of sight, or a new plain one.
    fn home_workspace(&mut self) -> anyhow::Result<Workspace> {
        match self.parked.iter().position(|w| w.project.is_none()) {
            Some(index) => Ok(self.parked.remove(index)),
            None => self.start_workspace(None),
        }
    }

    /// Writes the file of the project on screen: its tabs as they are now (see
    /// `save_tabs`). When you leave it and when the window closes: after `exit`
    /// in each tab, the file stays as it was, not a project of no tabs.
    pub(super) fn save_project(&self) -> anyhow::Result<()> {
        match &self.project {
            Some(open) => save_tabs(open, &self.tabs),
            None => Ok(()), // no project, or one that couldn't be read: not written over
        }
    }

    /// Writes the files of the projects out of sight: all of them, even after one
    /// fails. The first problem is the one returned.
    pub(super) fn save_parked(&self) -> anyhow::Result<()> {
        let mut saved = Ok(());
        for workspace in &self.parked {
            if let Some(open) = &workspace.project {
                let result = save_tabs(open, &workspace.tabs);
                if saved.is_ok() {
                    saved = result;
                }
            }
        }
        saved
    }

    /// Closes the open project `name`, on screen or out of sight: saves its file
    /// and ends its shells. The one on screen: home comes. If a command runs in
    /// it, or the file can't be written, only the second time (`again` says how).
    pub(super) fn close_project(&mut self, name: &str, again: &str) {
        let here = self.project.as_ref().is_some_and(|p| p.name == name);
        let (open, tabs) = if here {
            (self.project.as_ref(), &self.tabs)
        } else {
            match self.parked.iter().find(|w| w.name() == Some(name)) {
                Some(w) => (w.project.as_ref(), &w.tabs),
                None => return, // not open
            }
        };
        let (Some(open), Some(first)) = (open, tabs.first()) else {
            return;
        };
        let id = first.id;
        let running = tabs.iter().filter(|tab| tab.command.running()).count();
        let saved = save_tabs(open, tabs);
        let message = match (&saved, running) {
            (Err(e), _) => format!("{e:#}: {again}"),
            (Ok(()), 0) => String::new(),
            (Ok(()), 1) => format!("a command runs there: {again}"),
            (Ok(()), n) => format!("commands run in {n} tabs: {again}"),
        };
        let ask = saved.is_err() || running > 0;
        if self.confirm_close(Closing::Project(id), ask, message) {
            self.end_project(name);
        }
    }

    /// Ends the shells of the open project `name`, without writing its file. Out
    /// of sight: dropped. On screen: home comes, and if home can't start, the
    /// project stays (`false`).
    fn end_project(&mut self, name: &str) -> bool {
        let here = self.project.as_ref().is_some_and(|p| p.name == name);
        if !here {
            // Dropped, its tabs end their shells (`Tab`'s `drop`).
            self.parked.retain(|w| w.name() != Some(name));
            self.request_redraw();
            return true;
        }
        match self.home_workspace() {
            Ok(home) => {
                self.tabs.clear();
                self.enter(home);
                true
            }
            Err(e) => {
                self.set_status(Some(format!("{e:#}")));
                false
            }
        }
    }

    /// Deletes the project `name`: its file, not its folder (`project::delete`).
    /// Always on the second press only (`again` says how): a file deleted can't
    /// come back. An open one is closed first, its commands ended, its file not
    /// written.
    pub(super) fn delete_project(&mut self, name: &str, again: &str) {
        let here = self.project.as_ref().is_some_and(|p| p.name == name);
        let tabs = if here {
            Some(&self.tabs)
        } else {
            self.parked
                .iter()
                .find(|w| w.name() == Some(name))
                .map(|w| &w.tabs)
        };
        let open = tabs.is_some();
        let running = tabs.map_or(0, |tabs| {
            tabs.iter().filter(|tab| tab.command.running()).count()
        });
        let what = match running {
            // Short: the sidebar's bottom has room for ~30 letters a line.
            0 => "its file goes, folder stays",
            1 => "a command ends, folder stays",
            _ => "commands end, folder stays",
        };
        let message = format!("{again} to delete {name}: {what}");
        if !self.confirm_close(Closing::Delete(name.to_string()), true, message) {
            return;
        }
        if open && !self.end_project(name) {
            return;
        }
        if let Err(e) = project::delete(name) {
            self.set_status(Some(format!("{e:#}")));
        }
        // Home's start screen was made before the file went: again, without it.
        if self.start.is_some() {
            self.start = self.start_screen();
        }
        self.refresh_sidebar();
        self.request_redraw();
    }

    /// What's going on in the project `name` (`None`: home), for the sidebar.
    pub(super) fn live_of(&self, name: Option<&str>) -> Live {
        let here = self.project.as_ref().map(|p| p.name.as_str()) == name;
        let tabs = if here {
            &self.tabs
        } else {
            match self.parked.iter().find(|w| w.name() == name) {
                Some(w) => &w.tabs,
                None => return Live::default(), // not open
            }
        };
        Live {
            open: true,
            busy: tabs.iter().any(|tab| tab.command.running()),
            activity: tabs.iter().any(|tab| tab.activity),
        }
    }

    /// Ctrl+Alt+→ / ←: the next / previous open project (`step` 1 / -1). In the
    /// sidebar's order, which you set, not the order they were opened in: one
    /// that goes out of sight keeps its place. Home first, always there (a new
    /// one if it was closed); round: after the last one, home.
    pub(super) fn step_project(&mut self, step: isize) {
        let here = self.project.as_ref().map(|p| p.name.clone());
        let open = |name: &String| {
            here.as_ref() == Some(name) || self.parked.iter().any(|w| w.name() == Some(name))
        };
        let mut ring = vec![None];
        ring.extend(project::names().into_iter().filter(open).map(Some));
        let at = ring.iter().position(|name| *name == here).unwrap_or(0);
        let next = (at as isize + step).rem_euclid(ring.len() as isize) as usize;
        if next != at {
            let name = ring[next].clone();
            self.switch_to(name.as_deref());
        }
    }

    /// Goes to the project `name` in this window, or home (`None`). The one on
    /// screen is saved and goes out of sight, everything in it still running.
    /// The other one comes back as it was left, or starts from its file if it
    /// isn't open yet.
    pub(super) fn switch_to(&mut self, name: Option<&str>) {
        if self.project.as_ref().map(|p| p.name.as_str()) == name {
            // It's on screen. Home again: back to its start screen, position 0.
            if self.start.is_none() {
                self.start = self.start_screen();
                self.request_redraw();
            }
            return;
        }
        let open = self.parked.iter().position(|w| w.name() == name);
        let next = match open {
            Some(index) => self.parked.remove(index),
            None => match self.start_workspace(name) {
                Ok(workspace) => workspace,
                Err(e) => {
                    self.set_status(Some(format!("{e:#}")));
                    return;
                }
            },
        };
        // Can't be saved: say so, and go anyway. The file is tried again when
        // you leave next time, and when the window closes.
        let saved = self.save_project();
        let left = Workspace {
            project: self.project.take(),
            tabs: std::mem::take(&mut self.tabs),
            active: self.active,
        };
        self.start = None;
        self.parked.push(left);
        self.enter(next);
        if let Err(e) = saved {
            self.set_status(Some(format!("{e:#}")));
        }
    }

    /// `workspace` comes on screen: its tabs, its project's name and color.
    fn enter(&mut self, workspace: Workspace) {
        self.project = workspace.project;
        self.tabs = workspace.tabs;
        self.active = workspace.active.min(self.tabs.len() - 1);
        // What was waiting for a second press or held by the mouse was the
        // other project's.
        self.pending_close = None;
        self.tab_drag = None;
        self.set_status(None);
        self.on_resize(); // room for this project's name before the tabs
        self.show_tab(self.active);
        // After `show_tab`, which puts a start screen away.
        self.start = self.start_screen();
    }

    /// Home's start screen as you come back to it, whatever you did there
    /// before: the projects used last, as they are now. Home is position 0, a
    /// calm place to start from. `None` in a project, and while a command runs
    /// in the tab on screen: you came back to watch it. Under the screen, the
    /// terminal stays as it was (a key puts the screen away). Never with
    /// `start_screen = false`: home is a plain terminal then.
    pub(super) fn start_screen(&self) -> Option<Start> {
        let idle = !self.tab().command.running();
        (self.config.start_screen && self.project.is_none() && idle)
            .then(|| Start::new(project::recent_entries(start::RECENT), self.config.backdrop))
    }

    /// The tabs of the project `name` (`None`: home, one plain tab), started now:
    /// as `kel <name>` would start them, as big as the tabs on screen.
    fn start_workspace(&mut self, name: Option<&str>) -> anyhow::Result<Workspace> {
        let project = name.map(project::load).transpose()?;
        let recipes = project
            .as_ref()
            .map_or_else(|| vec![TabRecipe::default()], Project::tabs);
        let (rows, cols) = match self.tabs.first() {
            Some(tab) => {
                let terminal = tab.terminal.lock().unwrap();
                (terminal.grid.rows(), terminal.grid.cols())
            }
            None => (ROWS, COLS),
        };
        let mut tabs = Vec::new();
        let mut failed = None;
        for recipe in &recipes {
            let folder = project.as_ref().map(|p| p.folder_of(recipe));
            let shell = match &project {
                Some(project) => project.shell_of(recipe, || self.config.shell()),
                None => self.config.shell(),
            };
            let id = self.next_id;
            self.next_id += 1;
            let screen = self.screen(rows, cols);
            match Tab::spawn(id, &shell, folder.as_deref(), screen, self.proxy.clone()) {
                Ok(mut tab) => {
                    tab.name = recipe.name.clone();
                    tab.own_shell = recipe.shell.clone();
                    if let Some(command) = &recipe.command {
                        tab.type_at_start(command, recipe.start);
                    }
                    tabs.push(tab);
                }
                Err(e) => failed = Some(e),
            }
        }
        if tabs.is_empty() {
            let e = failed.unwrap_or_else(|| anyhow::anyhow!("no tabs"));
            return Err(e.context("can't open a tab"));
        }
        if let Some(e) = failed {
            self.set_status(Some(format!("can't open a tab: {e:#}")));
        }
        let project = name.zip(project).map(|(name, project)| OpenProject {
            name: name.to_string(),
            folder: project.folder,
            color: project.color,
            shell: project.shell,
        });
        // Home's start screen comes with `enter`.
        Ok(Workspace {
            project,
            tabs,
            active: 0,
        })
    }

    /// Saves the tabs as the new project `name`, in the folder of the tab on
    /// screen (the other tabs' folders are kept inside it, see `save_project`).
    /// The window becomes that project: its name shows, it's saved when it
    /// closes. Its color, if this window had one, stays.
    pub(super) fn save_as_project(&mut self, name: String) {
        if project::exists(&name) {
            // Made since the palette opened: never written over.
            self.set_status(Some(format!("project \"{name}\" already exists")));
            return;
        }
        let Some(folder) = self.tab().cwd() else {
            self.set_status(Some("can't save: no folder for this tab".to_string()));
            return;
        };
        // Its color and shell, if this window had them, stay.
        let color = self.project.as_ref().and_then(|p| p.color);
        let shell = self.project.as_ref().and_then(|p| p.shell.clone());
        let before = self.project.replace(OpenProject {
            name,
            folder,
            color,
            shell,
        });
        if let Err(e) = self.save_project() {
            self.project = before;
            self.set_status(Some(format!("{e:#}")));
            return;
        }
        self.update_window_title();
        self.on_resize(); // room for the name before the tabs
        self.request_redraw();
    }

    /// The project `old` is `new` now, and `color`: its file is renamed and
    /// written. One open in the window takes them at once: its name before the
    /// tabs, its color.
    pub(super) fn change_project(
        &mut self,
        old: &str,
        new: &str,
        color: Option<u32>,
    ) -> anyhow::Result<()> {
        if new != old {
            project::rename(old, new)?;
            let parked = self.parked.iter_mut().filter_map(|w| w.project.as_mut());
            for open in self.project.iter_mut().chain(parked) {
                if open.name == old {
                    open.name = new.to_string();
                }
            }
        }
        if let Some(open) = self.project.as_mut().filter(|p| p.name == new) {
            open.color = color;
            self.save_project()?;
            self.update_window_title();
            self.on_resize(); // the name before the tabs: another length
            return Ok(());
        }
        for workspace in &mut self.parked {
            if let Workspace {
                project: Some(open),
                tabs,
                ..
            } = workspace
                && open.name == new
            {
                open.color = color;
                return save_tabs(open, tabs);
            }
        }
        project::set_color(new, color)
    }
}

/// Writes the project `open`'s file with `tabs`, each in its last folder. Only
/// what you wrote in the file comes back, the name, the command and the shell:
/// Kel never adds a command, so opening a project never runs `git push` by
/// itself. A tab opened since (Ctrl+Shift+T) is saved as a plain one, with its
/// folder.
pub(super) fn save_tabs(open: &OpenProject, tabs: &[Tab]) -> anyhow::Result<()> {
    // Everything the file had but the tabs, or a line would be lost: the tabs
    // are written again, the rest is kept as it was read.
    let mut project = Project {
        folder: open.folder.clone(),
        color: open.color,
        shell: open.shell.clone(),
        tabs: Vec::new(),
    };
    let tabs = tabs.iter().map(|tab| TabRecipe {
        name: tab.name.clone(),
        folder: tab.cwd().and_then(|cwd| project.tab_folder(&cwd)),
        command: tab.start_command.clone(),
        // Without a command there's nothing to start: no line.
        start: tab.start_command.as_ref().map_or(When::Now, |_| tab.start),
        shell: tab.own_shell.clone(),
    });
    project.tabs = tabs.collect();
    project::save(&open.name, &project)
}
