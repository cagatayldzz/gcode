#[path = "../src/job_scope.rs"]
#[allow(dead_code)]
mod job_scope;

use job_scope::{lands_here, name_the_place, remember_status, shown_in, Draft, Drafts};
use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Status {
  changed: usize,
  push: bool,
}

fn gcode() -> &'static Path {
  Path::new("/work/gcode")
}

fn wfhub() -> &'static Path {
  Path::new("/work/wfhub")
}

// A job lands where the user is, or does not.

#[test]
fn a_job_lands_in_the_window_only_while_its_project_is_open() {
  assert!(lands_here(Some(gcode()), gcode()));
  assert!(!lands_here(Some(wfhub()), gcode()), "the user moved to another project while the job ran");
  assert!(!lands_here(None, gcode()), "no project is open any more");
}

#[test]
fn a_project_is_told_apart_by_its_whole_path_not_by_a_prefix() {
  assert!(!lands_here(Some(Path::new("/work/gcode-old")), gcode()));
  assert!(!lands_here(Some(Path::new("/work/gcode/sub")), gcode()));
  assert!(!lands_here(Some(Path::new("/work")), gcode()));
}

#[test]
fn work_is_shown_only_in_the_project_that_owns_it() {
  assert!(shown_in(gcode(), Some(gcode())));
  assert!(!shown_in(gcode(), Some(wfhub())));
  assert!(!shown_in(gcode(), None));
}

// A message names the project when the job ended elsewhere.

#[test]
fn a_message_for_the_open_project_is_left_alone() {
  assert_eq!(name_the_place("Pushed origin.", None), "Pushed origin.");
}

#[test]
fn a_message_for_another_project_names_it_before_the_full_stop() {
  assert_eq!(name_the_place("Pushed origin.", Some("gcode")), "Pushed origin (gcode).");
  assert_eq!(name_the_place("Switched to feature/login.", Some("wfhub")), "Switched to feature/login (wfhub).");
}

#[test]
fn a_message_without_a_full_stop_gets_the_name_at_the_end() {
  assert_eq!(name_the_place("Deleted topic", Some("gcode")), "Deleted topic (gcode)");
}

#[test]
fn only_the_closing_full_stop_moves_and_dots_inside_stay() {
  assert_eq!(name_the_place("Pushed v1.2.3.", Some("gcode")), "Pushed v1.2.3 (gcode).");
  assert_eq!(name_the_place("Committed a.b.", Some("x")), "Committed a.b (x).");
}

#[test]
fn an_empty_message_still_names_the_place() {
  assert_eq!(name_the_place("", Some("gcode")), " (gcode)");
}

// What the tabs remember.

fn known() -> HashMap<PathBuf, Status> {
  HashMap::from([(gcode().to_path_buf(), Status { changed: 0, push: true })])
}

#[test]
fn a_new_status_replaces_a_stale_one_and_says_it_changed() {
  let mut known = known();
  assert!(remember_status(&mut known, gcode(), Some(Status { changed: 0, push: false })));
  assert_eq!(known[gcode()], Status { changed: 0, push: false });
}

#[test]
fn the_same_status_changes_nothing_so_nothing_is_redrawn() {
  let mut known = known();
  assert!(!remember_status(&mut known, gcode(), Some(Status { changed: 0, push: true })));
}

#[test]
fn a_status_that_could_not_be_read_keeps_what_was_known() {
  let mut known = known();
  assert!(!remember_status(&mut known, gcode(), None));
  assert_eq!(known[gcode()], Status { changed: 0, push: true });
}

#[test]
fn a_project_not_known_yet_is_added() {
  let mut known = known();
  assert!(remember_status(&mut known, wfhub(), Some(Status { changed: 3, push: false })));
  assert_eq!(known.len(), 2);
}

#[test]
fn remembering_one_project_leaves_the_others_alone() {
  let mut known = known();
  known.insert(wfhub().to_path_buf(), Status { changed: 2, push: true });
  remember_status(&mut known, gcode(), Some(Status { changed: 0, push: false }));
  assert_eq!(known[wfhub()], Status { changed: 2, push: true });
}

#[test]
fn leaving_a_project_right_after_a_push_does_not_leave_its_mark_behind() {
  // The tab read ten seconds ago: one commit waiting. The window then pushed it and is left for another project.
  let mut known = known();
  let window_knows_now = Some(Status { changed: 0, push: false });
  remember_status(&mut known, gcode(), window_knows_now);
  assert!(!known[gcode()].push);
}

// Commit drafts.

fn draft(summary: &str, description: &str) -> Draft {
  Draft { summary: summary.into(), description: description.into() }
}

#[test]
fn a_draft_comes_back_with_its_project() {
  let mut drafts = Drafts::default();
  drafts.put_away(gcode(), draft("Fix the badge", "It spun."));
  assert_eq!(drafts.take(gcode()), draft("Fix the badge", "It spun."));
}

#[test]
fn a_draft_is_not_shown_in_another_project() {
  let mut drafts = Drafts::default();
  drafts.put_away(gcode(), draft("Fix the badge", ""));
  assert_eq!(drafts.take(wfhub()), Draft::default(), "the other project starts with empty boxes");
  assert_eq!(drafts.take(gcode()), draft("Fix the badge", ""), "and the first one still has its own");
}

#[test]
fn taking_a_draft_empties_its_place() {
  let mut drafts = Drafts::default();
  drafts.put_away(gcode(), draft("a", "b"));
  drafts.take(gcode());
  assert_eq!(drafts.take(gcode()), Draft::default());
}

#[test]
fn an_empty_box_puts_nothing_away_and_forgets_an_older_draft() {
  let mut drafts = Drafts::default();
  drafts.put_away(gcode(), draft("old", ""));
  drafts.put_away(gcode(), Draft::default());
  assert_eq!(drafts.take(gcode()), Draft::default());
}

#[test]
fn a_description_alone_is_a_draft() {
  assert!(!draft("", "only a description").is_empty());
  assert!(draft("", "").is_empty());
}

#[test]
fn a_commit_made_in_a_project_that_is_not_open_clears_its_draft() {
  let mut drafts = Drafts::default();
  drafts.put_away(gcode(), draft("Fix the badge", "It spun."));
  drafts.clear(gcode());
  assert_eq!(drafts.take(gcode()), Draft::default());
}

#[test]
fn clearing_one_draft_leaves_the_others() {
  let mut drafts = Drafts::default();
  drafts.put_away(gcode(), draft("a", ""));
  drafts.put_away(wfhub(), draft("b", ""));
  drafts.clear(gcode());
  assert_eq!(drafts.take(wfhub()), draft("b", ""));
}

#[test]
fn the_message_of_an_undone_commit_waits_in_the_project_it_was_taken_back_in() {
  let mut drafts = Drafts::default();
  drafts.set(gcode(), draft("Undone", "message"));
  assert_eq!(drafts.take(wfhub()), Draft::default());
  assert_eq!(drafts.take(gcode()), draft("Undone", "message"));
}

#[test]
fn leaving_and_coming_back_without_typing_keeps_the_boxes_empty() {
  let mut drafts = Drafts::default();
  drafts.put_away(gcode(), Draft::default());
  assert_eq!(drafts.take(gcode()), Draft::default());
}

#[test]
fn switching_back_and_forth_keeps_each_projects_text() {
  let mut drafts = Drafts::default();
  // gcode open, user types, goes to wfhub, types, comes back, goes away again.
  drafts.put_away(gcode(), draft("g1", "gd"));
  let wfhub_box = drafts.take(wfhub());
  assert_eq!(wfhub_box, Draft::default());
  drafts.put_away(wfhub(), draft("w1", ""));
  let gcode_box = drafts.take(gcode());
  assert_eq!(gcode_box, draft("g1", "gd"));
  drafts.put_away(gcode(), draft("g2", "gd"));
  assert_eq!(drafts.take(wfhub()), draft("w1", ""));
  assert_eq!(drafts.take(gcode()), draft("g2", "gd"));
}
