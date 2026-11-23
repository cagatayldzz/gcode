use std::path::Path;
use ws_updater::{bundle_of, checksum_for, parse_release, repository_slug, team_identifier};

const SLUG: &str = "owner/project";

/// The name of the archive a release of `tag` publishes.
fn archive(tag: &str) -> String {
  format!("{}-{tag}-macos-arm64.zip", ws_product::NAME)
}

fn release_json(tag: &str, archive_url: &str) -> String {
  let name = archive(tag);
  format!(
    r#"{{"tag_name":"{tag}","body":"Notes","html_url":"https://github.com/{SLUG}/releases/tag/{tag}","assets":[
      {{"name":"{name}","browser_download_url":"{archive_url}"}},
      {{"name":"SHA256SUMS.txt","browser_download_url":"https://github.com/{SLUG}/releases/download/{tag}/SHA256SUMS.txt"}}]}}"#
  )
}

#[test]
fn parses_a_release_with_its_assets() {
  let url = format!("https://github.com/{SLUG}/releases/download/v1.2.3/{}", archive("v1.2.3"));
  let release = parse_release(&release_json("v1.2.3", &url), SLUG).unwrap();
  assert_eq!(release.version, "1.2.3");
  assert_eq!(release.tag, "v1.2.3");
  assert_eq!(release.archive_name, archive("v1.2.3"));
  assert_eq!(release.archive_url, url);
  assert!(release.checksums_url.ends_with("/SHA256SUMS.txt"));
  assert_eq!(release.notes, "Notes");
  assert_eq!(release.page_url, format!("https://github.com/{SLUG}/releases/tag/v1.2.3"));
}

#[test]
fn rejects_assets_hosted_elsewhere_or_for_another_tag() {
  let other_host = format!("https://evil.example/{}", archive("v1.2.3"));
  assert!(parse_release(&release_json("v1.2.3", &other_host), SLUG).is_err());
  let other_tag = format!("https://github.com/{SLUG}/releases/download/v9.9.9/{}", archive("v1.2.3"));
  assert!(parse_release(&release_json("v1.2.3", &other_tag), SLUG).is_err());
  let other_repository = format!("https://github.com/someone/else/releases/download/v1.2.3/{}", archive("v1.2.3"));
  assert!(parse_release(&release_json("v1.2.3", &other_repository), SLUG).is_err());
}

#[test]
fn rejects_malformed_tags_and_missing_assets() {
  let url = format!("https://github.com/{SLUG}/releases/download/nightly/{}", archive("nightly"));
  assert!(parse_release(&release_json("nightly", &url), SLUG).is_err());
  assert!(parse_release(r#"{"tag_name":"v1.0.0","assets":[]}"#, SLUG).is_err());
  assert!(parse_release("not json", SLUG).is_err());
  let pre_release = format!("https://github.com/{SLUG}/releases/download/v1.0.0-beta/{}", archive("v1.0.0-beta"));
  assert!(parse_release(&release_json("v1.0.0-beta", &pre_release), SLUG).is_err(), "only plain x.y.z tags are releases");
}

#[test]
fn a_page_link_outside_the_repository_is_dropped() {
  let url = format!("https://github.com/{SLUG}/releases/download/v1.0.0/{}", archive("v1.0.0"));
  let json = release_json("v1.0.0", &url).replace(&format!("https://github.com/{SLUG}/releases/tag/v1.0.0"), "https://evil.example/");
  assert_eq!(parse_release(&json, SLUG).unwrap().page_url, "");
}

#[test]
fn finds_the_checksum_for_an_archive() {
  let hash = "a".repeat(64);
  let name = archive("v1.0.0");
  let listing = format!("{hash}  {name}\n{}  other.zip\n", "b".repeat(64));
  assert_eq!(checksum_for(&listing, &name), Some(hash));
  assert_eq!(checksum_for(&listing, "missing.zip"), None);
  assert_eq!(checksum_for("short  app.zip", "app.zip"), None);
}

#[test]
fn reads_the_team_identifier_and_ignores_ad_hoc_signatures() {
  assert_eq!(team_identifier("Identifier=x\nTeamIdentifier=ABCDE12345\n"), Some("ABCDE12345".into()));
  assert_eq!(team_identifier("TeamIdentifier=not set\n"), None);
  assert_eq!(team_identifier("Identifier=x\n"), None);
}

#[test]
fn locates_the_app_bundle_from_the_executable_path() {
  let name = ws_product::NAME;
  assert_eq!(bundle_of(Path::new(&format!("/Applications/{name}.app/Contents/MacOS/{name}"))), Some(format!("/Applications/{name}.app").into()));
  assert_eq!(bundle_of(Path::new(&format!("/repo/target/debug/{name}"))), None);
}

#[test]
fn the_repository_slug_comes_from_the_workspace() {
  // The slug is the part of the manifest's `repository` URL after the host.
  let expected = env!("CARGO_PKG_REPOSITORY").strip_prefix("https://github.com/").map(|slug| slug.trim_end_matches('/').to_string());
  assert_eq!(repository_slug().map(str::to_string), expected);
}

#[test]
fn release_notes_lose_markdown_markers() {
  let url = format!("https://github.com/{SLUG}/releases/download/v1.0.0/{}", archive("v1.0.0"));
  let mut release = parse_release(&release_json("v1.0.0", &url), SLUG).unwrap();
  release.notes = "## New Features\n\n- Added updates ([b096ff5](https://github.com/o/r/commit/b096ff5))\n- See [docs](https://x.y) now".into();
  assert_eq!(release.plain_notes(), "New Features\n\n• Added updates (b096ff5)\n• See docs now");
}
