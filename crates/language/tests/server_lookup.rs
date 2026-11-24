use std::fs;
use ws_language::{file_path, file_uri, find_server, language_id};

#[test]
fn language_ids_cover_javascript_and_typescript_only() {
  assert_eq!(language_id("a/b.ts".as_ref()), Some("typescript"));
  assert_eq!(language_id("a/b.tsx".as_ref()), Some("typescriptreact"));
  assert_eq!(language_id("a/b.mjs".as_ref()), Some("javascript"));
  assert_eq!(language_id("a/b.jsx".as_ref()), Some("javascriptreact"));
  assert_eq!(language_id("a/b.rs".as_ref()), None);
  assert_eq!(language_id("a/Makefile".as_ref()), None);
}

#[test]
fn uris_round_trip_paths_with_special_characters() {
  let path = std::path::Path::new("/Users/some one/proje ct/ünï/a#b.ts");
  let uri = file_uri(path);
  assert!(uri.starts_with("file:///Users/some%20one/"));
  assert!(!uri.contains(' ') && !uri.contains('#'));
  assert_eq!(file_path(&uri).as_deref(), Some(path));
  assert_eq!(file_path("https://example.com/a.ts"), None);
}

#[test]
fn the_project_local_language_server_is_found_above_the_file() {
  let dir = tempfile::tempdir().unwrap();
  let bin = dir.path().join("node_modules/.bin");
  fs::create_dir_all(&bin).unwrap();
  fs::write(bin.join("typescript-language-server"), "").unwrap();
  let file = dir.path().join("src/deep/file.ts");
  fs::create_dir_all(file.parent().unwrap()).unwrap();
  let server = find_server(&file).expect("the local server");
  assert_eq!(server.program, bin.join("typescript-language-server"));
  assert_eq!(server.args, ["--stdio"]);
  assert_eq!(server.root, dir.path());
}

#[test]
fn files_of_other_languages_have_no_server() {
  assert_eq!(find_server("/tmp/a.rs".as_ref()), None);
}

#[test]
fn definition_answers_of_every_shape_become_locations() {
  use serde_json::json;
  use ws_language::parse_locations;
  let location = json!({ "uri": "file:///a/b%20c.ts", "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 5 } } });
  let found = parse_locations(&location);
  assert_eq!(found.len(), 1);
  assert_eq!(found[0].path, std::path::Path::new("/a/b c.ts"));
  assert_eq!((found[0].line, found[0].column, found[0].end_column), (1, 2, 5));
  assert_eq!(found[0].origin, None);

  let link = json!([{
    "originSelectionRange": { "start": { "line": 0, "character": 9 }, "end": { "line": 0, "character": 21 } },
    "targetUri": "file:///x.d.ts",
    "targetRange": { "start": { "line": 5, "character": 0 }, "end": { "line": 9, "character": 0 } },
    "targetSelectionRange": { "start": { "line": 7, "character": 14 }, "end": { "line": 7, "character": 26 } },
  }]);
  let found = parse_locations(&link);
  assert_eq!((found[0].line, found[0].column, found[0].end_column), (7, 14, 26));
  assert_eq!(found[0].origin, Some((0, 9, 0, 21)));

  assert!(parse_locations(&json!(null)).is_empty());
  assert!(parse_locations(&json!([])).is_empty());
}
