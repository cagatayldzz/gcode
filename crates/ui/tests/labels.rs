#[path = "../src/labels.rs"]
mod labels;

use labels::{size_label, time_ago};
use std::time::{Duration, SystemTime};

#[test]
fn sizes_are_written_with_the_unit_that_keeps_the_number_short() {
  assert_eq!(size_label(0), "0 B");
  assert_eq!(size_label(812), "812 B");
  assert_eq!(size_label(1023), "1023 B");
  assert_eq!(size_label(1024), "1.0 KB");
  assert_eq!(size_label(3500), "3.4 KB");
  assert_eq!(size_label(5 * 1024 * 1024 + 300 * 1024), "5.3 MB");
  assert_eq!(size_label(3 * 1024 * 1024 * 1024), "3.0 GB");
  assert_eq!(size_label(5 * 1024u64.pow(4)), "5.0 TB");
  assert_eq!(size_label(u64::MAX), "16777216.0 TB", "there is no unit beyond TB");
}

#[test]
fn ages_are_written_in_the_largest_whole_unit() {
  let ago = |seconds: u64| time_ago(SystemTime::now() - Duration::from_secs(seconds));
  assert_eq!(ago(5), "just now");
  assert_eq!(ago(120), "2m ago");
  assert_eq!(ago(3 * 3600 + 5), "3h ago");
  assert_eq!(ago(2 * 86400 + 5), "2d ago");
  assert_eq!(time_ago(SystemTime::now() + Duration::from_secs(600)), "just now", "a time in the future is not negative");
}
