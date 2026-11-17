use std::{path::Path, process::Command};
use vekro_terminal::{dropped_paths, paste_bytes};

#[test]
fn dropped_file_paths_survive_shell_parsing_without_executing_their_contents() {
  let paths = [Path::new("/tmp/a file.png"), Path::new("/tmp/it's $(printf injected).png"), Path::new("/tmp/görsel; *.png")];
  let input = dropped_paths(paths);
  let script = format!("set -- {input}\nprintf '%s\\0' \"$@\"");
  let output = Command::new("sh").args(["-c", &script]).output().expect("run shell");

  assert!(output.status.success());
  let actual: Vec<_> = output.stdout.split(|byte| *byte == 0).filter(|part| !part.is_empty()).collect();
  let expected: Vec<_> = paths.iter().map(|path| path.to_str().unwrap().as_bytes()).collect();
  assert_eq!(actual, expected);
}

#[test]
fn dropped_files_are_one_bracketed_paste_when_the_terminal_requests_it() {
  let paths = [Path::new("/tmp/first image.png"), Path::new("/tmp/second.png")];
  let input = dropped_paths(paths);

  assert_eq!(paste_bytes(&input, true), b"\x1b[200~/tmp/first\\ image.png /tmp/second.png \x1b[201~");
  assert_eq!(paste_bytes(&input, false), input.as_bytes());
}
