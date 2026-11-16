//! The project list in Settings: every project and category of the rail, with what can be done to each.

use crate::{
  components::{avatar_look, display_path},
  theme::{colors, FONT_SIZE_SM},
  workspace::Workspace,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme, Icon, Sizable as _,
  },
  *,
};
use std::path::{Path, PathBuf};
use vekro_storage::{Category, Project};

pub(crate) struct ProjectManager {
  workspace: Entity<Workspace>,
  _observer: Subscription,
}

impl ProjectManager {
  pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
    // The rail changes whenever the workspace does (a drag, a menu, this list), so the list redraws with it.
    let observer = cx.observe(&workspace, |_, _, cx| cx.notify());
    Self { workspace, _observer: observer }
  }

  /// A small icon button that acts on the workspace.
  fn action(
    &self,
    id: impl Into<ElementId>,
    icon: &'static str,
    tooltip: &'static str,
    cx: &mut Context<Self>,
    act: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
  ) -> Button {
    Button::new(id).ghost().xsmall().icon(Icon::empty().path(icon)).tooltip(tooltip).on_click(cx.listener(move |this, _, window, cx| {
      this.workspace.update(cx, |workspace, cx| act(workspace, window, cx));
    }))
  }

  fn category_row(&self, index: usize, category: &Category, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let label = if category.name.is_empty() { format!("Category {}", index + 1) } else { category.name.clone() };
    let (rename, delete, up, down, fold) = (category.id.clone(), category.id.clone(), category.id.clone(), category.id.clone(), category.id.clone());
    h_flex()
      .gap_1()
      .items_center()
      .px_2()
      .pt_3()
      .child(Icon::empty().path("icons/folder.svg").size_4().text_color(theme.muted_foreground))
      .child(Label::new(label).font_weight(FontWeight::SEMIBOLD))
      .child(Label::new(format!("{count}")).text_color(theme.muted_foreground).text_size(FONT_SIZE_SM))
      .child(div().flex_1())
      .child(self.action(format!("category-rename-{}", category.id), "icons/pencil.svg", "Rename", cx, move |workspace, window, cx| {
        workspace.rename_category(rename.clone(), window, cx)
      }))
      .child(self.action(format!("category-up-{}", category.id), "icons/chevron-up.svg", "Move up", cx, move |workspace, _, cx| {
        workspace.move_category_by(&up, -1, cx)
      }))
      .child(self.action(format!("category-down-{}", category.id), "icons/chevron-down.svg", "Move down", cx, move |workspace, _, cx| {
        workspace.move_category_by(&down, 1, cx)
      }))
      .child(self.action(
        format!("category-fold-{}", category.id),
        if category.collapsed { "icons/folder.svg" } else { "icons/folder-open.svg" },
        if category.collapsed { "Unfold in the rail" } else { "Fold in the rail" },
        cx,
        move |workspace, _, cx| workspace.toggle_category(&fold, cx),
      ))
      .child(self.action(
        format!("category-delete-{}", category.id),
        "icons/trash.svg",
        "Delete category (the projects stay)",
        cx,
        move |workspace, _, cx| workspace.delete_category(&delete, cx),
      ))
  }

  fn project_row(&self, project: &Project, categories: &[(String, SharedString)], cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let selected = self.workspace.read(cx).project_list().active.as_deref() == Some(project.path.as_path());
    let look = avatar_look(project, selected, cx);
    let path: PathBuf = project.path.clone();
    let key = path.display().to_string();
    let edit = path.clone();
    let up = path.clone();
    let down = path.clone();
    let remove = path.clone();
    let current = project.category.clone();
    let categories = categories.to_vec();
    let workspace = self.workspace.clone();
    let menu_path = path.clone();
    let folder = display_path(Path::new(&project.path));
    let avatar = div()
      .size_8()
      .flex()
      .flex_none()
      .items_center()
      .justify_center()
      .rounded_md()
      .font_weight(FontWeight::MEDIUM)
      .text_color(look.text)
      .bg(look.fill.unwrap_or(palette.rail_hover))
      .when_some(look.icon.clone(), |avatar, icon| avatar.child(Icon::empty().path(icon).size_4()))
      .when(look.icon.is_none(), |avatar| avatar.child(look.letter.clone()));
    h_flex()
      .gap_2()
      .items_center()
      .pl_6()
      .pr_2()
      .py_1()
      .rounded_md()
      .hover(|row| row.bg(theme.muted))
      .child(avatar)
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .child(Label::new(project.name()).whitespace_nowrap().overflow_hidden().text_ellipsis())
          .child(Label::new(folder).text_color(theme.muted_foreground).text_size(FONT_SIZE_SM).whitespace_nowrap().overflow_hidden().text_ellipsis()),
      )
      .child(self.action(format!("project-edit-{key}"), "icons/sliders-horizontal.svg", "Name, icon and color", cx, move |workspace, window, cx| {
        workspace.open_project_settings(&edit, window, cx)
      }))
      .child(
        self
          .action(format!("project-up-{key}"), "icons/chevron-up.svg", "Move up", cx, move |workspace, _, cx| workspace.move_project_by(&up, -1, cx)),
      )
      .child(self.action(format!("project-down-{key}"), "icons/chevron-down.svg", "Move down", cx, move |workspace, _, cx| {
        workspace.move_project_by(&down, 1, cx)
      }))
      .child(
        Button::new(SharedString::from(format!("project-category-{key}")))
          .ghost()
          .xsmall()
          .icon(Icon::empty().path("icons/folder-input.svg"))
          .tooltip("Category")
          .dropdown_menu(move |menu, _, _| {
            let mut menu = menu;
            for (id, label) in &categories {
              let (workspace, path, id_owned) = (workspace.clone(), menu_path.clone(), id.clone());
              menu = menu.item(
                PopupMenuItem::new(label.clone())
                  .checked(current.as_deref() == Some(id.as_str()))
                  .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&path, Some(&id_owned), cx))),
              );
            }
            if !categories.is_empty() {
              menu = menu.separator();
            }
            let (new_workspace, new_path) = (workspace.clone(), menu_path.clone());
            menu = menu.item(
              PopupMenuItem::new("New category…")
                .on_click(move |_, window, cx| new_workspace.update(cx, |this, cx| this.new_category(new_path.clone(), window, cx))),
            );
            if current.is_some() {
              let (workspace, path) = (workspace.clone(), menu_path.clone());
              menu = menu.item(
                PopupMenuItem::new("No category").on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&path, None, cx))),
              );
            }
            menu
          }),
      )
      .child(self.action(format!("project-remove-{key}"), "icons/trash.svg", "Remove from Vekro", cx, move |workspace, window, cx| {
        workspace.confirm_remove_project(remove.clone(), window, cx)
      }))
  }
}

impl Render for ProjectManager {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let list = self.workspace.read(cx).project_list().clone();
    let categories: Vec<(String, SharedString)> = list
      .categories
      .iter()
      .enumerate()
      .map(|(index, category)| {
        (category.id.clone(), SharedString::from(if category.name.is_empty() { format!("Category {}", index + 1) } else { category.name.clone() }))
      })
      .collect();
    let loose: Vec<&Project> = list.projects_in(None).collect();
    if list.projects.is_empty() {
      return Label::new("No projects yet. Add one with + in the project bar.").text_color(theme.muted_foreground).into_any_element();
    }
    v_flex()
      .gap_1()
      .when(!loose.is_empty(), |column| {
        column
          .child(
            h_flex()
              .gap_1()
              .items_center()
              .px_2()
              .child(Icon::empty().path("icons/folder.svg").size_4().text_color(theme.muted_foreground))
              .child(Label::new("No category").font_weight(FontWeight::SEMIBOLD)),
          )
          .children(loose.iter().map(|project| self.project_row(project, &categories, cx)))
      })
      .children(list.categories.iter().enumerate().map(|(index, category)| {
        let members: Vec<&Project> = list.projects_in(Some(&category.id)).collect();
        v_flex()
          .gap_1()
          .child(self.category_row(index, category, members.len(), cx))
          .children(members.iter().map(|project| self.project_row(project, &categories, cx)))
      }))
      .into_any_element()
  }
}
