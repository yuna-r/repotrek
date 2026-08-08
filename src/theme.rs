use clap::ValueEnum;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    #[must_use]
    pub const fn palette(self) -> Theme {
        match self {
            Self::Dark => Theme::dark(),
            Self::Light => Theme::light(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub selection: Color,
    pub cursor: Color,
    pub success: Color,
    pub danger: Color,
    pub warning: Color,
    pub diff_add_bg: Color,
    pub diff_delete_bg: Color,
    pub diff_hunk_bg: Color,
    pub link: Color,
    pub keyword: Color,
    pub type_name: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub function: Color,
    pub constant: Color,
    pub punctuation: Color,
}

impl Theme {
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            background: Color::Rgb(13, 17, 23),
            surface: Color::Rgb(22, 27, 34),
            surface_alt: Color::Rgb(33, 38, 45),
            text: Color::Rgb(230, 237, 243),
            muted: Color::Rgb(139, 148, 158),
            border: Color::Rgb(48, 54, 61),
            accent: Color::Rgb(88, 166, 255),
            accent_text: Color::Rgb(13, 17, 23),
            selection: Color::Rgb(42, 85, 128),
            cursor: Color::Rgb(31, 64, 96),
            success: Color::Rgb(63, 185, 80),
            danger: Color::Rgb(248, 81, 73),
            warning: Color::Rgb(210, 153, 34),
            diff_add_bg: Color::Rgb(3, 48, 20),
            diff_delete_bg: Color::Rgb(64, 18, 24),
            diff_hunk_bg: Color::Rgb(22, 27, 34),
            link: Color::Rgb(88, 166, 255),
            keyword: Color::Rgb(255, 123, 114),
            type_name: Color::Rgb(121, 192, 255),
            string: Color::Rgb(165, 214, 255),
            number: Color::Rgb(121, 192, 255),
            comment: Color::Rgb(139, 148, 158),
            function: Color::Rgb(210, 168, 255),
            constant: Color::Rgb(255, 166, 87),
            punctuation: Color::Rgb(201, 209, 217),
        }
    }

    #[must_use]
    pub const fn light() -> Self {
        Self {
            background: Color::Rgb(255, 255, 255),
            surface: Color::Rgb(246, 248, 250),
            surface_alt: Color::Rgb(234, 238, 242),
            text: Color::Rgb(31, 35, 40),
            muted: Color::Rgb(101, 109, 118),
            border: Color::Rgb(208, 215, 222),
            accent: Color::Rgb(9, 105, 218),
            accent_text: Color::Rgb(255, 255, 255),
            selection: Color::Rgb(166, 211, 255),
            cursor: Color::Rgb(210, 232, 255),
            success: Color::Rgb(26, 127, 55),
            danger: Color::Rgb(207, 34, 46),
            warning: Color::Rgb(154, 103, 0),
            diff_add_bg: Color::Rgb(218, 251, 225),
            diff_delete_bg: Color::Rgb(255, 235, 233),
            diff_hunk_bg: Color::Rgb(221, 244, 255),
            link: Color::Rgb(9, 105, 218),
            keyword: Color::Rgb(207, 34, 46),
            type_name: Color::Rgb(130, 80, 223),
            string: Color::Rgb(10, 48, 105),
            number: Color::Rgb(5, 80, 174),
            comment: Color::Rgb(110, 119, 129),
            function: Color::Rgb(102, 57, 186),
            constant: Color::Rgb(149, 56, 0),
            punctuation: Color::Rgb(31, 35, 40),
        }
    }
}
