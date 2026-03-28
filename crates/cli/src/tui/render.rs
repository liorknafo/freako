use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use freako_core::tools::tool_name::format_tool_presentation;

use super::message::{
    ansi_line_spans, build_grouped_messages, fallback_tool_summary, message_style, render_message_lines,
    truncate_middle,
};
use super::{
    App, InputMode, PendingApproval, ASSISTANT_STYLE, CHAT_BG, CODE_BLOCK_BG, INPUT_BG,
    INPUT_CURSOR_STYLE, INPUT_HINT_STYLE, MUTED_STYLE, PendingShellViewport, SELECTED_BG,
    SHELL_CONSOLE_BG, SHELL_CONSOLE_BORDER, SHELL_CONSOLE_FG, SHELL_CONSOLE_VISIBLE_LINES,
    SIDEBAR_BG, SPINNER_FRAMES, THINKING_STYLE, TOOL_STYLE,
};

fn approval_options(_kind: super::ApprovalKind) -> &'static [&'static str] {
    &["Approve", "Approve for Session", "Always Approve", "Deny"]
}

pub(super) fn ui(frame: &mut ratatui::Frame, app: &mut App) {
    app.shell_mouse_regions.clear();
    let main_area = if app.selecting_session {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(34), Constraint::Min(20)])
            .split(frame.area());
        render_session_sidebar(frame, app, chunks[0]);
        chunks[1]
    } else {
        frame.area()
    };

    // Settings mode: render full-screen settings panel
    if matches!(app.input_mode, InputMode::Settings) {
        super::settings::render_settings(frame, &app.settings_state, &app.config, main_area);
        return;
    }

    let input_height = if matches!(app.input_mode, InputMode::WaitingApproval) {
        8
    } else {
        let inner_width = main_area.width.saturating_sub(2).max(1) as usize;
        let visual_lines: usize = app.input.split('\n').map(|line| {
            let chars = line.chars().count();
            if chars == 0 { 1 } else { (chars + inner_width - 1) / inner_width }
        }).sum();
        (visual_lines as u16 + 2).max(3).min(main_area.height / 2)
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(1), Constraint::Length(input_height)])
        .split(main_area);

    let plan_panel_width: u16 = if app.plan_tasks.is_empty() || app.plan_panel_hidden {
        0
    } else {
        36
    };
    let chat_plan_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Length(plan_panel_width),
        ])
        .split(main_chunks[0]);

    app.chat_area_rect = chat_plan_chunks[0];
    app.plan_panel_rect = chat_plan_chunks[1];

    let width = chat_plan_chunks[0].width;
    let inner_width = width;
    let chat_area_height = chat_plan_chunks[0].height as usize;

    // Rebuild the cached chat lines when messages change, terminal resizes, or collapse toggles
    let msg_count = app.session.messages.len();
    let collapse_gen = app.collapse_generation;
    if msg_count != app.chat_cache_msg_count || width != app.chat_cache_width || collapse_gen != app.chat_cache_collapse_gen {
        let grouped = build_grouped_messages(&app.session.messages);
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        app.tool_header_line_indices.clear();
        for msg in &grouped {
            let global_offset = lines.len();
            let style = message_style(msg.role);
            let prev_header_count = app.tool_header_line_indices.len();
            lines.extend(render_message_lines(msg, style, false, width, &app.tool_views, &app.tool_collapse_state, &mut app.tool_header_line_indices));
            // Fix header indices: render_message_lines records local indices,
            // but we need global indices in the cache.
            for entry in app.tool_header_line_indices.iter_mut().skip(prev_header_count) {
                entry.1 += global_offset;
            }
            lines.push(Line::raw(""));
            ranges.push((global_offset, lines.len()));
        }
        // Compute wrapped visual positions for tool headers using ratatui's own measurement
        let mut header_wrapped_positions: Vec<(String, u32)> = Vec::new();
        if !app.tool_header_line_indices.is_empty() {
            // For each header, measure total wrapped lines for all lines before it
            let mut sorted: Vec<(usize, &str)> = app.tool_header_line_indices
                .iter()
                .map(|(id, idx)| (*idx, id.as_str()))
                .collect();
            sorted.sort_by_key(|(idx, _)| *idx);

            let mut prev_idx = 0usize;
            let mut cumulative_wrapped: u32 = 0;
            for (hdr_idx, tool_call_id) in &sorted {
                // Measure lines from prev_idx to hdr_idx (exclusive)
                if *hdr_idx > prev_idx {
                    let slice: Vec<Line<'static>> = lines[prev_idx..*hdr_idx].to_vec();
                    let p = Paragraph::new(slice).wrap(Wrap { trim: false });
                    cumulative_wrapped += p.line_count(inner_width) as u32;
                }
                header_wrapped_positions.push((tool_call_id.to_string(), cumulative_wrapped));
                prev_idx = *hdr_idx;
            }
        }

        app.chat_lines_cache = lines;
        app.chat_ranges_cache = ranges;
        app.chat_cache_msg_count = msg_count;
        app.chat_cache_width = width;
        app.chat_cache_collapse_gen = collapse_gen;
        app.tool_header_wrapped_positions = header_wrapped_positions;
    }

    // Ephemeral tail lines (streaming, thinking) — always recomputed
    let mut tail_lines: Vec<Line> = Vec::new();
    for m in &app.messages {
        tail_lines.push(Line::styled(m.clone(), Style::default().fg(Color::Red)));
    }
    if !app.streaming_text.is_empty() {
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        tail_lines.extend(render_streaming_markdown(&app.streaming_text, spinner, width));
    }
    if !app.streaming_tool_calls.is_empty() || !app.live_shell_outputs.is_empty() {
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        tail_lines.extend(render_streaming_tools(app, spinner));
    }
    if app.is_thinking && app.streaming_text.is_empty() {
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        tail_lines.push(Line::styled(format!("{} Thinking…", spinner), THINKING_STYLE));
    }

    let mut chat_lines = app.chat_lines_cache.clone();
    chat_lines.extend(tail_lines);

    // Fill chat background first, then render content on top
    frame.render_widget(Block::default().style(Style::new().bg(CHAT_BG)), chat_plan_chunks[0]);
    let chat = Paragraph::new(chat_lines)
        .style(Style::new().bg(CHAT_BG))
        .wrap(Wrap { trim: false });
    let total_lines = chat.line_count(inner_width);
    let max_scroll = total_lines.saturating_sub(chat_area_height) as u16;
    // Clamp scroll_offset so it doesn't accumulate beyond content
    if app.scroll_offset > max_scroll {
        app.scroll_offset = max_scroll;
    }
    let scroll = max_scroll.saturating_sub(app.scroll_offset);
    let chat = chat.scroll((scroll, 0));
    frame.render_widget(chat, chat_plan_chunks[0]);

    // Store render-time layout values for on-demand click mapping
    app.last_scroll = scroll;
    app.last_inner_width = inner_width;
    app.last_chat_area_y = chat_plan_chunks[0].y;
    app.last_chat_area_height = chat_area_height;

    // Map precomputed wrapped positions to screen coordinates
    app.tool_header_regions.clear();
    let chat_area_y = app.last_chat_area_y;
    let chat_area_x = chat_plan_chunks[0].x;
    for (tool_call_id, wrapped_pos) in &app.tool_header_wrapped_positions {
        let screen_row = *wrapped_pos as i32 - scroll as i32;
        if screen_row >= 0 && (screen_row as usize) < chat_area_height {
            app.tool_header_regions.push((
                tool_call_id.clone(),
                Rect {
                    x: chat_area_x,
                    y: chat_area_y + screen_row as u16,
                    width: inner_width,
                    height: 1,
                },
            ));
        }
    }

    app.plan_task_header_rows.clear();
    if !app.plan_tasks.is_empty() {
        let title = if app.plan_focused {
            "Plan ('f'/Esc exit)"
        } else if app.plan_pending_review {
            "Plan (Enter approve)"
        } else {
            "Plan"
        };
        let mut lines: Vec<Line<'_>> = Vec::new();
        // Track which visual line index corresponds to a task header
        let mut header_line_indices: Vec<(String, usize)> = Vec::new();
        for (i, task) in app.plan_tasks.iter().enumerate() {
            let is_selected = app.plan_focused && i == app.plan_selected_task;
            let is_expanded = app.plan_task_expanded.contains(&task.id);
            let chevron = if is_expanded { "▼" } else { "▶" };
            let status_prefix = match task.status {
                freako_core::agent::events::TaskStatus::NotStarted => "[ ] ",
                freako_core::agent::events::TaskStatus::InProgress => "[~] ",
                freako_core::agent::events::TaskStatus::Done => "[x] ",
            };
            let header_style = if is_selected {
                Style::new().fg(Color::Indexed(252)).bg(SELECTED_BG).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::Indexed(252)).add_modifier(Modifier::BOLD)
            };
            let status_style = if is_selected {
                Style::new().fg(Color::Indexed(214)).bg(SELECTED_BG)
            } else {
                Style::new().fg(Color::Indexed(214))
            };
            let chevron_style = if is_selected {
                Style::new().fg(Color::Indexed(243)).bg(SELECTED_BG)
            } else {
                Style::new().fg(Color::Indexed(243))
            };
            header_line_indices.push((task.id.clone(), lines.len()));
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", chevron), chevron_style),
                Span::styled(status_prefix, status_style),
                Span::styled(task.header.clone(), header_style),
            ]));
            if is_expanded && !task.description.is_empty() {
                let panel_inner_width = 30_u16; // 36 panel - borders/padding/indent
                let md_lines = super::message::markdown_to_lines(&task.description, panel_inner_width);
                for md_line in md_lines {
                    let mut indented: Vec<Span<'static>> = vec![Span::raw("  ")];
                    indented.extend(md_line.spans);
                    lines.push(Line::from(indented));
                }
                lines.push(Line::from(""));
            }
        }
        let panel_rect = chat_plan_chunks[1];
        // +1 for the block border/title row
        let content_y = panel_rect.y + 1;
        let visible_height = panel_rect.height.saturating_sub(1) as usize; // minus title row
        let max_scroll = (lines.len()).saturating_sub(visible_height) as u16;
        if app.plan_scroll_offset > max_scroll {
            app.plan_scroll_offset = max_scroll;
        }
        let scroll = app.plan_scroll_offset;
        for (task_id, line_idx) in header_line_indices {
            let screen_y = content_y + line_idx as u16;
            // Only register clickable rows that are visible after scrolling
            if line_idx as u16 >= scroll {
                app.plan_task_header_rows.push((task_id, screen_y - scroll));
            }
        }
        let plan = Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::NONE).style(Style::new().bg(SIDEBAR_BG)))
            .style(Style::new().bg(SIDEBAR_BG))
            .scroll((scroll, 0));
        frame.render_widget(plan, panel_rect);
    }

    render_status_bar(frame, app, main_chunks[1]);

    let plan_label = if app.config.plan_mode { "[PLAN MODE]" } else { "[EXECUTE]" };
    let input_title: String = match app.input_mode {
        InputMode::Normal if app.plan_focused => format!("{} [PLAN] ↑↓ select | Enter toggle | Esc/f exit", plan_label),
        InputMode::Normal => format!("{} 'i' type | ↑↓ scroll | 'l' sessions | 'p' plan | 'f' tasks | 'h' hide | 'o' settings | 'c' compact | 'n' new | 'q' quit", plan_label),
        InputMode::Editing if app.plan_pending_review => format!("{} Plan ready — Enter to approve, or type feedback", plan_label),
        InputMode::Editing if app.config.plan_mode => format!("{} PLAN MODE — Enter send, Esc normal", plan_label),
        InputMode::Editing => format!("{} Enter send, Ctrl+V paste image, Esc normal", plan_label),
        InputMode::WaitingApproval => "Approval required".to_string(),
        InputMode::Settings => "Settings".to_string(),
    };

    if matches!(app.input_mode, InputMode::WaitingApproval) {
        if let Some(pending) = &app.pending_approval {
            render_approval_panel(frame, pending, app.approval_cursor, main_chunks[2]);
        }
    } else {
        let title_with_images = if app.pending_images.is_empty() {
            input_title
        } else {
            format!("{} │ 📎 {} image(s) attached (Alt+V to add more)", input_title, app.pending_images.len())
        };
        let input_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(main_chunks[2]);
        let input_hint = Paragraph::new(title_with_images)
            .style(Style::new().fg(Color::Black).bg(Color::White));
        frame.render_widget(input_hint, input_chunks[0]);
        let input_block = Block::default().borders(Borders::NONE).style(Style::new().bg(INPUT_BG));
        let input = Paragraph::new(app.input.as_str()).style(Style::new().bg(INPUT_BG)).block(input_block).wrap(Wrap { trim: false });
        frame.render_widget(input, input_chunks[1]);

        if matches!(app.input_mode, InputMode::Editing) {
            let inner_w = input_chunks[1].width.max(1) as usize;
            let mut cursor_row: u16 = 0;
            let mut cursor_col: u16 = 0;
            for (i, line) in app.input.split('\n').enumerate() {
                if i > 0 {
                    cursor_row += 1;
                }
                let chars = line.chars().count();
                let wrapped_rows = if chars == 0 { 0 } else { chars / inner_w };
                cursor_row += wrapped_rows as u16;
                cursor_col = (chars % inner_w) as u16;
            }
            let cx = input_chunks[1].x + cursor_col;
            let cy = input_chunks[1].y + cursor_row;
            let max_y = input_chunks[1].y + input_chunks[1].height.saturating_sub(1);
            if cx < input_chunks[1].x + input_chunks[1].width && cy <= max_y {
                frame.render_widget(Paragraph::new(" ").style(INPUT_CURSOR_STYLE), Rect { x: cx, y: cy, width: 1, height: 1 });
            }
        }
    }
}

fn render_approval_panel(frame: &mut ratatui::Frame, pending: &PendingApproval, cursor: usize, area: Rect) {
    let options = approval_options(pending.kind);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let body = format!("Tool: {}\n\n{}", pending.tool_name, pending.args_json);
    let paragraph = Paragraph::new(body)
        .block(Block::default().title("Approval required").style(Style::new().bg(super::APPROVAL_BG)))
        .style(Style::new().bg(super::APPROVAL_BG))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[0]);

    frame.render_widget(Paragraph::new("Use ←/→ or Tab to choose, Enter to confirm, Esc to deny").style(Style::new().bg(super::APPROVAL_BG)), chunks[1]);

    let spans = options.iter().enumerate().flat_map(|(i, label)| {
        let style = if i == cursor {
            Style::new().fg(Color::Black).bg(SELECTED_BG).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };
        let mut v = vec![Span::styled(format!(" {} ", label), style)];
        if i + 1 < options.len() {
            v.push(Span::raw("  "));
        }
        v
    }).collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Line::from(spans)).style(Style::new().bg(super::APPROVAL_BG)), chunks[2]);
}

fn render_session_sidebar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Min(5)]).split(area);
    frame.render_widget(Paragraph::new(" Sessions ").alignment(Alignment::Center).style(Style::new().bg(SIDEBAR_BG).add_modifier(Modifier::BOLD)), chunks[0]);

    let mut list_lines = Vec::new();
    if app.session_list.is_empty() {
        list_lines.push(Line::styled("No saved sessions", Style::new().bg(SIDEBAR_BG)));
    } else {
        for (i, (_, title, updated)) in app.session_list.iter().enumerate() {
            let selected = app.selecting_session && i == app.session_list_selected;
            let prefix = if selected { "> " } else { "  " };
            let style = if selected {
                Style::new().fg(Color::White).bg(SELECTED_BG).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::Indexed(252)).bg(SIDEBAR_BG)
            };
            list_lines.push(Line::styled(format!("{}{}", prefix, truncate_middle(title, 28)), style));
            list_lines.push(Line::styled(format!("   {}", truncate_middle(updated, 28)), MUTED_STYLE.bg(SIDEBAR_BG)));
        }
    }

    let list = Paragraph::new(list_lines)
        .block(Block::default().borders(Borders::NONE).title("Sessions").title_bottom(" Enter load │ x delete │ Esc close ").style(Style::new().bg(SIDEBAR_BG)))
        .style(Style::new().bg(SIDEBAR_BG))
        .wrap(Wrap { trim: false });
    frame.render_widget(list, chunks[1]);
}

fn render_status_bar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let provider = match app.config.provider.provider_type {
        freako_core::config::types::ProviderType::OpenAI => {
            if app.config.provider.openai_oauth.is_some() {
                "OpenAI (ChatGPT)"
            } else {
                "OpenAI (API)"
            }
        }
        _ => app.config.provider.provider_type.label(),
    };
    let model = &app.config.provider.model;
    let in_tok = app.session.total_input_tokens;
    let out_tok = app.session.total_output_tokens;

    let compaction_str = if app.config.context.enable_compaction {
        let trigger_at = app.config.context.compact_after_input_tokens.max(1);
        let current = app.session.total_input_tokens;
        if current >= trigger_at {
            " │ ⚡ compaction pending".to_string()
        } else {
            let pct = ((current as u64).saturating_mul(100) / trigger_at as u64).min(100);
            let remaining = trigger_at.saturating_sub(current);
            format!(" │ 📊 {}% ({}k tokens until compact)", pct, remaining / 1000)
        }
    } else {
        String::new()
    };

    let status_str = if let Some(ref tool) = app.current_tool {
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        format!(" │ {} Running: {}", spinner, tool)
    } else if app.is_thinking {
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        format!(" │ {} Thinking…", spinner)
    } else if app.is_working {
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        format!(" │ {} Working…", spinner)
    } else {
        " │ Ready".to_string()
    };

    let plan_indicator = if app.config.plan_mode { " │ 📋 PLAN" } else { "" };
    let thinking_indicator = match app.config.provider.thinking_effort {
        Some(freako_core::provider::types::ThinkingEffort::Low) => " │ 🧠 low",
        Some(freako_core::provider::types::ThinkingEffort::Medium) => " │ 🧠 med",
        Some(freako_core::provider::types::ThinkingEffort::High) => " │ 🧠 high",
        None => "",
    };
    let bar_text = format!(" {} / {} │ in:{} out:{}{}{}{}{}", provider, model, in_tok, out_tok, compaction_str, plan_indicator, thinking_indicator, status_str);
    let status_bg = Color::Rgb(30, 22, 48);
    frame.render_widget(Paragraph::new(Line::from(vec![Span::styled(bar_text, Style::new().fg(Color::Indexed(252)).bg(status_bg))])).style(Style::new().bg(status_bg)), area);
}

fn render_streaming_markdown(text: &str, spinner: &str, width: u16) -> Vec<Line<'static>> {
    use super::message::markdown_to_lines;
    let mut lines = vec![Line::styled(format!("{} AI:", spinner), ASSISTANT_STYLE.add_modifier(Modifier::BOLD))];
    // markdown_to_lines wraps code block lines to (md_width - 4) already
    let md_width = width.saturating_sub(4);
    let code_content_width = md_width.saturating_sub(4) as usize; // margin + padding each side
    for l in markdown_to_lines(text, md_width) {
        if l.style.bg == Some(CODE_BLOCK_BG) {
            let content_width: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
            let right_fill = code_content_width.saturating_sub(content_width);
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(l.spans.len() + 6);
            // 2-char indent + 1-char margin (no bg)
            spans.push(Span::raw("   "));
            // 1-char left padding (CODE_BLOCK_BG)
            spans.push(Span::styled(" ", Style::new().bg(CODE_BLOCK_BG)));
            // Content — ensure CODE_BLOCK_BG
            spans.extend(l.spans.into_iter().map(|span| {
                let style = if span.style.bg.is_some() {
                    span.style
                } else {
                    span.style.patch(Style::new().bg(CODE_BLOCK_BG))
                };
                Span::styled(span.content.into_owned(), style)
            }));
            // Right fill + 1-char right padding (CODE_BLOCK_BG)
            spans.push(Span::styled(" ".repeat(right_fill + 1), Style::new().bg(CODE_BLOCK_BG)));
            // 1-char right margin (no bg)
            spans.push(Span::raw(" "));
            lines.push(Line::from(spans));
        } else {
            let mut indented: Vec<Span<'static>> = vec![Span::raw("  ")];
            indented.extend(l.spans);
            lines.push(Line::from(indented));
        }
    }
    lines
}

/// Returns (lines, sub_agent_header_indices) where header indices are relative
/// to the start of the returned lines vec.
fn render_streaming_tools(app: &mut App, spinner: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::styled(format!("{} Tools:", spinner), TOOL_STYLE.add_modifier(Modifier::BOLD)));

    let streaming_calls = app.streaming_tool_calls.clone();
    for (id, name, args) in streaming_calls {
        let (title, summary) = format_tool_presentation(&name, &args)
            .unwrap_or_else(|| (name.clone().into(), fallback_tool_summary(&name, &args)));

        if name == "sub_agent" {
            // Always render expanded while streaming — no collapse toggle needed
            if let Some(view) = app.tool_views.get(&id) {
                let inner_width = app.last_inner_width.max(40);
                // Always show uncollapsed (false) while streaming
                lines.extend(super::tools::render_tool(view.as_ref(), false, inner_width));
            } else {
                let status = "starting…";
                lines.push(Line::styled(format!("  • {} – {} – {}", title, summary, status), TOOL_STYLE));
            }
        } else if name == "shell" {
            let status = if app.live_shell_outputs.contains_key(&id) { "running…" } else { "queued…" };
            lines.push(Line::styled(format!("  • {} – {} – {}", title, summary, status), TOOL_STYLE));
            if let Some(output) = app.live_shell_outputs.get(&id).cloned() {
                let viewport = app.shell_viewport.clone();
                lines.extend(render_shell_console_lines(
                    app,
                    &id,
                    &output,
                    None,
                    true,
                    &viewport,
                    SHELL_CONSOLE_VISIBLE_LINES,
                ));
            }
        } else {
            let status = if app.live_shell_outputs.contains_key(&id) { "running…" } else { "queued…" };
            lines.push(Line::styled(format!("  • {} – {} – {}", title, summary, status), TOOL_STYLE));
        }
    }

    let shell_order: Vec<String> = app.shell_tool_order.iter().cloned().collect();
    for tool_call_id in shell_order {
        if !app.streaming_tool_calls.iter().any(|(id, _, _)| id == &tool_call_id) {
            if let Some(output) = app.live_shell_outputs.get(&tool_call_id).cloned() {
                let viewport = app.shell_viewport.clone();
                lines.extend(render_shell_console_lines(
                    app,
                    &tool_call_id,
                    &output,
                    None,
                    true,
                    &viewport,
                    SHELL_CONSOLE_VISIBLE_LINES,
                ));
            }
        }
    }
    lines
}

fn collect_shell_output_lines(output: &super::LiveShellOutput) -> Vec<Line<'static>> {
    let mut rendered = Vec::new();
    let fallback = Style::new().fg(SHELL_CONSOLE_FG).bg(SHELL_CONSOLE_BG);
    for chunk in &output.chunks {
        rendered.extend(ansi_line_spans(&chunk.text, fallback, Some(chunk.stream)));
    }
    if rendered.is_empty() {
        rendered.push(Line::styled("<no output yet>", MUTED_STYLE.bg(SHELL_CONSOLE_BG)));
    }
    rendered
}

fn render_console_viewport_hint(scrolled: usize, total_lines: usize, visible_lines: usize) -> Option<String> {
    if total_lines <= visible_lines {
        None
    } else if scrolled == 0 {
        Some(format!("tailing latest {} / {} lines", visible_lines, total_lines))
    } else {
        Some(format!("showing older output (+{} from bottom)", scrolled))
    }
}

pub(super) fn render_shell_console_lines(
    _app: &mut App,
    tool_call_id: &str,
    output: &super::LiveShellOutput,
    command: Option<&str>,
    running: bool,
    viewport: &PendingShellViewport,
    visible_lines: usize,
) -> Vec<Line<'static>> {
    let all_lines = collect_shell_output_lines(output);
    let total_lines = all_lines.len();
    let selected = viewport.tool_call_id.as_deref() == Some(tool_call_id);
    let from_bottom = if selected { viewport.scroll_lines_from_bottom.min(total_lines.saturating_sub(1)) } else { 0 };
    let window_size = visible_lines.min(total_lines.max(1));
    let end = total_lines.saturating_sub(from_bottom);
    let start = end.saturating_sub(window_size);
    let header = command.map(|cmd| truncate_middle(cmd, 72)).unwrap_or_else(|| "shell".to_string());
    let status = if running { "running" } else { "done" };

    let mut rendered = Vec::new();
    rendered.push(Line::from(vec![
        Span::styled("      ┌─ ", Style::new().fg(SHELL_CONSOLE_BORDER).bg(SHELL_CONSOLE_BG)),
        Span::styled(
            header,
            Style::new().fg(SHELL_CONSOLE_FG).bg(SHELL_CONSOLE_BG).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" [{}]", status),
            if running {
                THINKING_STYLE.bg(SHELL_CONSOLE_BG)
            } else {
                MUTED_STYLE.bg(SHELL_CONSOLE_BG)
            },
        ),
    ]));

    for line in all_lines.into_iter().skip(start).take(end.saturating_sub(start)) {
        let mut spans = vec![Span::styled(
            "      │ ",
            Style::new().fg(SHELL_CONSOLE_BORDER).bg(SHELL_CONSOLE_BG),
        )];
        spans.extend(line.spans.into_iter().map(|span| {
            let style = span.style.bg.unwrap_or(SHELL_CONSOLE_BG);
            Span::styled(
                span.content.into_owned(),
                span.style.bg(style).fg(span.style.fg.unwrap_or(SHELL_CONSOLE_FG)),
            )
        }));
        rendered.push(Line::from(spans));
    }

    let hint = render_console_viewport_hint(from_bottom, total_lines, window_size)
        .unwrap_or_else(|| format!("{} line{}", total_lines, if total_lines == 1 { "" } else { "s" }));
    rendered.push(Line::from(vec![
        Span::styled("      └─ ", Style::new().fg(SHELL_CONSOLE_BORDER).bg(SHELL_CONSOLE_BG)),
        Span::styled(hint, MUTED_STYLE.bg(SHELL_CONSOLE_BG)),
    ]));
    rendered
}

