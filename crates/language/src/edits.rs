use lsp_types::{Position, TextEdit};

/// Applies a server's edits to a single unchanged document. LSP coordinates are UTF-16.
pub fn apply_text_edits(source: &str, edits: &[TextEdit]) -> Result<String, String> {
  let mut starts = vec![0];
  starts.extend(source.match_indices('\n').map(|(index, _)| index + 1));
  let offset = |position: Position| -> Option<usize> {
    let row = position.line as usize;
    let start = *starts.get(row)?;
    let mut line = &source[start..starts.get(row + 1).copied().unwrap_or(source.len())];
    if let Some(before) = line.strip_suffix('\n') {
      line = before.strip_suffix('\r').unwrap_or(before);
    }
    let mut column = 0;
    for (index, character) in line.char_indices() {
      if column == position.character {
        return Some(start + index);
      }
      column += character.len_utf16() as u32;
    }
    (column == position.character).then_some(start + line.len())
  };
  let mut ranges = Vec::with_capacity(edits.len());
  for edit in edits {
    let start = offset(edit.range.start).ok_or("The language server returned an invalid edit start.")?;
    let end = offset(edit.range.end).ok_or("The language server returned an invalid edit end.")?;
    if end < start {
      return Err("The language server returned a reversed edit.".into());
    }
    ranges.push((start, end, edit.new_text.as_str()));
  }
  ranges.sort_by_key(|(start, end, _)| (*start, *end));
  let mut output = String::with_capacity(source.len());
  let mut cursor = 0;
  for (start, end, replacement) in ranges {
    if start < cursor {
      return Err("The language server returned overlapping edits.".into());
    }
    output.push_str(&source[cursor..start]);
    output.push_str(replacement);
    cursor = end;
  }
  output.push_str(&source[cursor..]);
  Ok(output)
}
