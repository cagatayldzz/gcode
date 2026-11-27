//! The project's explicitly launched scripts, displayed in the existing terminal.

use super::*;
use gpui_kit::component::dialog::DialogFooter;
use ws_terminal::project_scripts::{discover, Script};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ScriptKey {
  root: PathBuf,
  source: &'static str,
  name: String,
}

impl ScriptKey {
  fn new(root: PathBuf, script: &Script) -> Self {
    Self { root, source: script.source, name: script.name.clone() }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScriptStatus {
  Running,
  Stopping,
  Stopped,
  Exited(Option<i32>),
}

pub(super) struct ScriptRun {
  terminal: Option<Entity<Terminal>>,
  status: ScriptStatus,
  restart: Option<Script>,
  stop_task: Option<Task<()>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Control {
  Run,
  Stop,
  Restart,
  Output,
}

impl Control {
  fn label(self) -> &'static str {
    match self {
      Self::Run => "Run",
      Self::Stop => "Stop",
      Self::Restart => "Restart",
      Self::Output => "Show Output",
    }
  }
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = app, no_json)]
pub(super) struct ControlScript {
  key: ScriptKey,
  script: Script,
  control: Control,
}

impl Workspace {
  pub(super) fn show_project_scripts(&mut self, _: &crate::ShowProjectScripts, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else {
      window.push_notification(Notification::info("Open a project to see its scripts."), cx);
      return;
    };
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { discover(&root) }
    });
    self.scripts_discovery = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.scripts_discovery = None;
        if this.projects.active.as_ref() != Some(&root) {
          return;
        }
        match result {
          Ok(scripts) => this.show_scripts(root, scripts, window, cx),
          Err(error) => window.push_notification(crate::failure(error), cx),
        }
      });
    }));
  }

  fn show_scripts(&self, root: PathBuf, scripts: Vec<Script>, window: &mut Window, cx: &mut Context<Self>) {
    let workspace = cx.entity().downgrade();
    let focus = self.focus_handle.clone();
    window.open_alert_dialog(cx, move |dialog, _, cx| {
      let mut rows = v_flex().gap_2();
      if scripts.is_empty() {
        rows = rows.child(Label::new("No project scripts were found."));
      }
      let workspace = workspace.upgrade();
      for script in &scripts {
        let key = ScriptKey::new(root.clone(), script);
        let run = workspace.as_ref().and_then(|workspace| workspace.read(cx).script_runs.get(&key));
        let status = match run.map(|run| run.status) {
          None => "Ready".into(),
          Some(ScriptStatus::Running) => "Running".into(),
          Some(ScriptStatus::Stopping) => "Stopping…".into(),
          Some(ScriptStatus::Stopped) => "Stopped".into(),
          Some(ScriptStatus::Exited(Some(code))) => format!("Exit {code}"),
          Some(ScriptStatus::Exited(None)) => "Exited".into(),
        };
        let controls: &[Control] = match run.map(|run| run.status) {
          Some(ScriptStatus::Running) => &[Control::Output, Control::Stop, Control::Restart],
          Some(ScriptStatus::Stopping) => &[Control::Output],
          _ if run.is_some_and(|run| run.terminal.is_some()) => &[Control::Output, Control::Run],
          _ => &[Control::Run],
        };
        rows = rows.child(
          h_flex()
            .gap_2()
            .items_center()
            .child(
              v_flex().flex_1().min_w_0().child(Label::new(script.name.clone()).truncate()).child(
                Label::new(match &script.body {
                  Some(body) => format!("{} · {} · {}", script.source, script.command(), body.lines().collect::<Vec<_>>().join(" ")),
                  None => format!("{} · {}", script.source, script.command()),
                })
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .truncate(),
              ),
            )
            .child(Label::new(status).text_sm())
            .children(controls.iter().map(|control| {
              let action = ControlScript { key: key.clone(), script: script.clone(), control: *control };
              let focus = focus.clone();
              Button::new(SharedString::from(format!("script-{}-{}-{}", script.source, script.name, control.label())))
                .small()
                .label(control.label())
                .on_click(move |_, window, cx| {
                  window.close_dialog(cx);
                  focus.dispatch_action(&action, window, cx);
                })
            })),
        );
      }
      dialog
        .title("Project Scripts")
        .w(rems(44.))
        .confirm()
        .child(rows.max_h_80().overflow_y_scrollbar())
        .footer(DialogFooter::new().child(Button::new("project-scripts-close").label("Close").on_click(|_, window, cx| window.close_dialog(cx))))
    });
  }

  pub(super) fn control_script(&mut self, action: &ControlScript, window: &mut Window, cx: &mut Context<Self>) {
    let key = &action.key;
    if self.projects.active.as_ref() != Some(&key.root) {
      return;
    }
    match action.control {
      Control::Run => self.start_script(key.clone(), action.script.clone(), window, cx),
      Control::Output => self.show_script_terminal(key, window, cx),
      Control::Stop => self.stop_script(key, None, window, cx),
      Control::Restart => self.stop_script(key, Some(action.script.clone()), window, cx),
    }
  }

  fn start_script(&mut self, key: ScriptKey, script: Script, window: &mut Window, cx: &mut Context<Self>) {
    if self.script_runs.get(&key).is_some_and(|run| matches!(run.status, ScriptStatus::Running | ScriptStatus::Stopping)) {
      return;
    }
    // Start a new tab after the previous output has been dismissed. Commands are passed as literal arguments.
    let shell = std::env::var("SHELL")
      .ok()
      .filter(|shell| Path::new(shell).file_name().and_then(|name| name.to_str()).is_some_and(|name| matches!(name, "zsh" | "bash" | "sh" | "ksh")))
      .unwrap_or_else(|| "/bin/sh".into());
    let Some(terminal) = self.start_program(key.root.clone(), Some(script.invocation(shell)), &script.name, window, cx) else { return };
    if let Some(old) = self.script_runs.remove(&key).and_then(|run| run.terminal) {
      if let Some(tabs) = self.terminal_tabs.get_mut(&key.root) {
        tabs.remove(&old);
      }
    }
    terminal.update(cx, |terminal, cx| terminal.set_title(script.name.clone().into(), cx));
    self.terminal_tabs.entry(key.root.clone()).or_insert_with(Arrangement::new).add_tab(terminal.clone());
    self.script_runs.insert(key.clone(), ScriptRun { terminal: Some(terminal), status: ScriptStatus::Running, restart: None, stop_task: None });
    if self.projects.active.as_ref() == Some(&key.root) {
      self.show_script_terminal(&key, window, cx);
    }
    cx.notify();
  }

  fn show_script_terminal(&mut self, key: &ScriptKey, window: &mut Window, cx: &mut Context<Self>) {
    let Some(terminal) = self.script_runs.get(key).and_then(|run| run.terminal.clone()) else { return };
    let Some(tabs) = self.terminal_tabs.get_mut(&key.root) else { return };
    let Some(index) = tabs.tabs.iter().position(|tab| tab.contains(&terminal)) else { return };
    tabs.select(index);
    if !self.terminal_open {
      self.terminal_docked = true;
    }
    self.app_settings = None;
    self.show_terminal(window, cx);
    self.focus_pane(&terminal, window, cx);
  }

  fn stop_script(&mut self, key: &ScriptKey, restart: Option<Script>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(run) = self.script_runs.get_mut(key).filter(|run| run.status == ScriptStatus::Running) else { return };
    let Some(terminal) = &run.terminal else { return };
    run.status = ScriptStatus::Stopping;
    run.restart = restart;
    terminal.read(cx).interrupt();
    let key = key.clone();
    let session = terminal.entity_id();
    run.stop_task = Some(cx.spawn_in(window, async move |this, cx| {
      cx.background_executor().timer(Duration::from_secs(2)).await;
      let _ = this.update_in(cx, |this, window, cx| {
        let Some(run) = this.script_runs.get_mut(&key) else { return };
        let Some(terminal) = run.terminal.as_ref().filter(|terminal| terminal.entity_id() == session) else { return };
        if let Err(error) = terminal.read(cx).terminate() {
          run.status = ScriptStatus::Running;
          run.restart = None;
          window.push_notification(crate::failure(format!("Could not stop {}: {error}", key.name)), cx);
          cx.notify();
        }
      });
    }));
    cx.notify();
  }

  pub(super) fn script_exited(&mut self, terminal: &Entity<Terminal>, code: Option<i32>, window: &mut Window, cx: &mut Context<Self>) -> bool {
    let Some((key, run)) = self.script_runs.iter_mut().find(|(_, run)| run.terminal.as_ref() == Some(terminal)) else { return false };
    let key = key.clone();
    run.status = if run.status == ScriptStatus::Stopping { ScriptStatus::Stopped } else { ScriptStatus::Exited(code) };
    run.stop_task = None;
    if let Some(script) = run.restart.take() {
      self.start_script(key, script, window, cx);
    }
    cx.notify();
    true
  }

  pub(super) fn script_terminal_closed(&mut self, terminal: &Entity<Terminal>, cx: &App) {
    if let Some(run) = self.script_runs.values_mut().find(|run| run.terminal.as_ref() == Some(terminal)) {
      run.terminal = None;
      run.restart = None;
      run.stop_task = None;
      if matches!(run.status, ScriptStatus::Running | ScriptStatus::Stopping) {
        let _ = terminal.read(cx).terminate();
        run.status = ScriptStatus::Stopped;
      }
    }
  }

  pub(super) fn clear_project_scripts(&mut self, root: &Path, cx: &App) {
    self.script_runs.retain(|key, run| {
      if key.root != root {
        return true;
      }
      if matches!(run.status, ScriptStatus::Running | ScriptStatus::Stopping) {
        if let Some(terminal) = &run.terminal {
          let _ = terminal.read(cx).terminate();
        }
      }
      false
    });
  }
}
