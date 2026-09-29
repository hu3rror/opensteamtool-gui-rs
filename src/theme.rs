//! 语义色板（ADR-0010）：全仓库唯一的色值来源；hover / 徽章浅底 / selection 由派生函数产出，不写死新色值。

use eframe::egui::Color32;

pub const ACCENT: Color32 = Color32::from_rgb(0x0F, 0x6C, 0xBD);
pub const SUCCESS: Color32 = Color32::from_rgb(0x16, 0xA3, 0x4A);
pub const WARN: Color32 = Color32::from_rgb(0xB4, 0x53, 0x09);
pub const DANGER: Color32 = Color32::from_rgb(0xDC, 0x26, 0x26);
pub const INK: Color32 = Color32::from_rgb(0x0F, 0x17, 0x2A);
pub const SUB: Color32 = Color32::from_rgb(0x33, 0x41, 0x55);
pub const WEAK: Color32 = Color32::from_rgb(0x64, 0x74, 0x8B);
pub const PANEL: Color32 = Color32::from_rgb(0xF8, 0xF9, 0xFA);
pub const CARD: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// 反色白（深底按钮文字；与卡片底同值不同槽）。
pub const WHITE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const BORDER: Color32 = Color32::from_rgb(0xE2, 0xE8, 0xF0);
pub const ENTRY: Color32 = Color32::from_rgb(0xCB, 0xD5, 0xE1);

pub const CAUTION_BG: Color32 = Color32::from_rgb(0xF0, 0xF9, 0xFF);
pub const CAUTION_HOVER: Color32 = Color32::from_rgb(0xE0, 0xF2, 0xFE);
pub const CAUTION_FG: Color32 = Color32::from_rgb(0x02, 0x84, 0xC7);
pub const CAUTION_BORDER: Color32 = Color32::from_rgb(0x7D, 0xD3, 0xFC);

const SOLID_HOVER: f32 = 0.92;
const NEUTRAL_HOVER: f32 = 0.9;

/// 暗化：`f` 为保留比例（1.0 不变）；直接截断——与 `blend` 的四舍五入不同（金样测试锁定）。
pub fn darken(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 * f) as u8,
        (c.g() as f32 * f) as u8,
        (c.b() as f32 * f) as u8,
    )
}

/// 线性混合：`t=0` 全 a，`t=1` 全 b（逐通道四舍五入）。
pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn badge_bg(base: Color32) -> Color32 {
    blend(base, WHITE, 0.9)
}

pub fn selection_bg() -> Color32 {
    blend(ACCENT, WHITE, 0.82)
}

pub fn accent_hover() -> Color32 {
    darken(ACCENT, SOLID_HOVER)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonPalette {
    pub bg: Color32,
    pub hover: Color32,
    pub fg: Color32,
    pub border: Option<Color32>,
}

/// 按钮样式语义名（对齐操作语义；新增样式只加变体，不新增色值）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonStyle {
    Deploy,
    Primary,
    Caution,
    Neutral,
}

impl ButtonStyle {
    pub fn palette(self) -> ButtonPalette {
        match self {
            ButtonStyle::Deploy => ButtonPalette {
                bg: SUCCESS,
                hover: darken(SUCCESS, SOLID_HOVER),
                fg: WHITE,
                border: None,
            },
            ButtonStyle::Primary => ButtonPalette {
                bg: ACCENT,
                hover: accent_hover(),
                fg: WHITE,
                border: None,
            },
            ButtonStyle::Caution => ButtonPalette {
                bg: CAUTION_BG,
                hover: CAUTION_HOVER,
                fg: CAUTION_FG,
                border: Some(CAUTION_BORDER),
            },
            ButtonStyle::Neutral => ButtonPalette {
                bg: CARD,
                hover: darken(CARD, NEUTRAL_HOVER),
                fg: SUB,
                border: Some(ENTRY),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOTS: [Color32; 12] = [
        ACCENT, SUCCESS, WARN, DANGER, INK, SUB, WEAK, PANEL, CARD, WHITE, BORDER, ENTRY,
    ];
    const CAUTION_GROUP: [Color32; 4] = [CAUTION_BG, CAUTION_HOVER, CAUTION_FG, CAUTION_BORDER];

    #[test]
    fn darken_golden_values() {
        assert_eq!(darken(Color32::WHITE, 1.0), Color32::WHITE);
        assert_eq!(
            darken(Color32::WHITE, 0.9),
            Color32::from_rgb(229, 229, 229)
        );
        assert_eq!(darken(SUCCESS, 0.92), Color32::from_rgb(0x14, 0x95, 0x44));
        assert_eq!(darken(ACCENT, 0.92), Color32::from_rgb(0x0D, 0x63, 0xAD));
        assert_eq!(darken(ACCENT, 0.0), Color32::from_rgb(0, 0, 0));
    }

    #[test]
    fn blend_golden_values() {
        assert_eq!(blend(SUCCESS, WHITE, 0.0), SUCCESS);
        assert_eq!(blend(SUCCESS, WHITE, 1.0), WHITE);
        assert_eq!(blend(SUCCESS, WHITE, 0.9), Color32::from_rgb(232, 246, 237));
        assert_eq!(blend(ACCENT, WHITE, 0.82), Color32::from_rgb(212, 229, 243));
    }

    #[test]
    fn derived_helpers_golden_values() {
        assert_eq!(badge_bg(SUCCESS), Color32::from_rgb(232, 246, 237));
        assert_eq!(selection_bg(), Color32::from_rgb(212, 229, 243));
    }

    #[test]
    fn button_palette_full_table() {
        let deploy = ButtonStyle::Deploy.palette();
        assert_eq!(deploy.bg, Color32::from_rgb(0x16, 0xA3, 0x4A));
        assert_eq!(deploy.hover, Color32::from_rgb(0x14, 0x95, 0x44));
        assert_eq!(deploy.fg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(deploy.border, None);

        let primary = ButtonStyle::Primary.palette();
        assert_eq!(primary.bg, Color32::from_rgb(0x0F, 0x6C, 0xBD));
        assert_eq!(primary.hover, Color32::from_rgb(0x0D, 0x63, 0xAD));
        assert_eq!(primary.fg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(primary.border, None);

        let caution = ButtonStyle::Caution.palette();
        assert_eq!(caution.bg, Color32::from_rgb(0xF0, 0xF9, 0xFF));
        assert_eq!(caution.hover, Color32::from_rgb(0xE0, 0xF2, 0xFE));
        assert_eq!(caution.fg, Color32::from_rgb(0x02, 0x84, 0xC7));
        assert_eq!(caution.border, Some(Color32::from_rgb(0x7D, 0xD3, 0xFC)));

        let neutral = ButtonStyle::Neutral.palette();
        assert_eq!(neutral.bg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(neutral.hover, Color32::from_rgb(0xE5, 0xE5, 0xE5));
        assert_eq!(neutral.fg, Color32::from_rgb(0x33, 0x41, 0x55));
        assert_eq!(neutral.border, Some(Color32::from_rgb(0xCB, 0xD5, 0xE1)));
    }

    #[test]
    fn palette_has_exactly_twelve_slots() {
        assert_eq!(SLOTS.len(), 12);
        let mut distinct = SLOTS.to_vec();
        distinct.sort_by_key(|c| (c.r(), c.g(), c.b()));
        distinct.dedup();
        assert_eq!(distinct.len(), 11, "仅 card/white 允许同值不同槽");
    }

    #[test]
    fn derived_colors_do_not_occupy_slots() {
        let no_dup = |c: Color32| !SLOTS.contains(&c) && !CAUTION_GROUP.contains(&c);
        for style in [
            ButtonStyle::Deploy,
            ButtonStyle::Primary,
            ButtonStyle::Neutral,
        ] {
            let p = style.palette();
            assert!(no_dup(p.hover), "{style:?} hover 占用了槽/警戒色");
            assert_ne!(p.hover, p.bg, "{style:?} hover 与 bg 相同");
        }
        let caution = ButtonStyle::Caution.palette();
        assert_ne!(caution.hover, caution.bg);
        assert!(no_dup(badge_bg(SUCCESS)) && no_dup(badge_bg(WARN)) && no_dup(badge_bg(DANGER)));
        assert!(no_dup(selection_bg()));
    }

    #[test]
    fn no_inline_color_literals_outside_theme() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&src_dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("theme.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source");
            for (i, line) in text.lines().enumerate() {
                if line.contains("Color32::from_") || line.contains("Color32::WHITE") {
                    offenders.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "theme.rs 之外发现内联色值：\n{}",
            offenders.join("\n")
        );
    }
}
