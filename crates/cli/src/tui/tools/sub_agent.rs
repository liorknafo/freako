use ratatui::prelude::*;

use freako_core::agent::events::{AgentEvent, ToolOutputStream};
use freako_core::tools::sub_agent::{SubAgentLogEntry, SubAgentResult};
use freako_core::tools::tool_name::ToolCall;

use super::{ToolResultInfo, ToolView};
use super::super::message::markdown_to_lines;
use crate::tui::{TOOL_STYLE, ERROR_STYLE, MUTED_STYLE, SUCCESS_STYLE};

const SUB_AGENT_VISIBLE_LINES: usize = 10;

/// View for sub_agent tool calls. Shows a scrollable timeline of the sub-agent's work.
pub struct SubAgentView {
    info: ToolResultInfo,
    /// Timeline of events in display order
    timeline: Vec<TimelineEntry>,
    /// Text being accumulated before the next tool call or completion
    pending_text: String,
    running: bool,
    /// Scroll offset from the bottom (0 = tailing latest)
    scroll_offset: usize,
}

enum TimelineEntry {
    /// Sub-agent text output
    Text(String),
    /// A tool call with optional result
    ToolCall {
        name: String,
        summary: String,
        result_preview: Option<String>,
        is_error: bool,
        done: bool,
    },
}

impl SubAgentView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self {
            info,
            timeline: Vec::new(),
            pending_text: String::new(),
            running: true,
            scroll_offset: 0,
        }
    }

    /// Process a nested AgentEvent from the sub-agent relay.
    pub fn push_nested_event(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::StreamDelta(text) => {
                self.pending_text.push_str(text);
            }
            AgentEvent::ToolCallRequested { name, arguments, .. } => {
                self.flush_pending_text();
                let summary = ToolCall::from_raw(name, arguments)
                    .map(|tc| freako_core::tools::tool_name::ToolPresentation::summary(&tc))
                    .unwrap_or_default();
                self.timeline.push(TimelineEntry::ToolCall {
                    name: name.clone(),
                    summary,
                    result_preview: None,
                    is_error: false,
                    done: false,
                });
                // Auto-scroll to bottom on new activity
                self.scroll_offset = 0;
            }
            AgentEvent::ToolResult { name, content, is_error, .. } => {
                for entry in self.timeline.iter_mut().rev() {
                    if let TimelineEntry::ToolCall {
                        name: n,
                        done: d,
                        result_preview: rp,
                        is_error: ie,
                        ..
                    } = entry
                    {
                        if *n == *name && !*d {
                            let preview = content.lines().take(2).collect::<Vec<_>>().join(" | ");
                            *rp = Some(truncate(&preview, 100));
                            *ie = *is_error;
                            *d = true;
                            break;
                        }
                    }
                }
                self.scroll_offset = 0;
            }
            AgentEvent::Done | AgentEvent::Error(_) | AgentEvent::Cancelled => {
                self.flush_pending_text();
                self.running = false;
            }
            _ => {}
        }
    }

    fn flush_pending_text(&mut self) {
        let text = self.pending_text.trim().to_string();
        if !text.is_empty() {
            self.timeline.push(TimelineEntry::Text(text));
        }
        self.pending_text.clear();
    }

    /// Build all content lines for the timeline.
    fn collect_lines(&self) -> Vec<Line<'static>> {
        let md_width: u16 = 100;
        let mut lines = Vec::new();

        for entry in &self.timeline {
            match entry {
                TimelineEntry::Text(text) => {
                    lines.extend(markdown_to_lines(text, md_width));
                }
                TimelineEntry::ToolCall { name, summary, result_preview, is_error, done } => {
                    let (status_text, status_style) = if !done {
                        ("running…", TOOL_STYLE)
                    } else if *is_error {
                        ("✗ error", ERROR_STYLE)
                    } else {
                        ("✓ done", SUCCESS_STYLE)
                    };

                    lines.push(Line::from(vec![
                        Span::styled("⚙ ", MUTED_STYLE),
                        Span::styled(name.clone(), TOOL_STYLE.add_modifier(Modifier::BOLD)),
                        Span::styled(format!(" – {}", summary), TOOL_STYLE),
                        Span::styled(format!(" – {}", status_text), status_style),
                    ]));

                    if *done {
                        if let Some(preview) = result_preview {
                            if !preview.is_empty() {
                                lines.push(Line::styled(
                                    format!("  ↳ {}", preview),
                                    MUTED_STYLE,
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Show currently accumulating text
        let trimmed = self.pending_text.trim();
        if !trimmed.is_empty() {
            lines.extend(markdown_to_lines(trimmed, md_width));
        }

        if lines.is_empty() {
            lines.push(Line::styled(
                if self.running { "thinking…" } else { "<empty>" },
                MUTED_STYLE,
            ));
        }

        lines
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let half = max.saturating_sub(3) / 2;
    let start: String = s.chars().take(half).collect();
    let end: String = s.chars().rev().take(half).collect::<String>().chars().rev().collect();
    format!("{}...{}", start, end)
}

impl ToolView for SubAgentView {
    fn info(&self) -> &ToolResultInfo { &self.info }

    fn default_collapsed(&self) -> bool { false }

    fn is_running(&self) -> bool { self.running }

    fn is_scrollable(&self) -> bool {
        // Only scrollable when there's more content than the visible window
        self.collect_lines().len() > SUB_AGENT_VISIBLE_LINES
    }

    fn scroll(&mut self, delta: isize) {
        let total = self.collect_lines().len();
        let max_offset = total.saturating_sub(SUB_AGENT_VISIBLE_LINES);
        let new = self.scroll_offset as isize + delta;
        self.scroll_offset = (new.max(0) as usize).min(max_offset);
    }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        let all_lines = self.collect_lines();
        let total = all_lines.len();
        let from_bottom = self.scroll_offset.min(total.saturating_sub(1));
        let window_size = SUB_AGENT_VISIBLE_LINES.min(total.max(1));
        let end = total.saturating_sub(from_bottom);
        let start = end.saturating_sub(window_size);

        let border_style = MUTED_STYLE;
        let header_style = TOOL_STYLE.add_modifier(Modifier::BOLD);
        let status_style = if self.running { TOOL_STYLE } else { MUTED_STYLE };

        let task = truncate(&self.info.summary(), 60);
        let status = if self.running { "running" } else { "done" };

        let mut rendered = Vec::new();

        // Header: ┌─ task [status]
        rendered.push(Line::from(vec![
            Span::styled("      \u{250c}\u{2500} ", border_style),
            Span::styled(task, header_style),
            Span::styled(format!(" [{}]", status), status_style),
        ]));

        // Visible lines
        for line in all_lines.into_iter().skip(start).take(end.saturating_sub(start)) {
            let mut spans = vec![Span::styled("      \u{2502} ", border_style)];
            spans.extend(line.spans);
            rendered.push(Line::from(spans));
        }

        // Footer: └─ hint
        let hint = if total <= window_size {
            format!("{} line{}", total, if total == 1 { "" } else { "s" })
        } else if from_bottom == 0 {
            format!("latest {} / {} lines", window_size, total)
        } else {
            format!("scroll +{} from bottom", from_bottom)
        };
        rendered.push(Line::from(vec![
            Span::styled("      \u{2514}\u{2500} ", border_style),
            Span::styled(hint, MUTED_STYLE),
        ]));

        rendered
    }

    fn push_stream_delta(&mut self, _stream: ToolOutputStream, text: &str) {
        self.pending_text.push_str(text);
    }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.flush_pending_text();
        self.info.content = content.to_string();
        self.info.is_error = is_error;
        self.running = false;

        // If timeline is empty (e.g. session reload), reconstruct from serialized log
        if self.timeline.is_empty() {
            if let Ok(result) = serde_json::from_str::<SubAgentResult>(content) {
                for entry in result.log {
                    match entry {
                        SubAgentLogEntry::Text { text } => {
                            self.timeline.push(TimelineEntry::Text(text));
                        }
                        SubAgentLogEntry::ToolCall { name, summary } => {
                            self.timeline.push(TimelineEntry::ToolCall {
                                name,
                                summary,
                                result_preview: None,
                                is_error: false,
                                done: false,
                            });
                        }
                        SubAgentLogEntry::ToolResult { name, preview, is_error } => {
                            // Find the last matching incomplete tool call
                            for te in self.timeline.iter_mut().rev() {
                                if let TimelineEntry::ToolCall {
                                    name: n,
                                    done: d,
                                    result_preview: rp,
                                    is_error: ie,
                                    ..
                                } = te
                                {
                                    if *n == name && !*d {
                                        *rp = Some(preview.clone());
                                        *ie = is_error;
                                        *d = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn push_nested_event(&mut self, event: &AgentEvent) {
        SubAgentView::push_nested_event(self, event);
    }
}
