use sha2::{Digest, Sha256};
use ws_extensions::{install_from, installed_in, parse_index, parse_manifest, uninstall_in, Available};

const MANIFEST: &str = r#"{"id":"ai-agent","name":"AI Agent","version":"1.0.0","description":"Codex and Claude sessions","publisher":"gcode","contributes":{"agents":["codex","claude"]}}"#;

fn sha256(text: &str) -> String {
  Sha256::digest(text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn available(text: &str) -> Available {
  Available {
    id: "ai-agent".into(),
    name: "AI Agent".into(),
    description: String::new(),
    publisher: "gcode".into(),
    version: "1.0.0".into(),
    icon: "icons/bot-message-square.svg".into(),
    package_url: "https://raw.githubusercontent.com/o/r/main/extensions/ai-agent/manifest.json".into(),
    sha256: sha256(text),
    draft: false,
  }
}

#[test]
fn the_index_lists_extensions_with_urls_inside_the_repository() {
  let index = format!(
    r#"{{"extensions":[
      {{"id":"ai-agent","name":"AI Agent","description":"d","publisher":"gcode","version":"1.0.0","icon":"icons/bot.svg","package":"ai-agent/manifest.json","sha256":"{}"}}
    ]}}"#,
    sha256(MANIFEST)
  );
  let listed = parse_index(&index, "o/r").unwrap();
  assert_eq!(listed.len(), 1);
  assert_eq!(listed[0].package_url, "https://raw.githubusercontent.com/o/r/main/extensions/ai-agent/manifest.json");
  assert_eq!((listed[0].name.as_str(), listed[0].icon.as_str()), ("AI Agent", "icons/bot.svg"));
}

#[test]
fn an_index_entry_that_could_point_elsewhere_is_left_out() {
  let hash = sha256(MANIFEST);
  let entry = |id: &str, package: &str, sha: &str, icon: &str| format!(r#"{{"id":"{id}","package":"{package}","sha256":"{sha}","icon":"{icon}"}}"#);
  let entries = [
    entry("good", "good/manifest.json", &hash, "icons/x.svg"),
    entry("up", "../secret/manifest.json", &hash, "icons/x.svg"),
    entry("absolute", "/etc/passwd", &hash, "icons/x.svg"),
    entry("host", "https://evil.example/manifest.json", &hash, "icons/x.svg"),
    entry("Bad_Id", "bad/manifest.json", &hash, "icons/x.svg"),
    entry("short", "short/manifest.json", "abc", "icons/x.svg"),
  ];
  let listed = parse_index(&format!(r#"{{"extensions":[{}]}}"#, entries.join(",")), "o/r").unwrap();
  assert_eq!(listed.iter().map(|available| available.id.as_str()).collect::<Vec<_>>(), ["good"]);
  // An icon outside the app's own falls back to the default one.
  let escaped = parse_index(&format!(r#"{{"extensions":[{}]}}"#, entry("e", "e/manifest.json", &hash, "../../x.svg")), "o/r").unwrap();
  assert_eq!(escaped[0].icon, "icons/package.svg");
  assert!(parse_index("not json", "o/r").is_err());
  assert!(parse_index(r#"{"nothing":[]}"#, "o/r").is_err());
}

#[test]
fn a_manifest_names_the_agents_it_switches_on() {
  let manifest = parse_manifest(MANIFEST).unwrap();
  assert_eq!((manifest.id.as_str(), manifest.version.as_str()), ("ai-agent", "1.0.0"));
  assert_eq!(manifest.agents, ["codex", "claude"]);
  assert!(parse_manifest(r#"{"id":"../x"}"#).is_err());
  assert!(parse_manifest("[]").is_err());
}

#[test]
fn installing_checks_the_checksum_and_the_id_and_uninstalling_removes_it() {
  let dir = tempfile::tempdir().unwrap();
  assert!(installed_in(dir.path()).is_empty());

  let wrong_hash = Available { sha256: sha256("something else"), ..available(MANIFEST) };
  assert!(install_from(dir.path(), &wrong_hash, MANIFEST).unwrap_err().to_string().contains("checksum"));
  let other = Available { id: "other".into(), ..available(MANIFEST) };
  assert!(install_from(dir.path(), &other, MANIFEST).unwrap_err().to_string().contains("not the extension"));
  assert!(installed_in(dir.path()).is_empty());

  install_from(dir.path(), &available(MANIFEST), MANIFEST).unwrap();
  let found = installed_in(dir.path());
  assert_eq!(found.len(), 1);
  assert_eq!(found[0].manifest.agents, ["codex", "claude"]);

  uninstall_in(dir.path(), "ai-agent").unwrap();
  assert!(installed_in(dir.path()).is_empty());
  // Removing what is not there is not an error; an id that is no folder name is refused.
  uninstall_in(dir.path(), "ai-agent").unwrap();
  assert!(uninstall_in(dir.path(), "../x").is_err());
}

#[test]
fn a_folder_that_does_not_hold_its_own_manifest_is_not_an_installed_extension() {
  let dir = tempfile::tempdir().unwrap();
  std::fs::create_dir_all(dir.path().join("renamed")).unwrap();
  std::fs::write(dir.path().join("renamed/manifest.json"), MANIFEST).unwrap();
  std::fs::create_dir_all(dir.path().join("empty")).unwrap();
  assert!(installed_in(dir.path()).is_empty());
}

#[test]
fn the_published_index_matches_the_manifests_in_the_repository() {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions");
  let index = std::fs::read_to_string(root.join("index.json")).unwrap();
  let slug = ws_extensions::repository_slug().expect("the repository is configured");
  let listed = parse_index(&index, slug).unwrap();
  assert!(listed.iter().any(|available| available.id == ws_extensions::AI_AGENT));
  for available in listed.into_iter().filter(|available| !available.draft) {
    let text = std::fs::read_to_string(root.join(&available.id).join("manifest.json")).unwrap();
    // Installing into a scratch folder checks the hash and the id, as a download would.
    let dir = tempfile::tempdir().unwrap();
    install_from(dir.path(), &available, &text)
      .unwrap_or_else(|error| panic!("{}: {error} (update the hash in extensions/index.json)", available.id));
  }
}

#[test]
fn a_draft_is_listed_without_a_package_and_cannot_be_installed() {
  let index = r#"{"extensions":[
    {"id":"spotify","name":"Spotify","description":"d","status":"draft","icon":"icons/music.svg"},
    {"id":"no-package","name":"Broken"}
  ]}"#;
  let listed = parse_index(index, "o/r").unwrap();
  // The entry that is no draft and has no package is left out; the draft is kept with nothing to download.
  assert_eq!(listed.iter().map(|available| available.id.as_str()).collect::<Vec<_>>(), ["spotify"]);
  assert!(listed[0].draft && listed[0].package_url.is_empty() && listed[0].sha256.is_empty());
  let dir = tempfile::tempdir().unwrap();
  assert!(install_from(dir.path(), &listed[0], MANIFEST).unwrap_err().to_string().contains("not available yet"));
  assert!(installed_in(dir.path()).is_empty());
}
