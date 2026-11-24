use ws_git::shell_path::extract_path;

#[test]
fn the_path_is_read_from_between_the_markers_despite_shell_noise() {
  let output = "Welcome back\n__PATH_START__/usr/bin:/Users/someone/.bun/bin__PATH_END__\n";
  assert_eq!(extract_path(output).as_deref(), Some("/usr/bin:/Users/someone/.bun/bin"));
}

#[test]
fn output_without_markers_gives_no_path() {
  assert_eq!(extract_path("nothing here"), None);
  assert_eq!(extract_path("__PATH_START____PATH_END__"), None);
}

#[cfg(unix)]
#[test]
fn the_script_prints_the_whole_path_between_the_markers() {
  let output = std::process::Command::new("/bin/sh")
    .args(["-c", &ws_git::shell_path::path_script()])
    .env("PATH", "/usr/bin:/Users/someone/.bun/bin")
    .output()
    .unwrap();

  let text = String::from_utf8_lossy(&output.stdout);

  assert_eq!(extract_path(&text).as_deref(), Some("/usr/bin:/Users/someone/.bun/bin"));
}
