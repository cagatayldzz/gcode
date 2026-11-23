use ws_version::is_newer;

#[test]
fn a_higher_component_is_newer() {
  assert!(is_newer("0.2.0", "0.1.9"));
  assert!(is_newer("1.0.0", "0.99.99"));
  assert!(is_newer("0.1.10", "0.1.9"), "components compare as numbers, not text");
  assert!(is_newer("v1.2.3", "1.2.2"), "a leading v is ignored");
}

#[test]
fn the_same_or_an_older_version_is_not_newer() {
  assert!(!is_newer("0.1.0", "0.1.0"));
  assert!(!is_newer("0.1.0", "0.2.0"));
  assert!(!is_newer("1.2", "1.2.0"), "missing components count as zero");
}

#[test]
fn a_version_that_does_not_parse_is_never_newer() {
  assert!(!is_newer("nightly", "0.1.0"));
  assert!(!is_newer("0.2.0", "dev"));
  assert!(!is_newer("", "0.1.0"));
  assert!(!is_newer("1..2", "0.1.0"));
}

#[test]
fn a_pre_release_suffix_is_ignored() {
  assert!(!is_newer("0.2.0-beta.1", "0.2.0"));
  assert!(is_newer("0.3.0-rc.1", "0.2.0"));
}
