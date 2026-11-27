//! Isolated agent sessions and review of their net worktree changes.

use super::*;
use ws_storage::AgentWorktree;

const SESSIONS_WIDTH: Pixels = px(560.);
const REVIEW_WIDTH: Pixels = px(1000.);

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub(super) struct ReviewAgentWorktree {
  source: PathBuf,
  checkout: PathBuf,
  record: AgentWorktree,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub(super) struct OpenAgentGit {
  source: PathBuf,
  checkout: PathBuf,
}

impl Workspace {
  pub(super) fn new_codex_worktree(&mut self, _: &crate::NewCodexWorktree, window: &mut Window, cx: &mut Context<Self>) {
    self.start_agent_worktree(AgentKind::Codex, window, cx);
  }

  pub(super) fn new_claude_worktree(&mut self, _: &crate::NewClaudeWorktree, window: &mut Window, cx: &mut Context<Self>) {
    self.start_agent_worktree(AgentKind::Claude, window, cx);
  }

  pub(super) fn agent_worktree_running(&self, checkout: &Path) -> bool {
    self.agent_tabs.values().any(|tabs| tabs.tabs.iter().any(|tab| tab.worktree.as_deref() == Some(checkout)))
  }

  fn start_agent_worktree(&mut self, kind: AgentKind, window: &mut Window, cx: &mut Context<Self>) {
    if self.agent_worktree_task.is_some() || self.agents_missing(window, cx) {
      return;
    }
    if !extensions::enabled_agents(cx).contains(&kind) {
      window.push_notification(crate::failure(format!("Enable {} in the AI Agent extension first.", kind.label())), cx);
      return;
    }
    let Some(source) = self.projects.active.clone() else {
      window.push_notification(crate::failure("Open a Git project to start an agent worktree."), cx);
      return;
    };
    let profile = self.profile.clone();
    let root = source.clone();
    let work = cx.background_executor().spawn(async move {
      let base = ws_git::head_commit_id(&root)?;
      let (checkout, record) = AgentWorktree::reserve(&profile, &root, kind.id(), base).map_err(|error| error.to_string())?;
      match ws_git::add_worktree_at_commit(&root, &checkout, &record.branch, &record.base) {
        Ok(path) => Ok((path, record)),
        Err(error) => {
          AgentWorktree::release_empty(&checkout);
          Err(error)
        }
      }
    });
    self.agent_worktree_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.agent_worktree_task = None;
        match result {
          Ok((path, record)) => {
            if this.projects.projects.iter().any(|project| project.path == source)
              && this.start_agent_tab(source.clone(), kind, Some(path), window, cx)
            {
              window.push_notification(Notification::info(this.say_where(&source, format!("Started {} on {}.", kind.label(), record.branch))), cx);
            } else {
              window.push_notification(Notification::info(format!("Worktree {} is available for review.", record.branch)), cx);
            }
          }
          Err(error) => window.push_notification(crate::failure(this.say_where(&source, format!("Could not start an agent worktree: {error}"))), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  pub(super) fn show_agent_changes(&mut self, _: &crate::ShowAgentChanges, window: &mut Window, cx: &mut Context<Self>) {
    let Some(source) = self.projects.active.clone() else {
      window.push_notification(crate::failure("Open a project to review its agent worktrees."), cx);
      return;
    };
    if self.agent_review_task.is_some() {
      return;
    }
    let profile = self.profile.clone();
    let root = source.clone();
    let work = cx.background_executor().spawn(async move { AgentWorktree::list(&profile, &root) });
    self.agent_review_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.agent_review_task = None;
        if this.projects.active.as_ref() != Some(&source) {
          return;
        }
        match result {
          Ok(sessions) => this.show_agent_sessions(source, sessions, window, cx),
          Err(error) => window.push_notification(crate::failure(format!("Could not read agent worktrees: {error}")), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  fn show_agent_sessions(&self, source: PathBuf, sessions: Vec<(PathBuf, AgentWorktree)>, window: &mut Window, cx: &mut Context<Self>) {
    let workspace = cx.entity().downgrade();
    let focus = self.focus_handle.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
      let Some(workspace) = workspace.upgrade() else { return dialog };
      let this = workspace.read(cx);
      let muted = cx.theme().muted_foreground;
      let rows = sessions.iter().map(|(checkout, record)| {
        let action = ReviewAgentWorktree { source: source.clone(), checkout: checkout.clone(), record: record.clone() };
        let focus = focus.clone();
        h_flex()
          .items_center()
          .gap_3()
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(Label::new(agent_label(&record.agent)).font_weight(FontWeight::SEMIBOLD))
              .child(Label::new(record.branch.clone()).text_sm().text_color(muted).truncate()),
          )
          .child(Label::new(if this.agent_worktree_running(checkout) { "Running" } else { "Stopped" }).text_sm().text_color(muted))
          .child(
            Button::new(SharedString::from(format!("review-{}", record.branch)))
              .small()
              .label("Review Changes")
              .on_click(move |_, window, cx| focus.dispatch_action(&action, window, cx)),
          )
      });
      dialog.title("Agent Changes").width(SESSIONS_WIDTH).child(
        v_flex()
          .gap_2()
          .when(sessions.is_empty(), |list| list.child(Label::new("No agent worktrees for this project.").text_color(muted)))
          .children(rows)
          .max_h_80()
          .overflow_y_scrollbar(),
      )
    });
  }

  pub(super) fn review_agent_worktree(&mut self, action: &ReviewAgentWorktree, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&action.source) {
      return;
    }
    let (checkout, base) = (action.checkout.clone(), action.record.base.clone());
    let title: SharedString = format!("{} — {}", agent_label(&action.record.agent), action.record.branch).into();
    let diff = cx.new(|cx| {
      let mut diff = AllChanges::for_agent(self.settings.clone());
      diff.reload_agent(checkout.clone(), base.clone(), cx);
      diff
    });
    let open = OpenAgentGit { source: action.source.clone(), checkout: checkout.clone() };
    let focus = self.focus_handle.clone();
    window.open_dialog(cx, move |dialog, window, cx| {
      let (refresh, checkout, base) = (diff.clone(), checkout.clone(), base.clone());
      let (focus, open) = (focus.clone(), open.clone());
      let height = window.viewport_size().height * 0.64;
      dialog.title(title.clone()).width(REVIEW_WIDTH).child(
        v_flex()
          .h(height)
          .gap_2()
          .child(
            h_flex()
              .gap_2()
              .items_center()
              .child(Label::new(format!("Changes since {:.7}", base)).flex_1().text_sm().text_color(cx.theme().muted_foreground))
              .child(Button::new("refresh-agent-changes").small().label("Refresh").on_click(move |_, _, cx| {
                refresh.update(cx, |diff, cx| diff.reload_agent(checkout.clone(), base.clone(), cx));
              }))
              .child(
                Button::new("open-agent-git")
                  .small()
                  .primary()
                  .label("Open Git Changes")
                  .on_click(move |_, window, cx| focus.dispatch_action(&open, window, cx)),
              ),
          )
          .child(div().flex_1().min_h_0().child(diff.clone())),
      )
    });
  }

  pub(super) fn open_agent_git(&mut self, action: &OpenAgentGit, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&action.source) {
      return;
    }
    if !action.checkout.join(".git").is_file() {
      window.push_notification(crate::failure("The agent worktree is unavailable."), cx);
      return;
    }
    window.close_all_dialogs(cx);
    if action.checkout != action.source {
      let path = action.checkout.clone();
      self.switch_project(window, cx, move |projects| {
        projects.add(path.clone());
        projects.set_favorite(&path, true);
      });
    }
    self.hide_terminal(window, cx);
    self.sidebar_tab = SidebarTab::Git;
    self.set_git_view(GitView::Changes, cx);
    cx.notify();
  }
}

fn agent_label(agent: &str) -> &'static str {
  AgentKind::ALL.into_iter().find(|kind| kind.id() == agent).map_or("Agent", AgentKind::label)
}
