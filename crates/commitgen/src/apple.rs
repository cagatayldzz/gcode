use crate::prompt::split_diff_with_budget;
use serde_json::json;
use std::{
  io::Write as _,
  path::Path,
  process::{Command, Stdio},
};

// Bytes give code-heavy and multilingual diffs more headroom than the chat API's token estimate.
const PROMPT_BUDGET: usize = 3_000;
const MERGE_BATCH_SIZE: usize = 3;

fn complete(helper: &Path, instructions: &str, prompt: &str) -> Result<String, String> {
  let input = json!({ "instructions": instructions, "prompt": prompt }).to_string();
  let mut child = Command::new(helper)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| format!("Could not start Apple Intelligence. Launch {} from its macOS app bundle. {error}", ws_product::NAME))?;
  let sent = child.stdin.take().expect("child input is piped").write_all(input.as_bytes());
  let output = child.wait_with_output().map_err(|error| format!("Could not read the Apple Intelligence response: {error}"))?;
  if !output.status.success() {
    let detail = String::from_utf8_lossy(&output.stderr);
    return Err(if detail.trim().is_empty() {
      "Apple Intelligence could not generate a commit message. Try again or choose another provider.".into()
    } else {
      detail.trim().into()
    });
  }
  sent.map_err(|error| format!("Could not send the diff to Apple Intelligence: {error}"))?;
  let answer = String::from_utf8(output.stdout).map_err(|_| "Apple Intelligence returned invalid text.".to_string())?;
  if answer.trim().is_empty() {
    return Err("Apple Intelligence sent no commit message.".into());
  }
  Ok(answer)
}

pub(crate) fn generate(instructions: &str, diff: &str) -> Result<String, String> {
  let helper = std::env::current_exe()
    .map_err(|error| format!("Could not locate the Apple Intelligence helper: {error}"))?
    .with_file_name(format!("{}-apple-intelligence", ws_product::slug()));
  let budget = PROMPT_BUDGET
    .checked_sub(instructions.len())
    .filter(|budget| *budget >= 512)
    .ok_or_else(|| "Shorten your commit instructions to leave room for the diff in Apple Intelligence.".to_string())?;
  let mut summaries =
    split_diff_with_budget(diff, budget, str::len).into_iter().map(|part| complete(&helper, instructions, &part)).collect::<Result<Vec<_>, _>>()?;
  while summaries.len() > 1 {
    summaries = summaries
      .chunks(MERGE_BATCH_SIZE)
      .map(|batch| match batch {
        [only] => Ok(only.clone()),
        _ => complete(
          &helper,
          instructions,
          &format!("Merge these draft messages for parts of the same change into one commit message:\n\n{}", batch.join("\n\n")),
        ),
      })
      .collect::<Result<Vec<_>, _>>()?;
  }
  summaries.pop().ok_or_else(|| "There are no changes to describe.".into())
}
