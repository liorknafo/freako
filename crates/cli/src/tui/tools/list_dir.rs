use ratatui::prelude::*;

use super::{ToolResultInfo, ToolView};
use crate::tui::{MUTED_STYLE, ERROR_STYLE};

const DIR_COLOR: Color = Color::Indexed(75);
const FILE_COLOR: Color = Color::Indexed(252);
const SIZE_COLOR: Color = Color::Indexed(243);

pub struct ListDirView {
    info: ToolResultInfo,
}

impl ListDirView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self { info }
    }
}

impl ToolView for ListDirView {
    fn info(&self) -> &ToolResultInfo { &self.info }

    fn default_collapsed(&self) -> bool { true }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.info.content = content.to_string();
        self.info.is_error = is_error;
    }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        if self.info.content.is_empty() {
            return vec![Line::styled("      <empty>", MUTED_STYLE)];
        }

        if self.info.is_error {
            return self.info.content
                .lines()
                .map(|l| Line::styled(format!("      {}", l), ERROR_STYLE))
                .collect();
        }

        let mut lines = Vec::new();
        let mut section = Section::None;

        for raw_line in self.info.content.lines() {
            let trimmed = raw_line.trim();

            // "Directory: path" header
            if let Some(path) = trimmed.strip_prefix("Directory: ") {
                lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(
                        format!("Directory: {}", path),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]));
                continue;
            }

            // Section headers
            if trimmed.starts_with("Directories (") || trimmed.starts_with("Directories:") {
                section = Section::Directories;
                lines.push(Line::styled(format!("      {}", trimmed), MUTED_STYLE));
                continue;
            }
            if trimmed.starts_with("Files (") || trimmed.starts_with("Files:") {
                section = Section::Files;
                lines.push(Line::styled(format!("      {}", trimmed), MUTED_STYLE));
                continue;
            }

            if trimmed == "(empty)" {
                lines.push(Line::styled("      (empty)", MUTED_STYLE));
                continue;
            }

            if trimmed.is_empty() {
                lines.push(Line::raw(""));
                continue;
            }

            match section {
                Section::Directories => {
                    let name = trimmed.trim_end_matches('/');
                    lines.push(Line::styled(
                        format!("      {}/", name),
                        Style::default().fg(DIR_COLOR),
                    ));
                }
                Section::Files => {
                    // Files may have size info: "filename  (1.2 KB)" or just "filename"
                    if let Some(paren_start) = trimmed.rfind("  (") {
                        let name = &trimmed[..paren_start];
                        let size = &trimmed[paren_start + 2..];
                        lines.push(Line::from(vec![
                            Span::raw("      "),
                            Span::styled(name.to_string(), Style::default().fg(FILE_COLOR)),
                            Span::styled(format!("  {}", size), Style::default().fg(SIZE_COLOR)),
                        ]));
                    } else {
                        lines.push(Line::styled(
                            format!("      {}", trimmed),
                            Style::default().fg(FILE_COLOR),
                        ));
                    }
                }
                Section::None => {
                    // Lines before any section header
                    lines.push(Line::styled(
                        format!("      {}", trimmed),
                        Style::default().fg(FILE_COLOR),
                    ));
                }
            }
        }

        if lines.is_empty() {
            return vec![Line::styled("      (empty)", MUTED_STYLE)];
        }

        lines
    }
}

#[derive(Clone, Copy)]
enum Section {
    None,
    Directories,
    Files,
}
