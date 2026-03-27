use iced::widget::{button, container, pick_list, row, text};
use iced::{Color, Element, Length};

use freako_core::config::types::ProviderType;
use crate::app::{compaction_progress, App, Message};
use crate::ui::theme::AppTheme;

/// Braille spinner frames — cycles through 8 frames.
const SPINNER_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠇"];

pub fn view(app: &App) -> Element<'_, Message> {
    let provider_label = app.config.provider.provider_type.label();
    let model = &app.config.provider.model;
    let tokens = format!(
        "in: {} / out: {}",
        app.session.total_input_tokens, app.session.total_output_tokens
    );

    let spinner = SPINNER_FRAMES[app.spinner_tick as usize];

    let (status, status_color) = if let Some(ref tool) = app.current_tool {
        (
            format!("{} Running: {}", spinner, tool),
            AppTheme::tool_active(),
        )
    } else if let Some(ref retry_status) = app.retry_status {
        (
            format!("{} {}", spinner, retry_status),
            AppTheme::warning(),
        )
    } else if app.is_thinking {
        (
            format!("{} Thinking…", spinner),
            AppTheme::thinking(),
        )
    } else if app.is_working {
        (
            format!("{} Working…", spinner),
            AppTheme::accent(),
        )
    } else {
        ("● Ready".to_string(), AppTheme::success())
    };
    let status = if app.plan_pending_review {
        format!("{} (plan ready for review)", status)
    } else {
        status
    };

    let settings_btn = button(
        text("Settings").size(12).color(AppTheme::text_secondary()),
    )
    .on_press(Message::ToggleSettings)
    .padding([4, 12])
    .style(|_t: &iced::Theme, status| {
        let bg = match status {
            button::Status::Hovered => AppTheme::sidebar_item_hover(),
            _ => Color::TRANSPARENT,
        };
        button::Style {
            background: Some(bg.into()),
            text_color: AppTheme::text_secondary(),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    });

    let compact_btn = button(
        text("Compact Now (Ctrl+M)").size(12).color(AppTheme::text_primary()),
    )
    .on_press(Message::CompactNow)
    .padding([4, 12])
    .style(|_t: &iced::Theme, status| {
        let bg = match status {
            button::Status::Hovered => AppTheme::accent_hover(),
            _ => AppTheme::accent(),
        };
        button::Style {
            background: Some(bg.into()),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    });

    let plan_mode = app.config.plan_mode;
    let (plan_label, plan_fg, plan_bg, plan_bg_hover) = if plan_mode {
        (
            "Plan Mode",
            AppTheme::plan_fg(),
            AppTheme::plan_accent(),
            AppTheme::plan_accent_hover(),
        )
    } else {
        (
            "Execute Mode",
            AppTheme::text_secondary(),
            AppTheme::sidebar_bg(),
            AppTheme::sidebar_item_hover(),
        )
    };
    let plan_btn = button(
        text(plan_label).size(12).color(plan_fg),
    )
    .on_press(Message::TogglePlanMode)
    .padding([4, 12])
    .style(move |_t: &iced::Theme, status| {
        let bg = match status {
            button::Status::Hovered => plan_bg_hover,
            _ => plan_bg,
        };
        button::Style {
            background: Some(bg.into()),
            text_color: plan_fg,
            border: iced::Border {
                radius: 4.0.into(),
                width: if plan_mode { 0.0 } else { 1.0 },
                color: AppTheme::border_default(),
            },
            ..Default::default()
        }
    });

    let provider_types: Vec<String> = ProviderType::ALL.iter().map(|p| p.label().to_string()).collect();
    let provider_picker = pick_list(
        provider_types,
        Some(provider_label.to_string()),
        Message::ProviderTypeChanged,
    )
    .padding([4, 10]);

    let model_picker = pick_list(
        app.models_for_current_provider(),
        Some(model.clone()),
        Message::ModelChanged,
    )
    .padding([4, 10]);

    // Count parts (interactions), not just top-level messages, for accurate progress
    let interaction_count: usize = app.session.messages.iter().map(|m| m.parts.len().max(1)).sum();
    let compaction: Option<Element<'_, Message>> = compaction_progress(interaction_count, &app.config.context).map(|progress| {
        let label = if progress.threshold_reached {
            format!("Compaction: 100% (active)")
        } else {
            format!(
                "Compaction: {}% ({} left)",
                progress.percent,
                progress.remaining_messages
            )
        };

        text(label)
            .size(12)
            .color(if progress.threshold_reached {
                AppTheme::plan_accent()
            } else {
                AppTheme::role_user()
            })
            .into()
    });

    container(
        row![
            text("Provider").size(12).color(AppTheme::text_muted()),
            provider_picker,
            text("Model").size(12).color(AppTheme::text_muted()),
            model_picker,
            iced::widget::space::horizontal().width(Length::Fill),
            if let Some(compaction) = compaction { compaction } else { text("").size(12).into() },
            text(tokens).size(12).color(AppTheme::text_secondary()),
            text(" | ").size(12).color(AppTheme::text_muted()),
            text(status).size(12).color(status_color),
            compact_btn,
            plan_btn,
            settings_btn,
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .padding([6, 16])
    .width(Length::Fill)
    .style(|_t: &iced::Theme| container::Style {
        background: Some(AppTheme::status_bg().into()),
        border: iced::Border {
            width: 1.0,
            color: AppTheme::border_subtle(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}
