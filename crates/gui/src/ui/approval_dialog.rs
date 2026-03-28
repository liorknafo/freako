use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};

use crate::app::Message;

#[allow(dead_code)]
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub expanded: bool,
    /// If set, this approval is for a tool inside a sub-agent, and the response
    /// should be routed through the sub-agent's approval channel.
    pub sub_agent_parent: Option<String>,
}

pub fn view(approval: &PendingApproval) -> Element<'_, Message> {
    let scope_message = match approval.tool_name.as_str() {
        "write_file" | "edit_file" => {
            if let Some(path) = approval.arguments.get("path").and_then(|v| v.as_str()) {
                let cwd = std::env::current_dir().ok();
                let file_path = std::path::Path::new(path);
                let is_in_cwd = cwd.as_ref().map_or(false, |cwd| {
                    file_path.canonicalize()
                        .ok()
                        .map_or(false, |p| p.starts_with(cwd))
                });

                if is_in_cwd {
                    format!("'Approve for Session' will allow all {} operations on this file", approval.tool_name)
                } else {
                    "File is outside working directory. 'Approve for Session' will approve this operation only.".to_string()
                }
            } else {
                "'Approve for Session' will approve this operation only".to_string()
            }
        }
        "shell" => "'Approve for Session' will approve this shell command only".to_string(),
        _ => "'Approve for Session' will approve all uses of this tool".to_string(),
    };

    let args_view = crate::ui::code_view::approval_arguments_view_collapsible(&approval.arguments, approval.expanded);

    let content = column![
        text("Tool Approval Required").size(18),
        text(format!("Tool: {}", approval.tool_name)).size(14),
        text("Arguments:").size(12),
        args_view,
        text(scope_message).size(11).style(|theme: &iced::Theme| {
            text::Style {
                color: Some(theme.palette().text.scale_alpha(0.7)),
            }
        }),
        row![
            button("Approve Once").on_press(Message::ApprovalApprove).padding([6, 14]),
            button("Approve for Session").on_press(Message::ApprovalApproveSession).padding([6, 14]),
            button("Always Approve").on_press(Message::ApprovalApproveAlways).padding([6, 14]),
            button("Deny").on_press(Message::ApprovalDeny).padding([6, 14]),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .padding(20)
    .max_width(900);

    container(
        container(content)
            .style(|theme: &iced::Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(palette.background.into()),
                    border: iced::Border {
                        radius: 12.0.into(),
                        width: 1.0,
                        color: palette.text,
                    },
                    ..Default::default()
                }
            }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(|_: &iced::Theme| container::Style {
        background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5).into()),
        ..Default::default()
    })
    .into()
}
