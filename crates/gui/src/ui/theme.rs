use iced::Color;

/// App-wide color palette. Call `dark()` to get the default dark theme.
pub struct AppTheme;

impl AppTheme {
    // ── Base surfaces (darkest → lightest) ──────────────────────────
    /// Main window background — deep charcoal
    pub fn bg() -> Color { Color::from_rgb(0.06, 0.06, 0.08) }
    /// Sidebar / panel background — slightly raised from the app shell
    pub fn sidebar_bg() -> Color { Color::from_rgb(0.08, 0.08, 0.10) }
    /// Chat area background — dark conversation canvas
    pub fn chat_bg() -> Color { Color::from_rgb(0.07, 0.07, 0.09) }
    /// Input area background
    pub fn input_bg() -> Color { Color::from_rgb(0.10, 0.10, 0.13) }
    /// Status bar background
    pub fn status_bg() -> Color { Color::from_rgb(0.07, 0.07, 0.09) }

    // ── Bubble colors ───────────────────────────────────────────────
    /// User message bubble — neutral elevated surface
    pub fn user_bubble() -> Color { Color::from_rgb(0.12, 0.12, 0.15) }
    pub fn user_bubble_border() -> Color { Color::from_rgb(0.18, 0.18, 0.22) }
    /// Assistant message bubble — slightly distinct neutral surface
    pub fn assistant_bubble() -> Color { Color::from_rgb(0.10, 0.10, 0.13) }
    pub fn assistant_bubble_border() -> Color { Color::from_rgb(0.16, 0.16, 0.20) }
    /// System / tool streaming bubble
    pub fn tool_bubble() -> Color { Color::from_rgb(0.11, 0.12, 0.14) }
    pub fn tool_bubble_border() -> Color { Color::from_rgb(0.17, 0.18, 0.21) }

    // ── Text colors ─────────────────────────────────────────────────
    pub fn text_primary() -> Color { Color::from_rgb(0.88, 0.89, 0.92) }
    pub fn text_secondary() -> Color { Color::from_rgb(0.50, 0.52, 0.58) }
    pub fn text_muted() -> Color { Color::from_rgb(0.35, 0.37, 0.42) }
    /// Role labels ("You", "Assistant")
    pub fn role_user() -> Color { Color::from_rgb(0.72, 0.74, 0.80) }
    pub fn role_assistant() -> Color { Color::from_rgb(0.55, 0.57, 0.63) }

    // ── Accent / semantic colors ────────────────────────────────────
    /// Primary accent — soft electric blue
    pub fn accent() -> Color { Color::from_rgb(0.30, 0.55, 0.95) }
    pub fn accent_hover() -> Color { Color::from_rgb(0.38, 0.62, 1.0) }
    /// Success — emerald green
    pub fn success() -> Color { Color::from_rgb(0.25, 0.78, 0.45) }
    /// Warning — amber
    pub fn warning() -> Color { Color::from_rgb(0.92, 0.70, 0.20) }
    /// Error — coral red
    pub fn error() -> Color { Color::from_rgb(0.90, 0.32, 0.32) }
    /// Thinking / streaming — soft violet
    pub fn thinking() -> Color { Color::from_rgb(0.58, 0.48, 0.95) }
    /// Tool running — warm orange
    pub fn tool_active() -> Color { Color::from_rgb(0.95, 0.60, 0.22) }

    // ── Borders ─────────────────────────────────────────────────────
    pub fn border_subtle() -> Color { Color::from_rgb(0.14, 0.14, 0.18) }
    pub fn border_default() -> Color { Color::from_rgb(0.18, 0.18, 0.23) }
    pub fn border_focus() -> Color { Color::from_rgb(0.30, 0.45, 0.80) }

    // ── Settings panel ──────────────────────────────────────────────
    pub fn card_bg() -> Color { Color::from_rgb(0.12, 0.13, 0.17) }
    pub fn card_border() -> Color { Color::from_rgb(0.18, 0.19, 0.24) }

    // ── Plan mode ───────────────────────────────────────────────────
    pub fn plan_bg() -> Color { Color::from_rgb(0.20, 0.17, 0.05) }
    pub fn plan_fg() -> Color { Color::from_rgb(0.15, 0.10, 0.0) }
    pub fn plan_accent() -> Color { Color::from_rgb(0.95, 0.75, 0.20) }
    pub fn plan_accent_hover() -> Color { Color::from_rgb(1.0, 0.85, 0.30) }

    // ── Session sidebar ─────────────────────────────────────────────
    pub fn sidebar_item_hover() -> Color { Color::from_rgb(0.12, 0.12, 0.16) }
    pub fn sidebar_item_selected() -> Color { Color::from_rgb(0.13, 0.13, 0.18) }
    pub fn sidebar_item_selected_border() -> Color { Color::from_rgb(0.22, 0.24, 0.30) }

    pub fn iced_theme() -> iced::Theme { iced::Theme::Dark }
}
