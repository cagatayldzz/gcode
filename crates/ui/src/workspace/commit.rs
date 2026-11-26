//! Updates, account connections and commit message generation.

use super::*;

impl Workspace {
  /// Tells the user once that a newer version is available, in the window they are using.
  pub(super) fn announce_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if !window.is_window_active() {
      return;
    }
    let announce = {
      let state = &self.settings.read(cx).updates;
      match &state.update {
        AppUpdate::Available(release) if state.announced.as_deref() != Some(release.tag.as_str()) => {
          Some((release.tag.clone(), release.version.clone()))
        }
        _ => None,
      }
    };
    if let Some((tag, version)) = announce {
      self.settings.update(cx, |model, _| model.updates.announced = Some(tag));
      window.push_notification(Notification::info(format!("{} {version} is available. Download it in Settings → About.", ws_product::NAME)), cx);
    }
  }

  /// Follows the title bar's Update button: restarts once the update is downloaded, or says why it failed.
  pub(super) fn continue_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (ready, failed) = {
      let state = &self.settings.read(cx).updates;
      if !state.restart_when_ready {
        return;
      }
      match &state.update {
        AppUpdate::Ready(_) => (true, None),
        AppUpdate::Failed(message) => (false, Some(message.clone())),
        _ => return,
      }
    };
    self.settings.update(cx, |model, _| model.updates.restart_when_ready = false);
    if ready {
      self.settings.update(cx, |model, cx| model.restart_to_update(cx));
    }
    if let Some(message) = failed {
      window.push_notification(crate::failure(format!("The update failed: {message}")), cx);
    }
  }

  /// Opens the activity log panel at the bottom of the window, or closes it.
  fn toggle_activity_log(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.activity_log.take().is_none() {
      let log = cx.new(|cx| crate::activity_panel::ActivityLog::new(window, cx));
      let subscription = cx.subscribe_in(&log, window, |this, _, _: &crate::activity_panel::ActivityClosed, _, cx| {
        this.activity_log = None;
        cx.notify();
      });
      self.activity_log = Some((log, subscription));
    }
    cx.notify();
  }

  /// The debug button of the title bar: opens the activity log panel. Only debug builds have it.
  pub(super) fn render_activity_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
    if !cfg!(debug_assertions) {
      return None;
    }
    let palette = colors(cx);
    Some(
      rail_button(
        "title-activity",
        Icon::empty().path("icons/bug.svg"),
        "Log Git commands and requests",
        self.activity_log.is_some(),
        false,
        &palette,
        cx.listener(|this, _, window, cx| this.toggle_activity_log(window, cx)),
      )
      .into_any_element(),
    )
  }

  /// The blue Update button at the right of the title bar, shown while a newer version is available. It downloads the update
  /// (the button turns into a loader) and restarts into it.
  pub(super) fn render_update_button(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let (update, restarting) = {
      let state = &self.settings.read(cx).updates;
      (state.update.clone(), state.restart_when_ready)
    };
    let working = matches!(update, AppUpdate::Working(..)) || (restarting && matches!(update, AppUpdate::Ready(_)));
    if !matches!(update, AppUpdate::Available(_) | AppUpdate::Working(..) | AppUpdate::Ready(_)) {
      return None;
    }
    let ready = matches!(update, AppUpdate::Ready(_)) && !restarting;
    let settings = self.settings.clone();
    Some(
      Button::new("title-update")
        .small()
        .custom(selected_style(cx))
        .when(!working, |button| button.bg(cx.theme().primary))
        .label(if working {
          "Updating…"
        } else if ready {
          "Restart"
        } else {
          "Update"
        })
        .loading(working)
        .disabled(working)
        .tooltip(if ready { "Restart to finish the update" } else { "Download the new version and restart" })
        .on_click(move |_, _, cx| {
          settings.update(cx, |model, cx| {
            model.updates.restart_when_ready = true;
            if matches!(model.updates.update, AppUpdate::Ready(_)) {
              model.restart_to_update(cx);
            } else {
              model.download_update(cx);
            }
            cx.notify();
          })
        }),
    )
  }

  /// Opens the dialog that connects a GitHub, GitLab or Azure DevOps account. With `existing`, the service and host are filled
  /// in and only a new token is needed: the account keeps its place in the list and its warning is replaced by the new check.
  pub(crate) fn open_connect_account(&mut self, existing: Option<ws_hosting::Account>, window: &mut Window, cx: &mut Context<Self>) {
    let title = if existing.is_some() { "Reconnect the account" } else { "Connect an account" };
    let form = {
      let keychain = self.settings.read(cx).accounts.keychain();
      cx.new(|cx| ConnectAccount::new(keychain, existing, window, cx))
    };
    self._dialog_subscription = Some(cx.subscribe_in(&form, window, |this, _, connected: &AccountConnected, window, cx| {
      let (mut account, warnings, unchecked) = (connected.0.clone(), connected.1.clone(), connected.2.clone());
      // A permission the token certainly lacks stays on the account until a check finds it fixed.
      account.issue = (!warnings.is_empty()).then(|| warnings.join(" "));
      let described = describe(&account);
      this.settings.update(cx, |model, cx| {
        model.accounts.add(account);
        if let Err(error) = model.accounts.save(&model.profile.accounts_path()) {
          eprintln!("Could not save the accounts: {error}");
        }
        cx.notify();
      });
      window.close_dialog(cx);
      if !warnings.is_empty() {
        window.push_notification(Notification::warning(format!("Connected {described}, but: {}", warnings.join(" "))), cx);
      } else if let Some(note) = unchecked {
        window.push_notification(Notification::info(format!("Connected {described}. {note}")), cx);
      } else {
        window.push_notification(Notification::info(format!("Connected {described}.")), cx);
      }
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title(title).width(px(580.)).p_5().child(form.clone()));
  }

  /// Disconnects an account: it leaves the list and its token leaves the Keychain.
  pub(crate) fn remove_account(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
    let described = self.settings.read(cx).accounts.items.iter().find(|account| account.key() == key).map(describe);
    let keychain = self.settings.read(cx).accounts.keychain();
    let work = cx.background_executor().spawn({
      let key = key.clone();
      async move { ws_hosting::TokenStore::delete(&keychain, &key) }
    });
    self.settings.update(cx, |model, cx| {
      model.accounts.items.retain(|account| account.key() != key);
      if let Err(error) = model.accounts.save(&model.profile.accounts_path()) {
        eprintln!("Could not save the accounts: {error}");
      }
      cx.notify();
    });
    cx.spawn(async move |_, _| work.await).detach();
    if let Some(described) = described {
      window.push_notification(Notification::info(format!("Disconnected {described}.")), cx);
    }
  }

  /// Opens the dialog for the commit message generator: the commit message settings and the commit instructions.
  pub(crate) fn open_commit_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (config, instructions) = {
      let model = self.settings.read(cx);
      (model.commitgen.clone(), model.preferences.commit_instructions.clone())
    };
    let form = cx.new(|cx| CommitSettings::new(config, instructions, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&form, window, |this, _, saved: &CommitSettingsSaved, window, cx| {
      if let Err(error) = saved.config.save(&ws_commitgen::config_path()) {
        window.push_notification(crate::failure(format!("Could not save the commit message settings: {error}")), cx);
        return;
      }
      let (config, instructions) = (saved.config.clone(), saved.instructions.clone());
      this.settings.update(cx, |model, cx| {
        model.commitgen = config;
        model.preferences.commit_instructions = instructions;
        cx.notify();
      });
      window.close_dialog(cx);
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("commit message settings").width(px(600.)).p_5().child(form.clone()));
  }

  /// Writes the summary and description of the commit from the ticked files' changes, with AI.
  /// Asks before removing the AI Agent extension from the commit box's menu.
  pub(super) fn confirm_remove_ai_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let panel = self.extensions_panel.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
      let panel = panel.clone();
      alert
        .title("Remove AI Agent?")
        .description("The AI view with Codex and Claude, and the AI commit messages, stop working until you install it again from Extensions.")
        .width(px(430.))
        .confirm()
        .ok_text("Remove")
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, window, cx| {
          panel.update(cx, |panel, cx| panel.uninstall(ws_extensions::AI_AGENT.to_string(), window, cx));
          true
        })
    });
  }

  /// Asks before disabling the AI Agent extension from the commit box's menu.
  pub(super) fn confirm_disable_ai_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let panel = self.extensions_panel.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
      let panel = panel.clone();
      alert
        .title("Disable AI Agent?")
        .description("The AI view with Codex and Claude, and the AI commit messages, are switched off. You can enable it again in Extensions.")
        .width(px(430.))
        .confirm()
        .ok_text("Disable")
        .on_ok(move |_, window, cx| {
          panel.update(cx, |panel, cx| panel.set_enabled(ws_extensions::AI_AGENT, false, window, cx));
          true
        })
    });
  }

  pub(super) fn generate_commit_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.generating {
      return;
    }
    if !crate::extensions::is_enabled(ws_extensions::AI_AGENT, cx) {
      window.push_notification(crate::failure("Install or enable the AI Agent extension to generate commit messages."), cx);
      return;
    }
    let files: Vec<PathBuf> = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).map(|change| change.path.clone()).collect();
    if files.is_empty() {
      window.push_notification(crate::failure("Choose at least one changed file to describe."), cx);
      return;
    }
    let (config, instructions) = {
      let model = self.settings.read(cx);
      (model.commitgen.clone(), model.preferences.commit_instructions.clone().unwrap_or_else(|| DEFAULT_INSTRUCTIONS.to_string()))
    };
    if config.needs_api_key() {
      window.push_notification(crate::failure("Add your API key to generate commit messages."), cx);
      self.open_commit_settings(window, cx);
      return;
    }
    self.generating = true;
    cx.notify();
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move {
        let count = files.len();
        let diff = ws_activity::timed(
          "ui",
          || format!("build the diff of {count} files for the commit message"),
          || ws_git::diff_files(&root, &files),
          |result| match result {
            Ok(diff) => format!("ok, {} KB", diff.len() / 1024),
            Err(error) => format!("failed: {error}"),
          },
        )?;
        ws_activity::timed(
          "ui",
          || "write the commit message".into(),
          || ws_commitgen::generate(&config, &instructions, &diff),
          |result| match result {
            Ok(message) => format!("ok: {}", message.summary),
            Err(error) => format!("failed: {error}"),
          },
        )
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.generating = false;
        // The user may have moved to another project while the message was being written.
        if this.projects.active.as_ref() == Some(&root) {
          match result {
            Ok(message) => {
              this.commit_summary.update(cx, |input, cx| input.set_value(message.summary, window, cx));
              this.commit_description.update(cx, |input, cx| input.set_value(message.description, window, cx));
            }
            Err(message) => window.push_notification(crate::failure(message), cx),
          }
        }
        cx.notify();
      });
    })
    .detach();
  }
}
