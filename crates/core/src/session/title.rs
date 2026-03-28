use crate::session::types::{ConversationMessage, Role};

const DEFAULT_SESSION_TITLE: &str = "New Session";
const TITLE_RENAME_INTERVAL: usize = 5;
const TITLE_SOURCE_MESSAGE_LIMIT: usize = 20;
const TITLE_MAX_CHARS: usize = 60;

pub fn maybe_generate_session_title(
    current_title: &str,
    messages: &[ConversationMessage],
) -> Option<String> {
    let user_messages: Vec<&ConversationMessage> = messages
        .iter()
        .filter(|message| message.role == Role::User)
        .collect();

    if user_messages.is_empty() {
        return None;
    }

    let should_rename = current_title == DEFAULT_SESSION_TITLE
        || user_messages.len() % TITLE_RENAME_INTERVAL == 0;
    if !should_rename {
        return None;
    }

    let recent_messages = messages
        .iter()
        .rev()
        .take(TITLE_SOURCE_MESSAGE_LIMIT)
        .collect::<Vec<_>>();

    let mut chunks = Vec::new();
    for message in recent_messages.into_iter().rev() {
        let text = message.full_text();
        let normalized = normalize_whitespace(&text);
        if normalized.is_empty() {
            continue;
        }

        chunks.push(format!("{}: {normalized}", message.role));
    }

    if chunks.is_empty() {
        return None;
    }

    let combined = chunks.join(" | ");
    let title = truncate_chars(&combined, TITLE_MAX_CHARS);

    if title.is_empty() || title == current_title {
        None
    } else {
        Some(title)
    }
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}
