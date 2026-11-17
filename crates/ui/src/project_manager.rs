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

/// A project row being dragged in the list.
#[derive(Clone)]
struct DraggedRow(PathBuf);

/// What follows the pointer while a row is dragged.
struct DragLabel(SharedString);

impl Render for DragLabel {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    div().px_3().py_1().rounded_md().border_1().border_color(theme.border).bg(theme.popover).text_color(theme.foreground).child(self.0.clone())
  }
}

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

  /// The title row of a group: the projects outside any category (`category` is `None`) or one category. Dropping a project on it
  /// moves the project into that group.
  fn group_header(&self, index: usize, category: Option<&Category>, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let target = category.map(|category| category.id.clone());
    let label = match category {
      None => "No category".to_string(),
      Some(category) if category.name.is_empty() => format!("Category {}", index + 1),
      Some(category) => category.name.clone(),
    };
    let key = target.clone().unwrap_or_else(|| "none".into());
    let mut row = h_flex()
      .id(SharedString::from(format!("group-{key}")))
      .gap_1()
      .items_center()
      .px_2()
      .py_1()
      .mt_2()
      .rounded_md()
      .drag_over::<DraggedRow>({
        let muted = theme.muted;
        move |style, _, _, _| style.bg(muted)
      })
      .on_drop(cx.listener(move |this, dragged: &DraggedRow, _, cx| {
        this.workspace.update(cx, |workspace, cx| workspace.move_project(&dragged.0, target.as_deref(), cx));
      }))
      .child(Icon::empty().path("icons/folder.svg").size_4().text_color(theme.muted_foreground))
      .child(Label::new(label).font_weight(FontWeight::SEMIBOLD))
      .child(Label::new(format!("{count}")).text_color(theme.muted_foreground).text_size(FONT_SIZE_SM))
      .child(div().flex_1());
    if let Some(category) = category {
      let (rename, delete, up, down) = (category.id.clone(), category.id.clone(), category.id.clone(), category.id.clone());
      row = row
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
          format!("category-delete-{}", category.id),
          "icons/trash.svg",
          "Delete category (the projects stay)",
          cx,
          move |workspace, _, cx| workspace.delete_category(&delete, cx),
        ));
    }
    row
  }

  fn project_row(&self, project: &Project, categories: &[(String, SharedString)], cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let selected = self.workspace.read(cx).project_list().active.as_deref() == Some(project.path.as_path());
    let look = avatar_look(project, selected, cx);
    let path: PathBuf = project.path.clone();
    let key = path.display().to_string();
    let (edit, up, down, remove) = (path.clone(), path.clone(), path.clone(), path.clone());
    let current = project.category.clone();
    let current_label =
      categories.iter().find(|(id, _)| Some(id) == current.as_ref()).map_or_else(|| SharedString::from("No category"), |(_, label)| label.clone());
    let categories = categories.to_vec();
    let workspace = self.workspace.clone();
    let menu_path = path.clone();
    let name = SharedString::from(project.name());
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
      .id(SharedString::from(format!("row-{key}")))
      .gap_2()
      .items_center()
      .pl_2()
      .pr_2()
      .py_1()
      .rounded_md()
      .hover(|row| row.bg(theme.muted))
      .drag_over::<DraggedRow>({
        let muted = theme.muted;
        move |style, _, _, _| style.bg(muted)
      })
      .on_drag(DraggedRow(path.clone()), {
        let name = name.clone();
        move |_, _, _, cx| cx.new(|_| DragLabel(name.clone()))
      })
      .on_drop(cx.listener({
        let target = path.clone();
        move |this, dragged: &DraggedRow, _, cx| {
          this.workspace.update(cx, |workspace, cx| workspace.move_project_before(&dragged.0, &target, cx));
        }
      }))
      .child(Icon::empty().path("icons/grip-vertical.svg").size_4().text_color(theme.muted_foreground))
      .child(avatar)
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .child(Label::new(name).whitespace_nowrap().overflow_hidden().text_ellipsis())
          .child(Label::new(folder).text_color(theme.muted_foreground).text_size(FONT_SIZE_SM).whitespace_nowrap().overflow_hidden().text_ellipsis()),
      )
      .child(self.action(format!("project-edit-{key}"), "icons/sliders-horizontal.svg", "Name, icon and color", cx, move |workspace, window, cx| {
        workspace.open_project_settings(&edit, window, cx)
      }))
      .child(
        div().w_32().flex_none().child(
          Button::new(SharedString::from(format!("project-category-{key}")))
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/folder-input.svg"))
            .label(current_label)
            .tooltip("Move to a category")
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
        ),
      )
      .child(
        self
          .action(format!("project-up-{key}"), "icons/chevron-up.svg", "Move up", cx, move |workspace, _, cx| workspace.move_project_by(&up, -1, cx)),
      )
      .child(self.action(format!("project-down-{key}"), "icons/chevron-down.svg", "Move down", cx, move |workspace, _, cx| {
        workspace.move_project_by(&down, 1, cx)
      }))
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
    let new_category =
      Button::new("new-category").small().outline().icon(Icon::empty().path("icons/folder-plus.svg")).label("New category").on_click(cx.listener(
        |this, _, window, cx| {
          this.workspace.update(cx, |workspace, cx| workspace.create_category(window, cx));
        },
      ));
    v_flex()
      .gap_1()
      .child(
        h_flex()
          .gap_2()
          .items_center()
          .child(
            Label::new("Drag a row onto a category or onto another project.").text_color(theme.muted_foreground).text_size(FONT_SIZE_SM).flex_1(),
          )
          .child(new_category),
      )
      .when(list.projects.is_empty(), |column| {
        column.child(Label::new("No projects yet. Add one with + in the project bar.").text_color(theme.muted_foreground).py_2())
      })
      .when(!loose.is_empty() || !list.categories.is_empty(), |column| {
        column.child(self.group_header(0, None, loose.len(), cx)).children(loose.iter().map(|project| self.project_row(project, &categories, cx)))
      })
      .children(list.categories.iter().enumerate().map(|(index, category)| {
        let members: Vec<&Project> = list.projects_in(Some(&category.id)).collect();
        v_flex()
          .gap_1()
          .child(self.group_header(index, Some(category), members.len(), cx))
          .when(members.is_empty(), |group| {
            group.child(
              Label::new("Empty. Drop a project here or pick this category in a project's menu.")
                .text_color(theme.muted_foreground)
                .text_size(FONT_SIZE_SM)
                .pl_8()
                .py_1(),
            )
          })
          .children(members.iter().map(|project| self.project_row(project, &categories, cx)))
      }))
      .into_any_element()
  }
}
