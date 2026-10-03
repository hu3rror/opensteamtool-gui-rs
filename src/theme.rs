//! 语义色板（ADR-0010）：全仓库唯一的色值来源；hover / 徽章浅底 / selection 由派生函数产出，不写死新色值。
//! 色值 = Dark（B）定稿（spec §10 Deep Navy / Ice Blue）；Light（L1）不进代码（spec §48.2）。

use eframe::egui::Color32;

pub const ACCENT: Color32 = Color32::from_rgb(0x5C, 0x91, 0xC7);
pub const SUCCESS: Color32 = Color32::from_rgb(0x6B, 0xA8, 0x8F);
pub const WARN: Color32 = Color32::from_rgb(0xC3, 0x9A, 0x5B);
pub const DANGER: Color32 = Color32::from_rgb(0xC9, 0x79, 0x79);
/// 主文字（Dark 下为浅色，见 spec §10 Text Primary）。
pub const INK: Color32 = Color32::from_rgb(0xE8, 0xED, 0xF3);
pub const SUB: Color32 = Color32::from_rgb(0xA9, 0xB5, 0xC3);
pub const WEAK: Color32 = Color32::from_rgb(0x77, 0x85, 0x96);
/// 窗口页面底（Background）。
pub const PANEL: Color32 = Color32::from_rgb(0x0F, 0x15, 0x1C);
pub const CARD: Color32 = Color32::from_rgb(0x15, 0x1E, 0x28);
/// 反色白（深底按钮文字）。
pub const WHITE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const BORDER: Color32 = Color32::from_rgb(0x29, 0x37, 0x46);
/// 控件层次/描边（Surface Elevated 的深色对偶：比 hairline 暗一档，spec §10）。
pub const ENTRY: Color32 = Color32::from_rgb(0x1C, 0x28, 0x35);

/// 警戒组（ADR-0010 唯一写死例外）：Warning Secondary Blue 家族，仅「退出 Steam 并卸载补丁」。
pub const CAUTION_BG: Color32 = Color32::from_rgb(0x1A, 0x2A, 0x3D);
pub const CAUTION_HOVER: Color32 = Color32::from_rgb(0x23, 0x37, 0x4F);
pub const CAUTION_FG: Color32 = Color32::from_rgb(0x58, 0x7A, 0x9D);
pub const CAUTION_BORDER: Color32 = Color32::from_rgb(0x4A, 0x6E, 0x93);

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

/// 按钮样式语义名（对齐操作语义；spec §18 起 Primary CTA 统一 Solid Brand Blue，Deploy 绿废止）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonStyle {
    Primary,
    /// ADR-0010 警戒组样式表契约保留（主页面现为 inline 警示行消费 CAUTION_FG；
    /// 按钮形态未在本次主页面使用，禁删以防 Deploy 绿/警戒按钮回归样式分裂）。
    #[allow(dead_code)]
    Caution,
    Neutral,
}

impl ButtonStyle {
    pub fn palette(self) -> ButtonPalette {
        match self {
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
        assert_eq!(darken(SUCCESS, 0.92), Color32::from_rgb(0x62, 0x9A, 0x83));
        assert_eq!(darken(ACCENT, 0.92), Color32::from_rgb(0x54, 0x85, 0xB7));
        assert_eq!(darken(ACCENT, 0.0), Color32::from_rgb(0, 0, 0));
    }

    #[test]
    fn blend_golden_values() {
        assert_eq!(blend(SUCCESS, WHITE, 0.0), SUCCESS);
        assert_eq!(blend(SUCCESS, WHITE, 1.0), WHITE);
        assert_eq!(
            blend(SUCCESS, WHITE, 0.9),
            Color32::from_rgb(0xF0, 0xF6, 0xF4)
        );
        assert_eq!(
            blend(ACCENT, WHITE, 0.82),
            Color32::from_rgb(0xE2, 0xEB, 0xF5)
        );
    }

    #[test]
    fn derived_helpers_golden_values() {
        assert_eq!(badge_bg(SUCCESS), Color32::from_rgb(0xF0, 0xF6, 0xF4));
        assert_eq!(selection_bg(), Color32::from_rgb(0xE2, 0xEB, 0xF5));
    }

    #[test]
    fn button_palette_full_table() {
        let primary = ButtonStyle::Primary.palette();
        assert_eq!(primary.bg, Color32::from_rgb(0x5C, 0x91, 0xC7));
        assert_eq!(primary.hover, Color32::from_rgb(0x54, 0x85, 0xB7));
        assert_eq!(primary.fg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(primary.border, None);

        let caution = ButtonStyle::Caution.palette();
        assert_eq!(caution.bg, Color32::from_rgb(0x1A, 0x2A, 0x3D));
        assert_eq!(caution.hover, Color32::from_rgb(0x23, 0x37, 0x4F));
        assert_eq!(caution.fg, Color32::from_rgb(0x58, 0x7A, 0x9D));
        assert_eq!(caution.border, Some(Color32::from_rgb(0x4A, 0x6E, 0x93)));

        let neutral = ButtonStyle::Neutral.palette();
        assert_eq!(neutral.bg, Color32::from_rgb(0x15, 0x1E, 0x28));
        assert_eq!(neutral.hover, Color32::from_rgb(0x12, 0x1B, 0x24));
        assert_eq!(neutral.fg, Color32::from_rgb(0xA9, 0xB5, 0xC3));
        assert_eq!(neutral.border, Some(Color32::from_rgb(0x1C, 0x28, 0x35)));
    }

    #[test]
    fn palette_has_exactly_twelve_slots() {
        assert_eq!(SLOTS.len(), 12);
        let mut distinct = SLOTS.to_vec();
        distinct.sort_by_key(|c| (c.r(), c.g(), c.b()));
        distinct.dedup();
        assert_eq!(distinct.len(), 12, "Dark（B）色源 12 槽全异");
    }

    #[test]
    fn derived_colors_do_not_occupy_slots() {
        let no_dup = |c: Color32| !SLOTS.contains(&c) && !CAUTION_GROUP.contains(&c);
        for style in [ButtonStyle::Primary, ButtonStyle::Neutral] {
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
