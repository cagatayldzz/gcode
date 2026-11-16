use std::{collections::HashSet, fs};

#[path = "../src/explorer.rs"]
mod explorer;

#[test]
fn folders_come_first_and_only_expanded_folders_show_their_children() {
  let root = std::env::temp_dir().join(format!("vekro-explorer-{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(root.join("src")).unwrap();
  fs::create_dir_all(root.join(".git")).unwrap();
  fs::write(root.join("src/main.rs"), "").unwrap();
  fs::write(root.join("Cargo.toml"), "").unwrap();

  let collapsed: Vec<_> = explorer::rows(&root, &HashSet::new()).into_iter().map(|row| (row.name, row.depth)).collect();
  assert_eq!(collapsed, [("src".to_string(), 0), ("Cargo.toml".to_string(), 0)]);

  let expanded: HashSet<_> = [root.join("src")].into();
  let open: Vec<_> = explorer::rows(&root, &expanded).into_iter().map(|row| (row.name, row.depth)).collect();
  assert_eq!(open, [("src".to_string(), 0), ("main.rs".to_string(), 1), ("Cargo.toml".to_string(), 0)]);
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn creating_an_entry_makes_subfolders_and_refuses_to_leave_or_replace() {
  let root = std::env::temp_dir().join(format!("vekro-explorer-create-{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).unwrap();

  let file = explorer::create(&root, "src/lib.rs", false).unwrap();
  assert!(file.is_file());
  let folder = explorer::create(&root, "docs", true).unwrap();
  assert!(folder.is_dir());

  assert!(explorer::create(&root, "src/lib.rs", false).is_err());
  assert!(explorer::create(&root, "  ", false).is_err());
  assert!(explorer::create(&root, "../outside", false).is_err());
  assert!(explorer::create(&root, "/etc/outside", false).is_err());
  fs::remove_dir_all(root).unwrap();
}
