//! Selection state for tracking text selection across markdown items.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::time::Instant;



/// Actions emitted by the selectable markdown widget.
#[derive(Debug, Clone)]
pub enum SelectionAction {
    /// Mouse pressed at a position within a specific item.
    Press { item_index: usize, char_offset: usize },
    /// Mouse dragged to a position within a specific item.
    Drag { item_index: usize, char_offset: usize },
    /// Mouse released.
    Release,
    /// Copy selection to clipboard.
    Copy,
}

/// A position in the document: which item and what character offset within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DocPosition {
    item_index: usize,
    char_offset: usize,
}

impl DocPosition {
    /// Ordering: first by item_index, then by char_offset.
    fn cmp_pos(&self, other: &Self) -> std::cmp::Ordering {
        self.item_index.cmp(&other.item_index)
            .then(self.char_offset.cmp(&other.char_offset))
    }
}

/// Persistent selection state held by the application.
#[derive(Debug, Clone)]
pub struct SelectionState {
    /// Selection anchor (where mouse-down happened).
    anchor: Option<DocPosition>,
    /// Selection head (current drag position).
    head: Option<DocPosition>,
    /// Whether the mouse is currently dragging.
    dragging: bool,
    /// Last click time for double/triple click detection.
    last_click_time: Option<Instant>,
    click_count: u8,
    /// Plain text registered by each widget during rendering, keyed by item_index.
    /// Used for clipboard copy and word boundary detection.
    item_texts: RefCell<HashMap<usize, String>>,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            anchor: None,
            head: None,
            dragging: false,
            last_click_time: None,
            click_count: 0,
            item_texts: RefCell::new(HashMap::new()),
        }
    }
}

impl SelectionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the plain text for an item (called by widgets during layout/draw).
    pub fn register_item_text(&self, item_index: usize, text: String) {
        self.item_texts.borrow_mut().insert(item_index, text);
    }

    /// Process a selection action.
    pub fn perform(&mut self, action: SelectionAction) {
        match action {
            SelectionAction::Press { item_index, char_offset } => {
                let pos = DocPosition { item_index, char_offset };

                // Double/triple-click detection
                let now = Instant::now();
                if let Some(last_time) = self.last_click_time {
                    if now.duration_since(last_time).as_millis() < 400 {
                        self.click_count = (self.click_count + 1).min(3);
                    } else {
                        self.click_count = 1;
                    }
                } else {
                    self.click_count = 1;
                }
                self.last_click_time = Some(now);

                match self.click_count {
                    2 => {
                        // Double-click: select word
                        let (start, end) = self.word_boundaries(item_index, char_offset);
                        self.anchor = Some(DocPosition { item_index, char_offset: start });
                        self.head = Some(DocPosition { item_index, char_offset: end });
                    }
                    3 => {
                        // Triple-click: select entire item text
                        let len = self.item_texts.borrow()
                            .get(&item_index)
                            .map(|t| t.len())
                            .unwrap_or(0);
                        self.anchor = Some(DocPosition { item_index, char_offset: 0 });
                        self.head = Some(DocPosition { item_index, char_offset: len });
                    }
                    _ => {
                        // Single click: set cursor
                        self.anchor = Some(pos);
                        self.head = Some(pos);
                    }
                }
                self.dragging = true;
            }
            SelectionAction::Drag { item_index, char_offset } => {
                if self.dragging {
                    self.head = Some(DocPosition { item_index, char_offset });
                }
            }
            SelectionAction::Release => {
                self.dragging = false;
            }
            SelectionAction::Copy => {
                // Handled by the caller via selected_text()
            }
        }
    }

    /// Get the char range selected within a specific item, if any.
    pub fn item_selection(&self, item_index: usize) -> Option<Range<usize>> {
        let (anchor, head) = match (self.anchor, self.head) {
            (Some(a), Some(h)) => (a, h),
            _ => return None,
        };

        // Order anchor and head
        let (start, end) = if anchor.cmp_pos(&head) == std::cmp::Ordering::Less
            || anchor.cmp_pos(&head) == std::cmp::Ordering::Equal
        {
            (anchor, head)
        } else {
            (head, anchor)
        };

        // Same position = no selection
        if start == end {
            return None;
        }

        if start.item_index == end.item_index && start.item_index == item_index {
            // Selection within a single item
            Some(start.char_offset..end.char_offset)
        } else if item_index > start.item_index && item_index < end.item_index {
            // Item is fully within the selection range — select all of it
            let len = self.item_texts.borrow()
                .get(&item_index)
                .map(|t| t.len())
                .unwrap_or(1000);
            Some(0..len)
        } else if item_index == start.item_index && item_index < end.item_index {
            // Start item: from anchor to end
            let len = self.item_texts.borrow()
                .get(&item_index)
                .map(|t| t.len())
                .unwrap_or(1000);
            Some(start.char_offset..len)
        } else if item_index == end.item_index && item_index > start.item_index {
            // End item: from beginning to head
            Some(0..end.char_offset)
        } else {
            None
        }
    }

    /// Whether there is any active selection.
    pub fn has_selection(&self) -> bool {
        match (self.anchor, self.head) {
            (Some(a), Some(h)) => a != h,
            _ => false,
        }
    }

    /// Get the selected plain text for clipboard.
    pub fn selected_text(&self) -> String {
        let (anchor, head) = match (self.anchor, self.head) {
            (Some(a), Some(h)) if a != h => (a, h),
            _ => return String::new(),
        };

        let (start, end) = if anchor.cmp_pos(&head) == std::cmp::Ordering::Less {
            (anchor, head)
        } else {
            (head, anchor)
        };

        let texts = self.item_texts.borrow();
        let mut result = String::new();

        // Collect text from all items in the selection range
        for idx in start.item_index..=end.item_index {
            if let Some(item_text) = texts.get(&idx) {
                let from = if idx == start.item_index { start.char_offset } else { 0 };
                let to = if idx == end.item_index {
                    end.char_offset.min(item_text.len())
                } else {
                    item_text.len()
                };
                if from < to && from < item_text.len() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&item_text[from..to.min(item_text.len())]);
                }
            }
        }

        result
    }

    /// Whether currently dragging.
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    fn word_boundaries(&self, item_index: usize, offset: usize) -> (usize, usize) {
        let texts = self.item_texts.borrow();
        let text = match texts.get(&item_index) {
            Some(t) => t.as_str(),
            None => return (offset, offset + 1),
        };
        let bytes = text.as_bytes();
        let len = bytes.len();
        let offset = offset.min(len);

        let is_word_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

        let mut start = offset;
        while start > 0 && start <= len && is_word_char(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = offset;
        while end < len && is_word_char(bytes[end]) {
            end += 1;
        }

        if start == end && end < len {
            end += 1;
        }

        (start, end)
    }
}
