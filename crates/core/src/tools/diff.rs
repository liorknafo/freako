use similar::{Algorithm, ChangeTag, TextDiff};

#[derive(Debug, Clone)]
pub enum DiffLineKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub enum InlineChangeKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct InlineChange {
    pub kind: InlineChangeKind,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DiffRow {
    pub old_line: Option<DiffLine>,
    pub new_line: Option<DiffLine>,
    pub old_changes: Vec<InlineChange>,
    pub new_changes: Vec<InlineChange>,
    /// Merged inline changes for modified lines (has both old and new).
    /// Contains Unchanged, Removed, and Added segments interleaved in order.
    pub merged_changes: Vec<InlineChange>,
}

const CONTEXT_LINES: usize = 3;

pub fn render_diff(path: &str, old: &str, new: &str) -> String {
    let rows = diff_rows(old, new);

    // Mark which rows are changed (not Unchanged)
    let is_changed: Vec<bool> = rows.iter().map(|r| {
        let dominated_by_unchanged = match (&r.old_line, &r.new_line) {
            (Some(ol), None) => matches!(ol.kind, DiffLineKind::Unchanged),
            (None, Some(nl)) => matches!(nl.kind, DiffLineKind::Unchanged),
            (Some(ol), Some(_)) => matches!(ol.kind, DiffLineKind::Unchanged),
            (None, None) => true,
        };
        !dominated_by_unchanged
    }).collect();

    // For each row, determine if it's within CONTEXT_LINES of a changed row
    let mut visible = vec![false; rows.len()];
    for (i, changed) in is_changed.iter().enumerate() {
        if *changed {
            let start = i.saturating_sub(CONTEXT_LINES);
            let end = (i + CONTEXT_LINES + 1).min(rows.len());
            for v in &mut visible[start..end] {
                *v = true;
            }
        }
    }

    let mut out = String::new();
    out.push_str(&format!("Edited {}\n\n", path));

    let mut in_gap = false;
    for (i, row) in rows.iter().enumerate() {
        if !visible[i] {
            if !in_gap {
                out.push_str("@@\n");
                in_gap = true;
            }
            continue;
        }
        in_gap = false;

        // Modified line (has both old and new) — emit single merged line with ~ prefix
        if row.old_line.is_some() && row.new_line.is_some() && !row.merged_changes.is_empty() {
            out.push_str("~ ");
            out.push_str(&encode_inline_changes(&row.merged_changes));
            out.push('\n');
            continue;
        }

        if let Some(old_line) = &row.old_line {
            out.push_str(match old_line.kind {
                DiffLineKind::Unchanged => "  ",
                DiffLineKind::Removed => "- ",
                DiffLineKind::Added => "+ ",
            });
            out.push_str(&encode_inline_changes(&row.old_changes));
            out.push('\n');
        }
        if let Some(new_line) = &row.new_line {
            out.push_str(match new_line.kind {
                DiffLineKind::Unchanged => "  ",
                DiffLineKind::Removed => "- ",
                DiffLineKind::Added => "+ ",
            });
            out.push_str(&encode_inline_changes(&row.new_changes));
            out.push('\n');
        }
    }

    out
}

pub fn diff_rows(old: &str, new: &str) -> Vec<DiffRow> {
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .diff_lines(old, new);

    let mut rows = Vec::new();
    let mut removed_buffer: Vec<String> = Vec::new();
    let mut added_buffer: Vec<String> = Vec::new();

    let flush_buffers = |rows: &mut Vec<DiffRow>, removed_buffer: &mut Vec<String>, added_buffer: &mut Vec<String>| {
        if removed_buffer.is_empty() && added_buffer.is_empty() {
            return;
        }

        if removed_buffer.len() == added_buffer.len() {
            for (old_line, new_line) in removed_buffer.drain(..).zip(added_buffer.drain(..)) {
                if old_line == new_line {
                    rows.push(DiffRow {
                        old_line: Some(DiffLine { kind: DiffLineKind::Unchanged, text: old_line.clone() }),
                        new_line: None,
                        old_changes: vec![InlineChange { kind: InlineChangeKind::Unchanged, text: trim_newline(&old_line).to_string() }],
                        new_changes: Vec::new(),
                        merged_changes: Vec::new(),
                    });
                } else {
                    let (old_changes, new_changes, merged) = inline_changes(&old_line, &new_line);
                    rows.push(DiffRow {
                        old_line: Some(DiffLine { kind: DiffLineKind::Removed, text: old_line }),
                        new_line: Some(DiffLine { kind: DiffLineKind::Added, text: new_line }),
                        old_changes,
                        new_changes,
                        merged_changes: merged,
                    });
                }
            }
        } else {
            for old_line in removed_buffer.drain(..) {
                rows.push(DiffRow {
                    old_line: Some(DiffLine { kind: DiffLineKind::Removed, text: old_line.clone() }),
                    new_line: None,
                    old_changes: vec![InlineChange { kind: InlineChangeKind::Removed, text: trim_newline(&old_line).to_string() }],
                    new_changes: Vec::new(),
                    merged_changes: Vec::new(),
                });
            }
            for new_line in added_buffer.drain(..) {
                rows.push(DiffRow {
                    old_line: None,
                    new_line: Some(DiffLine { kind: DiffLineKind::Added, text: new_line.clone() }),
                    old_changes: Vec::new(),
                    new_changes: vec![InlineChange { kind: InlineChangeKind::Added, text: trim_newline(&new_line).to_string() }],
                    merged_changes: Vec::new(),
                });
            }
        }
    };

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                flush_buffers(&mut rows, &mut removed_buffer, &mut added_buffer);
                let text = change.to_string();
                rows.push(DiffRow {
                    old_line: Some(DiffLine { kind: DiffLineKind::Unchanged, text: text.clone() }),
                    new_line: None,
                    old_changes: vec![InlineChange { kind: InlineChangeKind::Unchanged, text: trim_newline(&text).to_string() }],
                    new_changes: Vec::new(),
                    merged_changes: Vec::new(),
                });
            }
            ChangeTag::Delete => removed_buffer.push(change.to_string()),
            ChangeTag::Insert => added_buffer.push(change.to_string()),
        }
    }

    flush_buffers(&mut rows, &mut removed_buffer, &mut added_buffer);
    rows
}

fn inline_changes(old: &str, new: &str) -> (Vec<InlineChange>, Vec<InlineChange>, Vec<InlineChange>) {
    let old_trimmed = trim_newline(old);
    let new_trimmed = trim_newline(new);
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .diff_chars(old_trimmed, new_trimmed);

    let mut old_changes = Vec::new();
    let mut new_changes = Vec::new();
    let mut merged = Vec::new();

    for change in diff.iter_all_changes() {
        let text = change.to_string();
        if text.is_empty() {
            continue;
        }

        match change.tag() {
            ChangeTag::Equal => {
                old_changes.push(InlineChange { kind: InlineChangeKind::Unchanged, text: text.clone() });
                new_changes.push(InlineChange { kind: InlineChangeKind::Unchanged, text: text.clone() });
                merged.push(InlineChange { kind: InlineChangeKind::Unchanged, text });
            }
            ChangeTag::Delete => {
                old_changes.push(InlineChange { kind: InlineChangeKind::Removed, text: text.clone() });
                merged.push(InlineChange { kind: InlineChangeKind::Removed, text });
            }
            ChangeTag::Insert => {
                new_changes.push(InlineChange { kind: InlineChangeKind::Added, text: text.clone() });
                merged.push(InlineChange { kind: InlineChangeKind::Added, text });
            }
        }
    }

    if old_changes.is_empty() {
        old_changes.push(InlineChange { kind: InlineChangeKind::Unchanged, text: String::new() });
    }
    if new_changes.is_empty() {
        new_changes.push(InlineChange { kind: InlineChangeKind::Unchanged, text: String::new() });
    }
    if merged.is_empty() {
        merged.push(InlineChange { kind: InlineChangeKind::Unchanged, text: String::new() });
    }

    (old_changes, new_changes, merged)
}

fn encode_inline_changes(changes: &[InlineChange]) -> String {
    let mut out = String::new();
    for change in changes {
        let marker = match change.kind {
            InlineChangeKind::Unchanged => '=',
            InlineChangeKind::Added => '+',
            InlineChangeKind::Removed => '-',
        };
        out.push('⟦');
        out.push(marker);
        out.push(':');
        // Strip newlines so markers never span multiple lines in the encoded output
        let clean = change.text
            .replace('\n', "")
            .replace('\r', "")
            .replace('⟦', "\\u{27e6}")
            .replace('⟧', "\\u{27e7}");
        out.push_str(&clean);
        out.push('⟧');
    }
    out
}

fn trim_newline(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))
        .unwrap_or(text)
}
