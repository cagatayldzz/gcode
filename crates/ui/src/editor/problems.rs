//! The problems a language server finds in the open files: red and yellow underlines in the editor, the counts in the status
//! bar and the list in the Problems panel.

use super::*;
use futures::StreamExt as _;
use gpui_kit::base::input::{Diagnostic, DiagnosticSeverity, Position};
use ws_language::{Problem, Server, Session, Severity};

/// How long typing pauses before the language server is told the new text.
const SYNC_DELAY: Duration = Duration::from_millis(300);

/// A running language server and the task that takes in what it reports.
pub(super) struct ProjectLanguage {
  session: Arc<Session>,
  _task: Task<()>,
}

/// The problems of one file, errors and warnings only, in the order they appear in the text.
pub(crate) struct FileProblems {
  pub path: PathBuf,
  pub problems: Vec<Problem>,
}

impl EditorPanel {
  pub(crate) fn sync_shown_file(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    if !self.servers.contains_key(path) {
      self.sync_problems(path, window, cx);
    }
  }

  pub(crate) fn enable_language_servers(&mut self, project: PathBuf, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.set_root(Some(project), cx);
    if self.trusted_language_root.as_ref() == Some(&root) {
      return;
    }
    self.disable_language_servers(cx);
    self.trusted_language_root = Some(root);
    if let Some(path) = self.active_file().map(Path::to_path_buf) {
      self.sync_problems(&path, window, cx);
    }
    cx.notify();
  }

  pub(crate) fn disable_language_servers(&mut self, cx: &mut Context<Self>) {
    self.trusted_language_root = None;
    self.sync_tasks.clear();
    self.servers.clear();
    self.languages.clear();
    for (path, _) in std::mem::take(&mut self.problems) {
      self.take_problems(path, Vec::new(), cx);
    }
    cx.notify();
  }

  /// Whether a language server is running for some project: only then do zero problems mean something.
  pub fn has_language_server(&self) -> bool {
    self.languages.values().any(Option::is_some)
  }

  /// The files under `root` that have problems, sorted by path.
  pub fn problems_under(&self, root: &Path) -> Vec<FileProblems> {
    let mut found: Vec<FileProblems> = self
      .problems
      .iter()
      .filter(|(path, problems)| path.starts_with(root) && !problems.is_empty())
      .map(|(path, problems)| FileProblems { path: path.clone(), problems: problems.clone() })
      .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
  }

  /// The number of errors and of warnings under `root`.
  pub fn problem_counts(&self, root: &Path) -> (usize, usize) {
    let mut counts = (0, 0);
    for (path, problems) in &self.problems {
      if path.starts_with(root) {
        for problem in problems {
          if problem.severity == Severity::Error {
            counts.0 += 1;
          } else {
            counts.1 += 1;
          }
        }
      }
    }
    counts
  }

  /// Tells the language server the text of a file, starting the project's server the first time.
  pub(super) fn sync_problems(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    if self.read_only || self.trusted_language_root.is_none() || ws_language::language_id(path).is_none() {
      return;
    }
    let Some(file) = self.files.get(path) else { return };
    let server =
      self.servers.entry(path.to_path_buf()).or_insert_with(|| ws_language::find_server(path, self.trusted_language_root.as_deref())).clone();
    let Some(server) = server else { return };
    let text = file.state.read(cx).value();
    if !self.languages.contains_key(&server.root) {
      let language = Self::start_language(&server, window, cx);
      self.languages.insert(server.root.clone(), language);
    }
    if let Some(Some(language)) = self.languages.get(&server.root) {
      language.session.sync(path, &text);
    }
  }

  fn start_language(server: &Server, window: &mut Window, cx: &mut Context<Self>) -> Option<ProjectLanguage> {
    let (session, mut updates) = match Session::start(server) {
      Ok(started) => started,
      Err(error) => {
        window.push_notification(crate::failure(format!("Could not start the language server: {error}")), cx);
        return None;
      }
    };
    let task = cx.spawn_in(window, async move |this, cx| {
      while let Some(update) = updates.next().await {
        if this.update_in(cx, |this, _, cx| this.take_problems(update.path, update.problems, cx)).is_err() {
          break;
        }
      }
    });
    Some(ProjectLanguage { session: Arc::new(session), _task: task })
  }

  /// Tells the language server again after typing pauses.
  pub(super) fn schedule_problem_sync(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    if self.trusted_language_root.is_none() || ws_language::language_id(path).is_none() {
      return;
    }
    let owner = path.to_path_buf();
    let task = cx.spawn_in(window, async move |this, cx| {
      cx.background_executor().timer(SYNC_DELAY).await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.sync_tasks.remove(&owner);
        this.sync_problems(&owner, window, cx);
      });
    });
    self.sync_tasks.insert(path.to_path_buf(), task);
  }

  /// Draws what the server found in a file under its text, and keeps it for the status bar and the Problems panel.
  fn take_problems(&mut self, path: PathBuf, mut problems: Vec<Problem>, cx: &mut Context<Self>) {
    problems.retain(|problem| matches!(problem.severity, Severity::Error | Severity::Warning));
    problems.sort_by_key(|problem| (problem.line, problem.column));
    let Some(file) = self.files.get(&path) else { return };
    file.state.update(cx, |state, cx| {
      let text = state.text().clone();
      if let Some(set) = state.diagnostics_mut() {
        set.reset(&text);
        set.extend(problems.iter().map(diagnostic));
      }
      cx.notify();
    });
    if problems.is_empty() {
      self.problems.remove(&path);
    } else {
      self.problems.insert(path, problems);
    }
    cx.notify();
  }

  /// The language server that serves a file, if one runs.
  pub(super) fn session_for(&self, path: &Path) -> Option<Arc<Session>> {
    let server = self.servers.get(path)?.as_ref()?;
    Some(self.languages.get(&server.root)?.as_ref()?.session.clone())
  }

  /// Forgets a file's problems and tells the server it is closed.
  pub(super) fn release_problems(&mut self, path: &Path) {
    self.problems.remove(path);
    self.sync_tasks.remove(path);
    if let Some(Some(server)) = self.servers.remove(path) {
      if let Some(Some(language)) = self.languages.get(&server.root) {
        language.session.close(path);
      }
    }
  }
}

fn diagnostic(problem: &Problem) -> Diagnostic {
  Diagnostic {
    range: Position { line: problem.line, character: problem.column }..Position { line: problem.end_line, character: problem.end_column },
    severity: if problem.severity == Severity::Error { DiagnosticSeverity::Error } else { DiagnosticSeverity::Warning },
    code: problem.code.clone().map(Into::into),
    source: problem.source.clone().map(Into::into),
    message: problem.message.clone().into(),
    ..Default::default()
  }
}
