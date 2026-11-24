//! The Problems panel: the errors and warnings the language server found in the project's files.

use super::*;
use ws_language::Severity;

impl Workspace {
  /// Opens the file of a problem with the cursor on it.
  fn open_problem(&mut self, path: PathBuf, line: u32, column: u32, window: &mut Window, cx: &mut Context<Self>) {
    self.open_file(path.clone(), window, cx);
    self.editor_panel.update(cx, |editor, cx| editor.reveal_position(&path, line as usize, column as usize, window, cx));
  }

  /// The list of problems under the editor, opened from the status bar's counts. A row opens the file at the problem.
  pub(super) fn render_problems_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
    let root = self.projects.active.clone()?;
    let palette = colors(cx);
    let files = self.editor_panel.read(cx).problems_under(&root);
    let total: usize = files.iter().map(|file| file.problems.len()).sum();
    let mut list = v_flex().id("problems-list").size_full().pb_2().overflow_y_scrollbar();
    for file in files {
      let name = file.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
      let folder =
        file.path.parent().and_then(|parent| parent.strip_prefix(&root).ok()).map(|parent| parent.display().to_string()).unwrap_or_default();
      list = list.child(
        h_flex()
          .px_3()
          .pt_2()
          .pb_1()
          .gap_2()
          .items_center()
          .child(Label::new(name).font_weight(FontWeight::SEMIBOLD))
          .child(Label::new(folder).text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
          .child(Label::new(file.problems.len().to_string()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary)),
      );
      for problem in file.problems {
        let (icon, color) = match problem.severity {
          Severity::Error => ("icons/circle-x.svg", palette.deleted),
          _ => ("icons/triangle-alert.svg", palette.modified),
        };
        let origin = match (&problem.source, &problem.code) {
          (Some(source), Some(code)) => format!("{source}({code})"),
          (None, Some(code)) => code.clone(),
          (Some(source), None) => source.clone(),
          (None, None) => String::new(),
        };
        let id = SharedString::from(format!(
          "problem:{}:{}:{}:{}",
          file.path.display(),
          problem.line,
          problem.column,
          problem.code.as_deref().unwrap_or("")
        ));
        let path = file.path.clone();
        let (line, column) = (problem.line, problem.column);
        list = list.child(
          h_flex()
            .id(id)
            .px_6()
            .py_1()
            .gap_2()
            .items_center()
            .cursor_pointer()
            .hover(|row| row.bg(palette.hover))
            .child(Icon::empty().path(icon).size_4().flex_none().text_color(color))
            .child(Label::new(problem.message).flex_none().max_w_full().overflow_hidden().whitespace_nowrap().text_ellipsis())
            .child(Label::new(origin).flex_none().text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
            .child(
              Label::new(format!("[Ln {}, Col {}]", problem.line + 1, problem.column + 1))
                .flex_none()
                .text_size(FONT_SIZE_SM)
                .text_color(palette.text_secondary),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_problem(path.clone(), line, column, window, cx))),
        );
      }
    }
    if total == 0 {
      list = list.child(div().px_3().pt_2().child(Label::new("No problems have been detected in the project.").text_color(palette.text_secondary)));
    }
    Some(
      v_flex()
        .flex_none()
        .h_56()
        .w_full()
        .bg(palette.background)
        .border_t_1()
        .border_color(palette.border)
        .child(
          h_flex()
            .flex_none()
            .px_3()
            .py_1()
            .gap_2()
            .items_center()
            .child(Label::new("Problems").font_weight(FontWeight::SEMIBOLD))
            .child(Label::new(total.to_string()).text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
            .child(div().flex_1())
            .child(Button::new("problems-close").ghost().xsmall().icon(Icon::new(IconName::Close)).tooltip("Close").on_click(cx.listener(
              |this, _, _, cx| {
                this.problems_open = false;
                cx.notify();
              },
            ))),
        )
        .child(div().flex_1().min_h_0().child(list))
        .into_any_element(),
    )
  }
}
