//! A project whose folder was deleted or moved outside the app: it stays in the list, and opening it says so and offers the ways out
//! (find the folder, clone the repository again, remove the project) instead of silently dropping it.

use super::*;

impl Workspace {
  /// Whether the open project's folder is gone.
  pub(super) fn active_project_missing(&self) -> bool {
    self.projects.active.as_ref().is_some_and(|root| self.missing_projects.contains(root))
  }

  /// Keeps the address the open project's repository has, so that the project can be cloned again if its folder is deleted.
  pub(super) fn remember_remote(&mut self) {
    let (Some(root), Some(url)) = (self.projects.active.clone(), self.git.as_ref().and_then(|git| git.sync.as_ref()).map(|sync| sync.url.clone()))
    else {
      return;
    };
    if self.projects.set_remote_url(&root, &url) {
      self.persist_projects();
    }
  }

  /// Checks the folder again, as the link on the page says, and shows the project if it is back.
  fn check_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.sync_projects(window, cx);
  }

  /// Asks for the folder the project is in now, and points the project at it.
  fn locate_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(old) = self.projects.active.clone() else { return };
    let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Locate".into()) });
    cx.spawn_in(window, async move |this, cx| {
      let Ok(Ok(Some(paths))) = picked.await else { return };
      let Some(new) = paths.into_iter().next() else { return };
      let _ = this.update_in(cx, |this, window, cx| this.relocate_missing(old, new, window, cx));
    })
    .detach();
  }

  /// Points the missing project `old` at the folder `new`, which must not be a listed project already.
  fn relocate_missing(&mut self, old: PathBuf, new: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    if old != new && self.projects.projects.iter().any(|project| project.path == new) {
      window.push_notification(crate::failure(format!("{} is already in the project list.", display_path(&new))), cx);
      return;
    }
    self.missing_projects.remove(&old);
    if old != new {
      let (from, to) = (old.clone(), new.clone());
      self.switch_project(window, cx, move |projects| {
        projects.relocate(&from, &to);
      });
      self.terminal_tabs.remove(&old);
      self.agent_tabs.remove(&old);
      self.views.remove(&old);
    }
    self.sync_projects(window, cx);
    cx.notify();
  }

  /// Clones the repository again into the folder the project was in, from the address it had when it was last seen.
  fn clone_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(old) = self.projects.active.clone() else { return };
    let Some(url) = self.projects.projects.iter().find(|project| project.path == old).and_then(|project| project.remote_url.clone()) else { return };
    if self.recovering {
      return;
    }
    self.recovering = true;
    cx.notify();
    let accounts = self.settings.read(cx).accounts.clone();
    let parent = old.parent().map(Path::to_path_buf).unwrap_or_else(|| old.clone());
    let work = cx.background_executor().spawn(async move {
      std::fs::create_dir_all(&parent).map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
      ws_git::clone(&url, &parent, crate::connections::credentials_for_url(&accounts, &url).as_ref())
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.recovering = false;
        match result {
          Ok(path) => this.relocate_missing(old, path, window, cx),
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// What the main area shows for a project whose folder is gone: the page of GitHub Desktop's "Can't find …".
  pub(super) fn render_missing_project(&self, cx: &mut Context<Self>) -> AnyElement {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let Some(root) = self.projects.active.clone() else { return div().into_any_element() };
    let project = self.projects.projects.iter().find(|project| project.path == root);
    let name = project.map(Project::name).unwrap_or_else(|| root.display().to_string());
    let can_clone = project.is_some_and(|project| project.remote_url.is_some());
    let recovering = self.recovering;
    v_flex()
      .size_full()
      .items_center()
      .justify_center()
      .gap_4()
      .bg(palette.background)
      .child(div().text_2xl().child(format!("Can't find \"{name}\"")))
      .child(
        h_flex()
          .gap_1()
          .items_center()
          .text_color(palette.text_secondary)
          .child("It was last seen at")
          .child(
            div()
              .px_1()
              .rounded_sm()
              .bg(palette.surface)
              .font_family(theme.mono_font_family.clone())
              .text_size(FONT_SIZE_SM)
              .text_color(palette.text)
              .child(root.display().to_string()),
          )
          .child(".")
          .child(
            div()
              .id("missing-check-again")
              .cursor_pointer()
              .text_color(palette.link)
              .hover(|link| link.underline())
              .child("Check again.")
              .on_click(cx.listener(|this, _, window, cx| this.check_again(window, cx))),
          ),
      )
      .child(
        h_flex()
          .gap_2()
          .child(
            Button::new("missing-locate")
              .primary()
              .label("Locate…")
              .disabled(recovering)
              .on_click(cx.listener(|this, _, window, cx| this.locate_project(window, cx))),
          )
          .when(can_clone, |buttons| {
            buttons.child(
              Button::new("missing-clone")
                .outline()
                .label(if recovering { "Cloning…" } else { "Clone Again" })
                .disabled(recovering)
                .on_click(cx.listener(|this, _, window, cx| this.clone_again(window, cx))),
            )
          })
          .child(Button::new("missing-remove").outline().label("Remove").disabled(recovering).on_click({
            let root = root.clone();
            cx.listener(move |this, _, window, cx| this.remove_project(&root, window, cx))
          })),
      )
      .into_any_element()
  }
}
