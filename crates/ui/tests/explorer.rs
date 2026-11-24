use std::{collections::HashSet, fs};

#[path = "../src/explorer.rs"]
mod explorer;

#[test]
fn folders_come_first_and_only_expanded_folders_show_their_children() {
  let root = std::env::temp_dir().join(format!("ws-explorer-{}", std::process::id()));
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
  let root = std::env::temp_dir().join(format!("ws-explorer-create-{}", std::process::id()));
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

#[test]
fn copying_and_moving_files_preserve_existing_destinations() {
  let root = tempfile::tempdir().unwrap();
  let source = root.path().join("notes.md");
  let existing = root.path().join("notes copy.md");
  let moved = root.path().join("renamed.md");
  fs::write(&source, "original").unwrap();
  fs::write(&existing, "existing").unwrap();
  let metadata = fs::symlink_metadata(&source).unwrap();

  assert!(explorer::move_file(&source, &existing).is_err());
  assert!(explorer::copy_file(&source, &existing, &metadata, false).is_err());
  assert_eq!(fs::read_to_string(&existing).unwrap(), "existing");

  let copy = explorer::copy_file(&source, &source, &metadata, true).unwrap();
  assert_eq!(copy, root.path().join("notes copy 2.md"));
  assert_eq!(fs::read_to_string(copy).unwrap(), "original");
  explorer::move_file(&source, &moved).unwrap();
  assert!(!source.exists());
  assert_eq!(fs::read_to_string(moved).unwrap(), "original");
}

#[test]
fn entries_ignored_by_git_are_marked_and_their_children_inherit_it() {
  let root = tempfile::tempdir().unwrap();
  git2::Repository::init(root.path()).unwrap();
  fs::create_dir_all(root.path().join("target/debug")).unwrap();
  fs::create_dir_all(root.path().join("src")).unwrap();
  fs::write(root.path().join(".gitignore"), "/target/\n").unwrap();
  fs::write(root.path().join("src/main.rs"), "").unwrap();

  let expanded: HashSet<_> = [root.path().join("target"), root.path().join("src")].into();
  let rows: Vec<_> = explorer::rows(root.path(), &expanded).into_iter().map(|row| (row.name, row.ignored)).collect();
  assert_eq!(
    rows,
    [
      ("src".to_string(), false),
      ("main.rs".to_string(), false),
      ("target".to_string(), true),
      ("debug".to_string(), true),
      (".gitignore".to_string(), false)
    ]
  );
}

#[test]
fn names_are_sorted_by_bytes_as_on_github() {
  let root = std::env::temp_dir().join(format!("ws-explorer-order-{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).unwrap();
  for folder in ["scripts", ".cargo", "apps", "Docs"] {
    fs::create_dir(root.join(folder)).unwrap();
  }
  for file in ["rustfmt.toml", "README.md", "bun.lock", "Cargo.toml", "AGENTS.md", ".gitignore", "clippy.toml"] {
    fs::write(root.join(file), "").unwrap();
  }
  let names: Vec<_> = explorer::rows(&root, &HashSet::new()).into_iter().map(|row| row.name).collect();
  assert_eq!(
    names,
    [".cargo", "Docs", "apps", "scripts", ".gitignore", "AGENTS.md", "Cargo.toml", "README.md", "bun.lock", "clippy.toml", "rustfmt.toml"]
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn copying_a_folder_copies_its_contents_and_refuses_to_enter_itself() {
  let root = tempfile::tempdir().unwrap();
  let source = root.path().join("pages");
  fs::create_dir_all(source.join("nested")).unwrap();
  fs::write(source.join("nested/page.tsx"), "page").unwrap();
  let metadata = fs::symlink_metadata(&source).unwrap();

  let copy = explorer::copy_file(&source, &source, &metadata, true).unwrap();
  assert_eq!(copy, root.path().join("pages copy"));
  assert_eq!(fs::read_to_string(copy.join("nested/page.tsx")).unwrap(), "page");

  assert!(explorer::copy_file(&source, &source.join("nested/pages"), &metadata, false).is_err());
  assert!(!source.join("nested/pages").exists());
}
