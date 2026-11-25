use ws_git::conflict_markers::conflict_regions;

#[test]
fn finds_every_closed_conflict_block_with_its_sides() {
  let text = "a\n<<<<<<< test\nours 1\nours 2\n=======\ntheirs\n>>>>>>> develop\nb\n<<<<<<< test\nx\n=======\ny\n>>>>>>> develop\n";
  let regions = conflict_regions(text);
  assert_eq!(regions.len(), 2);
  assert_eq!((regions[0].lines.clone(), regions[0].ours.clone(), regions[0].theirs.clone()), (1..7, 2..4, 5..6));
  assert_eq!(regions[1].lines, 8..13);
}

#[test]
fn skips_unclosed_blocks_and_ignores_plain_text() {
  assert!(conflict_regions("<<<<<<< test\nours\n=======\ntheirs\n").is_empty());
  assert!(conflict_regions("a\n==========\nb\n").is_empty());
  assert!(conflict_regions("").is_empty());
}
