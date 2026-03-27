use iced::widget::{button, column, container, row, scrollable, text, text_input, Column};
use iced::{Color, Element, Length};

use freako_core::config::types::ProviderType;
use freako_core::memory::types::MemoryScope;
use crate::app::{App, Message, OAuthStatus};
use crate::ui::widgets::{card, labeled_field};

/// Small link-style button to open a URL.
fn link_button<'a>(label: &str, url: &str) -> Element<'a, Message> {
    button(
        text(label.to_string()).size(12).color(Color::from_rgb(0.4, 0.6, 1.0)),
    )
    .on_press(Message::OpenUrl(url.to_string()))
    .padding([4, 0])
    .style(|_t: &iced::Theme, status| {
        button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered => Color::from_rgb(0.5, 0.7, 1.0),
                _ => Color::from_rgb(0.4, 0.6, 1.0),
            },
            ..Default::default()
        }
    })
    .into()
}

fn section_title<'a>(title: &'a str, subtitle: &'a str) -> Column<'a, Message> {
    Column::new()
        .spacing(4)
        .push(text(title).size(16).color(Color::from_rgb(0.92, 0.92, 0.95)))
        .push(text(subtitle).size(12).color(Color::from_rgb(0.5, 0.5, 0.55)))
}

/// API KEY label row with a "Get API Key" link button.
fn api_key_field_with_link<'a>(
    placeholder: &str,
    value: &str,
    key_url: &str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message> {
    Column::new()
        .spacing(4)
        .push(
            row![
                text("API KEY").size(12).color(Color::from_rgb(0.55, 0.55, 0.6)),
                iced::widget::space::horizontal().width(Length::Fill),
                link_button("Get API Key ->", key_url),
            ]
            .align_y(iced::Alignment::Center),
        )
        .push(
            text_input(placeholder, value)
                .on_input(on_input)
                .secure(true)
                .padding(10),
        )
        .into()
}

fn openai_connection_card(app: &App) -> Element<'_, Message> {
    let mut fields = Column::new()
        .spacing(14)
        .push(section_title(
            "OpenAI-compatible",
            "Save your API endpoint, API key, or ChatGPT subscription login.",
        ))
        .push(labeled_field(
            "API BASE URL",
            text_input(
                "https://api.openai.com/v1",
                app.config.provider.openai_api_base.as_deref().unwrap_or(""),
            )
            .on_input(Message::OpenAIApiBaseChanged)
            .padding(10),
        ))
        .push(api_key_field_with_link(
            "sk-...",
            app.config.provider.openai_api_key.as_deref().unwrap_or(""),
            "https://platform.openai.com/api-keys",
            Message::OpenAIApiKeyChanged,
        ))
        .push(
            container(
                text("Or use your ChatGPT subscription login")
                    .size(12)
                    .color(Color::from_rgb(0.45, 0.45, 0.5)),
            )
            .width(Length::Fill)
            .padding([4, 0]),
        );

    if app.config.provider.openai_oauth.is_some() {
        fields = fields.push(
            Column::new()
                .spacing(8)
                .push(
                    text("Connected via ChatGPT subscription")
                        .size(13)
                        .color(Color::from_rgb(0.4, 0.85, 0.5)),
                )
                .push(
                    button(text("Logout").size(13).color(Color::WHITE))
                        .on_press(Message::OAuthLogout)
                        .padding([8, 20])
                        .style(|_t: &iced::Theme, status| {
                            let bg = match status {
                                button::Status::Hovered => Color::from_rgb(0.6, 0.2, 0.2),
                                _ => Color::from_rgb(0.5, 0.15, 0.15),
                            };
                            button::Style {
                                background: Some(bg.into()),
                                text_color: Color::WHITE,
                                border: iced::Border { radius: 6.0.into(), ..Default::default() },
                                ..Default::default()
                            }
                        }),
                ),
        );
    } else {
        match &app.oauth_status {
            OAuthStatus::WaitingForCallback => {
                fields = fields.push(
                    text("Waiting for browser login...")
                        .size(13)
                        .color(Color::from_rgb(0.6, 0.6, 0.65)),
                );
            }
            OAuthStatus::Error(err) => {
                fields = fields.push(
                    Column::new()
                        .spacing(8)
                        .push(
                            text(format!("OAuth error: {}", err))
                                .size(12)
                                .color(Color::from_rgb(0.9, 0.3, 0.3)),
                        )
                        .push(
                            button(text("Retry Login with ChatGPT").size(13).color(Color::WHITE))
                                .on_press(Message::OAuthStart)
                                .padding([8, 20])
                                .style(|_t: &iced::Theme, status| {
                                    let bg = match status {
                                        button::Status::Hovered => Color::from_rgb(0.85, 0.55, 0.2),
                                        _ => Color::from_rgb(0.78, 0.48, 0.15),
                                    };
                                    button::Style {
                                        background: Some(bg.into()),
                                        text_color: Color::WHITE,
                                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                                        ..Default::default()
                                    }
                                }),
                        ),
                );
            }
            OAuthStatus::Idle => {
                fields = fields.push(
                    Column::new()
                        .spacing(6)
                        .push(
                            text("Use your ChatGPT Plus / Pro subscription")
                                .size(12)
                                .color(Color::from_rgb(0.45, 0.45, 0.5)),
                        )
                        .push(
                            button(text("Login with ChatGPT").size(13).color(Color::WHITE))
                                .on_press(Message::OAuthStart)
                                .padding([8, 20])
                                .style(|_t: &iced::Theme, status| {
                                    let bg = match status {
                                        button::Status::Hovered => Color::from_rgb(0.2, 0.75, 0.4),
                                        _ => Color::from_rgb(0.15, 0.65, 0.35),
                                    };
                                    button::Style {
                                        background: Some(bg.into()),
                                        text_color: Color::WHITE,
                                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                                        ..Default::default()
                                    }
                                }),
                        ),
                );
            }
        }
    }

    card("OpenAI", fields)
}

fn anthropic_connection_card(app: &App) -> Element<'_, Message> {
    let fields = Column::new()
        .spacing(14)
        .push(section_title(
            "Anthropic",
            "Save your Anthropic API key so you can switch providers from chat instantly.",
        ))
        .push(api_key_field_with_link(
            "sk-ant-api-...",
            app.config.provider.anthropic_api_key.as_deref().unwrap_or(""),
            "https://console.anthropic.com/settings/keys",
            Message::AnthropicApiKeyChanged,
        ));

    card("Anthropic", fields)
}

fn bedrock_connection_card(app: &App) -> Element<'_, Message> {
    let fields = Column::new()
        .spacing(14)
        .push(section_title(
            "AWS Bedrock",
            "Optional: configure Bedrock so it is also available in the chat switcher.",
        ))
        .push(labeled_field(
            "AWS REGION",
            text_input("us-east-1", app.config.provider.aws_region.as_deref().unwrap_or(""))
                .on_input(Message::AwsRegionChanged)
                .padding(10),
        ))
        .push(labeled_field(
            "AWS PROFILE",
            text_input("default", app.config.provider.aws_profile.as_deref().unwrap_or(""))
                .on_input(Message::AwsProfileChanged)
                .padding(10),
        ));

    card("Bedrock", fields)
}

pub fn view(app: &App) -> Element<'_, Message> {
    let provider_types: Vec<String> = ProviderType::ALL.iter().map(|p| p.label().to_string()).collect();
    let selected = app.config.provider.provider_type.label().to_string();

    let provider_card = card(
        "Active Provider",
        Column::new()
            .spacing(14)
            .push(text("Choose the provider you want to use right now. You can also change this directly from the chat bar.")
                .size(12)
                .color(Color::from_rgb(0.5, 0.5, 0.55)))
            .push(labeled_field(
                "CURRENT PROVIDER",
                iced::widget::pick_list(provider_types, Some(selected), Message::ProviderTypeChanged)
                    .padding(10),
            )),
    );

    let behavior_card = {
        let fields = Column::new().spacing(14)
            .push(labeled_field(
                "MAX TOKENS",
                text_input("4096", &app.config.provider.max_tokens.to_string())
                    .on_input(Message::MaxTokensChanged).padding(10),
            ))
            .push(labeled_field(
                "TEMPERATURE",
                text_input("0.7", &app.config.provider.temperature.map(|t| t.to_string()).unwrap_or_default())
                    .on_input(Message::TemperatureChanged).padding(10),
            ));
        card("Generation", fields)
    };

    let context_card = {
        let fields = Column::new().spacing(14)
            .push(
                text("Automatically compacts older messages for provider requests while keeping full session history visible and persisted.")
                    .size(12)
                    .color(Color::from_rgb(0.45, 0.45, 0.5)),
            )
            .push(labeled_field(
                "ENABLE COMPACTION",
                text_input(
                    "true",
                    if app.config.context.enable_compaction { "true" } else { "false" },
                )
                .on_input(Message::ContextCompactionEnabledChanged)
                .padding(10),
            ))
            .push(labeled_field(
                "COMPACT AFTER MESSAGES",
                text_input("40", &app.config.context.compact_after_messages.to_string())
                    .on_input(Message::CompactAfterMessagesChanged)
                    .padding(10),
            ))
            .push(labeled_field(
                "KEEP RECENT MESSAGES",
                text_input("12", &app.config.context.keep_recent_messages.to_string())
                    .on_input(Message::KeepRecentMessagesChanged)
                    .padding(10),
            ));
        card("Context", fields)
    };

    let skills_card = {
        let source_rows = app
            .config
            .skills
            .sources
            .iter()
            .enumerate()
            .fold(Column::new().spacing(8), |col, (index, source)| {
                col.push(
                    row![
                        text_input("vercel-labs/agent-skills", source)
                            .on_input(move |value| Message::SkillsSourceChanged(index, value))
                            .padding(10)
                            .width(Length::Fill),
                        button(text("-").size(16))
                            .on_press(Message::RemoveSkillsSource(index))
                            .padding([8, 12]),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
            });

        let fields = Column::new().spacing(14)
            .push(
                text("Discover local skills and load repo-based skill sources like Vercel/OpenCode.")
                    .size(12)
                    .color(Color::from_rgb(0.45, 0.45, 0.5)),
            )
            .push(labeled_field(
                "ENABLE SKILLS",
                text_input(
                    "true",
                    if app.config.skills.enabled { "true" } else { "false" },
                )
                .on_input(Message::SkillsEnabledChanged)
                .padding(10),
            ))
            .push(
                Column::new()
                    .spacing(8)
                    .push(text("SKILL SOURCES").size(12).color(Color::from_rgb(0.55, 0.55, 0.6)))
                    .push(source_rows)
                    .push(
                        button(text("+").size(16))
                            .on_press(Message::AddSkillsSource)
                            .padding([8, 12]),
                    ),
            );
        card("Skills", fields)
    };

    let prompt_card = {
        let project_memory = app
            .memory_entries
            .iter()
            .find(|entry| matches!(entry.scope, MemoryScope::Project))
            .map(|entry| entry.content.as_str())
            .unwrap_or("");
        let global_memory = app
            .memory_entries
            .iter()
            .find(|entry| matches!(entry.scope, MemoryScope::Global))
            .map(|entry| entry.content.as_str())
            .unwrap_or("");

        let fields = Column::new().spacing(8)
            .push(
                text("Appended to the built-in system prompt.")
                    .size(12)
                    .color(Color::from_rgb(0.45, 0.45, 0.5)),
            )
            .push(
                text_input(
                    "e.g. Always respond in Spanish...",
                    app.config.system_prompt.as_deref().unwrap_or(""),
                )
                .on_input(Message::SystemPromptChanged)
                .padding(10),
            )
            .push(
                text("Persistent project memory for this working directory.")
                    .size(12)
                    .color(Color::from_rgb(0.45, 0.45, 0.5)),
            )
            .push(
                text_input("Project memory...", project_memory)
                    .on_input(Message::MemoryBankProjectChanged)
                    .padding(10),
            )
            .push(
                text("Persistent global memory shared across repos.")
                    .size(12)
                    .color(Color::from_rgb(0.45, 0.45, 0.5)),
            )
            .push(
                text_input("Global memory...", global_memory)
                    .on_input(Message::MemoryBankGlobalChanged)
                    .padding(10),
            );
        card("Additional Instructions & Memory", fields)
    };

    let header = Column::new()
        .spacing(4)
        .push(text("Settings").size(26).color(Color::from_rgb(0.92, 0.92, 0.95)))
        .push(text("Manage saved provider connections and app behavior. Model switching now lives in the chat bar.")
            .size(13)
            .color(Color::from_rgb(0.5, 0.5, 0.55)));

    let close_btn = container(
        button(
            text("Save & Close").size(14).color(Color::WHITE),
        )
        .on_press(Message::ToggleSettings)
        .padding([12, 28])
        .style(|_t: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => Color::from_rgb(0.3, 0.55, 1.0),
                _ => Color::from_rgb(0.26, 0.47, 0.9),
            };
            button::Style {
                background: Some(bg.into()),
                text_color: Color::WHITE,
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }),
    )
    .width(Length::Fill)
    .center_x(Length::Fill);

    let content = column![
        header,
        provider_card,
        openai_connection_card(app),
        anthropic_connection_card(app),
        bedrock_connection_card(app),
        behavior_card,
        context_card,
        skills_card,
        prompt_card,
        close_btn,
        text("").size(16),
    ]
    .spacing(20)
    .padding(40)
    .max_width(700);

    container(
        scrollable(
            container(content)
                .width(Length::Fill)
                .center_x(Length::Fill),
        ),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_t: &iced::Theme| container::Style {
        background: Some(Color::from_rgb(0.09, 0.09, 0.11).into()),
        ..Default::default()
    })
    .into()
}
