#[allow(dead_code)]
#[path = "../src/diff_model.rs"]
mod diff_model;

use diff_model::{changed_runs, expand, layout, Edge, Expansion, LineKind, Patch, Row, Unit};

/// A patch of a 40-line file with whole-file context in which line 20 was replaced and line 21 was added after it.
fn whole_file_patch() -> String {
  let mut patch = String::from("diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,40 +1,41 @@\n");
  for line in 1..=19 {
    patch.push_str(&format!(" line {line}\n"));
  }
  patch.push_str("-let value = compute(first, second);\n+let value = compute(first, third);\n+extra\n");
  for line in 21..=40 {
    patch.push_str(&format!(" line {line}\n"));
  }
  patch
}

#[test]
fn a_patch_is_read_into_numbered_lines() {
  let patch = Patch::parse(&whole_file_patch());
  assert_eq!(patch.lines.len(), 42);
  assert_eq!(patch.hunk_starts, [0]);
  let removed = &patch.lines[19];
  assert_eq!((removed.kind, removed.old, removed.new), (LineKind::Removed, Some(20), None));
  let added = &patch.lines[20];
  assert_eq!((added.kind, added.old, added.new), (LineKind::Added, None, Some(20)));
  let after = &patch.lines[22];
  assert_eq!((after.kind, after.old, after.new), (LineKind::Context, Some(21), Some(22)));
  assert_eq!(patch.digits(), 2);
}

#[test]
fn a_patch_without_hunks_has_no_lines() {
  assert!(Patch::parse("diff --git a/x.png b/x.png\nBinary files a/x.png and b/x.png differ\n").lines.is_empty());
  assert!(Patch::parse("").lines.is_empty());
}

#[test]
fn unchanged_lines_far_from_a_change_are_folded_with_the_header_of_what_follows() {
  let patch = Patch::parse(&whole_file_patch());
  let shown = layout(&patch, &Expansion::new(), false);
  let folds: Vec<_> = shown.rows.iter().filter(|row| matches!(row, Row::Fold { .. })).collect();
  assert_eq!(folds.len(), 2);
  match folds[0] {
    Row::Fold { hidden, edge, label, .. } => {
      assert_eq!((hidden.clone(), *edge), (0..16, Edge::Top));
      assert_eq!(label, "@@ -17,7 +17,8 @@");
    }
    _ => unreachable!(),
  }
  match folds[1] {
    Row::Fold { edge, label, .. } => assert_eq!((*edge, label.as_str()), (Edge::Bottom, "")),
    _ => unreachable!(),
  }
  // Three lines before the change, the three changed ones, three after.
  assert_eq!(shown.rows.len(), 1 + 3 + 3 + 3 + 1);
  assert_eq!(shown.change_starts, [1 + 3]);
}

#[test]
fn expanding_brings_back_lines_next_to_the_block_it_is_made_from() {
  let patch = Patch::parse(&whole_file_patch());
  let mut expansion = Expansion::new();
  let Row::Fold { hidden, base, .. } = layout(&patch, &expansion, false).rows[0].clone() else { panic!("the first row is a fold") };
  expand(&mut expansion, base, &hidden, true);
  let shown = layout(&patch, &expansion, false);
  // 16 hidden lines at the top, so one step of 20 brings back all of them and the fold goes.
  assert!(!matches!(shown.rows[0], Row::Fold { .. }));
  assert_eq!(shown.rows.iter().filter(|row| matches!(row, Row::Fold { .. })).count(), 1);
}

#[test]
fn a_fold_of_a_few_lines_between_changes_is_not_folded() {
  let mut patch = String::from("@@ -1,12 +1,12 @@\n-a\n+b\n");
  for line in 0..7 {
    patch.push_str(&format!(" same {line}\n"));
  }
  patch.push_str("-c\n+d\n");
  patch.push_str(" tail\n");
  let patch = Patch::parse(&patch);
  let shown = layout(&patch, &Expansion::new(), false);
  // The seven lines between the changes: three after the first, three before the second, one left over, which is shown.
  assert!(shown.rows.iter().all(|row| matches!(row, Row::Line(_))));
}

#[test]
fn only_the_words_that_changed_are_emphasized_and_never_the_indent() {
  let patch = Patch::parse(&whole_file_patch());
  let removed = &patch.lines[19].text;
  let emphasized: Vec<&str> = patch.emphasis[19].iter().map(|range| &removed[range.clone()]).collect();
  assert_eq!(emphasized, ["second"]);
  let (old, new) = changed_runs("  const a = 1;", "    const a = 2;", Unit::Word);
  let digit = |range: &std::ops::Range<usize>| (range.start, range.end);
  assert_eq!((old.iter().map(digit).collect::<Vec<_>>(), new.iter().map(digit).collect::<Vec<_>>()), (vec![(12, 13)], vec![(14, 15)]));
  let (old, new) = changed_runs("    alpha", "  alpha", Unit::Word);
  assert!(old.is_empty() && new.is_empty());
  let (old, new) = changed_runs("aaaa bbbb", "zzzzzzzzz yyyy", Unit::Word);
  assert!(old.is_empty() && new.is_empty());
}

#[test]
fn side_by_side_puts_removed_lines_beside_the_added_ones_that_follow() {
  let patch = Patch::parse("@@ -1,5 +1,6 @@\n a\n-b\n-c\n+B\n+C\n+D\n e\n");
  let shown = layout(&patch, &Expansion::new(), true);
  let pairs: Vec<_> = shown.rows.iter().filter_map(|row| if let Row::Pair(old, new) = row { Some((*old, *new)) } else { None }).collect();
  assert_eq!(pairs, [(Some(0), Some(0)), (Some(1), Some(3)), (Some(2), Some(4)), (None, Some(5)), (Some(6), Some(6))]);
  assert_eq!(shown.change_starts, [1]);
}

#[test]
fn characters_can_be_compared_instead_of_words() {
  let digit = |range: &std::ops::Range<usize>| (range.start, range.end);
  let (old, new) = changed_runs("let colour = 1;", "let color = 1;", Unit::Character);
  assert_eq!((old.iter().map(digit).collect::<Vec<_>>(), new.iter().map(digit).collect::<Vec<_>>()), (vec![(8, 9)], vec![]));
  // As words the whole word differs.
  let (old, new) = changed_runs("let colour = 1;", "let color = 1;", Unit::Word);
  assert_eq!((old.iter().map(digit).collect::<Vec<_>>(), new.iter().map(digit).collect::<Vec<_>>()), (vec![(4, 10)], vec![(4, 9)]));
}

#[test]
fn runs_of_changed_lines_are_groups_with_their_places_in_both_files() {
  let patch = Patch::parse(&whole_file_patch());
  let groups = patch.change_groups();
  assert_eq!(groups.len(), 1);
  assert_eq!((groups[0].lines.clone(), groups[0].old.clone(), groups[0].new.clone()), (19..22, 20..21, 20..22));

  let mut text = String::from("diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,6 +1,6 @@\n");
  text.push_str(" a\n+added\n b\n c\n-gone\n d\n");
  let groups = Patch::parse(&text).change_groups();
  assert_eq!(groups.len(), 2);
  // An added line sits before old line 2; a removed one was where new line 5 now is.
  assert_eq!((groups[0].old.clone(), groups[0].new.clone()), (2..2, 2..3));
  assert_eq!((groups[1].old.clone(), groups[1].new.clone()), (4..5, 5..5));
}
