//! 语义色板（ADR-0010 / ADR-0016）：全仓库唯一的色值来源；hover / 徽章浅底 / selection 由 Palette 派生，
//! 不写死新色值。`Palette::dark()` = Dark（B）定稿；`Palette::light()` = L1 — Ice Mist 定稿（spec §10）。

use eframe::egui::Color32;

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

/// 警戒组（ADR-0010 唯一写死例外）：Warning Secondary Blue 家族，仅「退出 Steam 并卸载补丁」。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CautionPalette {
    pub bg: Color32,
    pub hover: Color32,
    pub fg: Color32,
    pub border: Color32,
}

/// 语义色板：12 语义槽 + 反色白 + 警戒组；`dark()` / `light()` 为唯二构造点（spec §48.2 / ADR-0016）。
/// 派生色不占槽：accent_hover 在 Dark 下为派生值、在 L1 下为定稿槽（spec §10，非 darken 可派生）；
/// selection / warn_fg / busy_ink / health 行底色按模式派生。UI 层只消费实例字段与方法，不写内联色值。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub(crate) dark_mode: bool,
    pub accent: Color32,
    /// Dark：`darken(accent, 0.92)` 派生；Light：定稿值（spec §10 表，偏色不可派生）。
    pub accent_hover: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub danger: Color32,
    /// 主文字（Dark 下为浅色、L1 下为深色，见 spec §10 Text Primary）。
    pub ink: Color32,
    pub sub: Color32,
    pub weak: Color32,
    /// 窗口页面底（Background）。
    pub panel: Color32,
    /// 卡片底（Surface）。
    pub card: Color32,
    /// 反色白（深底按钮文字）；L1 下与 card 同值（spec Surface = #FFFFFF，语义不同槽）。
    pub white: Color32,
    pub border: Color32,
    /// 控件层次/描边（Surface Elevated 对偶：Dark 比 hairline 暗一档、L1 为浅灰蓝底，spec §10）。
    pub entry: Color32,
    pub caution: CautionPalette,
}

impl Palette {
    /// Dark（B）定稿（spec §10 Deep Navy / Ice Blue）。
    pub fn dark() -> Self {
        Self {
            dark_mode: true,
            accent: Color32::from_rgb(0x5C, 0x91, 0xC7),
            accent_hover: darken(Color32::from_rgb(0x5C, 0x91, 0xC7), SOLID_HOVER),
            success: Color32::from_rgb(0x6B, 0xA8, 0x8F),
            warn: Color32::from_rgb(0xC3, 0x9A, 0x5B),
            danger: Color32::from_rgb(0xC9, 0x79, 0x79),
            ink: Color32::from_rgb(0xE8, 0xED, 0xF3),
            sub: Color32::from_rgb(0xA9, 0xB5, 0xC3),
            weak: Color32::from_rgb(0x77, 0x85, 0x96),
            panel: Color32::from_rgb(0x0F, 0x15, 0x1C),
            card: Color32::from_rgb(0x15, 0x1E, 0x28),
            white: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            border: Color32::from_rgb(0x29, 0x37, 0x46),
            entry: Color32::from_rgb(0x1C, 0x28, 0x35),
            caution: CautionPalette {
                bg: Color32::from_rgb(0x1A, 0x2A, 0x3D),
                hover: Color32::from_rgb(0x23, 0x37, 0x4F),
                fg: Color32::from_rgb(0x58, 0x7A, 0x9D),
                border: Color32::from_rgb(0x4A, 0x6E, 0x93),
            },
        }
    }

    /// Light（L1 — Ice Mist）定稿（spec §10）；警戒组与对比度敏感派生取 L1 原型经验值（ADR-0016）。
    pub fn light() -> Self {
        Self {
            dark_mode: false,
            accent: Color32::from_rgb(0x3E, 0x76, 0xAC),
            accent_hover: Color32::from_rgb(0x35, 0x68, 0x9A),
            success: Color32::from_rgb(0x3F, 0x8E, 0x63),
            warn: Color32::from_rgb(0xA6, 0x7C, 0x2E),
            danger: Color32::from_rgb(0xC2, 0x55, 0x4E),
            ink: Color32::from_rgb(0x1C, 0x26, 0x30),
            sub: Color32::from_rgb(0x47, 0x56, 0x6A),
            weak: Color32::from_rgb(0x84, 0x94, 0xA7),
            panel: Color32::from_rgb(0xF3, 0xF6, 0xF9),
            card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            white: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            border: Color32::from_rgb(0xD3, 0xDC, 0xE6),
            entry: Color32::from_rgb(0xE8, 0xEE, 0xF5),
            caution: CautionPalette {
                bg: blend(Color32::from_rgb(0x3E, 0x6F, 0x9E), Color32::WHITE, 0.85),
                hover: blend(Color32::from_rgb(0x3E, 0x6F, 0x9E), Color32::WHITE, 0.8),
                fg: Color32::from_rgb(0x3E, 0x6F, 0x9E),
                border: Color32::from_rgb(0x3E, 0x6F, 0x9E),
            },
        }
    }

    /// 徽章浅底：向页面背景混（Dark 向 PANEL、L1 向 WHITE，spec §10 派生说明）。
    pub fn badge_bg(self, base: Color32) -> Color32 {
        blend(
            base,
            if self.dark_mode {
                self.panel
            } else {
                self.white
            },
            0.85,
        )
    }

    pub fn selection_bg(self) -> Color32 {
        blend(self.accent, self.white, 0.82)
    }

    /// Health Warning 行前景：Dark 向 WHITE 混调亮（现状值）；L1 向 BLACK 混调深（原型 `--warn-fg` 经验值）。
    pub fn warn_fg(self) -> Color32 {
        if self.dark_mode {
            blend(self.warn, self.white, 0.2)
        } else {
            blend(self.warn, Color32::BLACK, 0.45)
        }
    }

    /// 状态栏忙碌文字：Dark 用 accent 纯色（现状值）；L1 向 BLACK 混调深（原型 `--busy-ink` 经验值）。
    pub fn busy_ink(self) -> Color32 {
        if self.dark_mode {
            self.accent
        } else {
            blend(self.accent, Color32::BLACK, 0.55)
        }
    }

    /// Health Warning 行底色（Dark 向 PANEL、L1 向 WHITE 混出浅琥珀底）。
    pub fn health_warning_bg(self) -> Color32 {
        blend(
            self.warn,
            if self.dark_mode {
                self.panel
            } else {
                self.white
            },
            0.86,
        )
    }

    /// Health Warning 行描边。
    pub fn health_warning_border(self) -> Color32 {
        blend(
            self.warn,
            if self.dark_mode {
                self.panel
            } else {
                self.white
            },
            0.6,
        )
    }
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
    pub fn palette(self, p: Palette) -> ButtonPalette {
        match self {
            ButtonStyle::Primary => ButtonPalette {
                bg: p.accent,
                hover: p.accent_hover,
                fg: p.white,
                border: None,
            },
            ButtonStyle::Caution => ButtonPalette {
                bg: p.caution.bg,
                hover: p.caution.hover,
                fg: p.caution.fg,
                border: Some(p.caution.border),
            },
            ButtonStyle::Neutral => ButtonPalette {
                bg: p.card,
                hover: darken(p.card, NEUTRAL_HOVER),
                fg: p.sub,
                border: Some(p.entry),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(p: Palette) -> [Color32; 12] {
        [
            p.accent, p.success, p.warn, p.danger, p.ink, p.sub, p.weak, p.panel, p.card, p.white,
            p.border, p.entry,
        ]
    }

    fn caution_slots(p: Palette) -> [Color32; 4] {
        [
            p.caution.bg,
            p.caution.hover,
            p.caution.fg,
            p.caution.border,
        ]
    }

    #[test]
    fn darken_golden_values() {
        assert_eq!(darken(Color32::WHITE, 1.0), Color32::WHITE);
        assert_eq!(
            darken(Color32::WHITE, 0.9),
            Color32::from_rgb(229, 229, 229)
        );
        assert_eq!(
            darken(Color32::from_rgb(0x6B, 0xA8, 0x8F), 0.92),
            Color32::from_rgb(0x62, 0x9A, 0x83)
        );
        assert_eq!(
            darken(Color32::from_rgb(0x5C, 0x91, 0xC7), 0.92),
            Color32::from_rgb(0x54, 0x85, 0xB7)
        );
        assert_eq!(
            darken(Color32::from_rgb(0x5C, 0x91, 0xC7), 0.0),
            Color32::from_rgb(0, 0, 0)
        );
    }

    #[test]
    fn blend_golden_values() {
        assert_eq!(
            blend(Color32::from_rgb(0x6B, 0xA8, 0x8F), Color32::WHITE, 0.0),
            Color32::from_rgb(0x6B, 0xA8, 0x8F)
        );
        assert_eq!(
            blend(Color32::from_rgb(0x6B, 0xA8, 0x8F), Color32::WHITE, 1.0),
            Color32::WHITE
        );
        assert_eq!(
            blend(Color32::from_rgb(0x6B, 0xA8, 0x8F), Color32::WHITE, 0.9),
            Color32::from_rgb(0xF0, 0xF6, 0xF4)
        );
        assert_eq!(
            blend(Color32::from_rgb(0x5C, 0x91, 0xC7), Color32::WHITE, 0.82),
            Color32::from_rgb(0xE2, 0xEB, 0xF5)
        );
    }

    #[test]
    fn dark_derived_helpers_golden_values() {
        let p = Palette::dark();
        assert_eq!(p.badge_bg(p.success), Color32::from_rgb(0x1D, 0x2B, 0x2D));
        assert_eq!(p.selection_bg(), Color32::from_rgb(0xE2, 0xEB, 0xF5));
        assert_eq!(p.accent_hover, Color32::from_rgb(0x54, 0x85, 0xB7));
        assert_eq!(p.warn_fg(), Color32::from_rgb(0xCF, 0xAE, 0x7C));
        assert_eq!(p.busy_ink(), Color32::from_rgb(0x5C, 0x91, 0xC7));
        assert_eq!(p.health_warning_bg(), Color32::from_rgb(0x28, 0x28, 0x25));
        assert_eq!(
            p.health_warning_border(),
            Color32::from_rgb(0x57, 0x4A, 0x35)
        );
    }

    #[test]
    fn light_slot_golden_values() {
        let p = Palette::light();
        assert_eq!(p.accent, Color32::from_rgb(0x3E, 0x76, 0xAC));
        assert_eq!(p.success, Color32::from_rgb(0x3F, 0x8E, 0x63));
        assert_eq!(p.warn, Color32::from_rgb(0xA6, 0x7C, 0x2E));
        assert_eq!(p.danger, Color32::from_rgb(0xC2, 0x55, 0x4E));
        assert_eq!(p.ink, Color32::from_rgb(0x1C, 0x26, 0x30));
        assert_eq!(p.sub, Color32::from_rgb(0x47, 0x56, 0x6A));
        assert_eq!(p.weak, Color32::from_rgb(0x84, 0x94, 0xA7));
        assert_eq!(p.panel, Color32::from_rgb(0xF3, 0xF6, 0xF9));
        assert_eq!(p.card, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(p.border, Color32::from_rgb(0xD3, 0xDC, 0xE6));
        assert_eq!(p.entry, Color32::from_rgb(0xE8, 0xEE, 0xF5));
    }

    #[test]
    fn light_accent_hover_is_locked_slot_not_derived() {
        let p = Palette::light();
        assert_eq!(p.accent_hover, Color32::from_rgb(0x35, 0x68, 0x9A));
        assert_ne!(
            p.accent_hover,
            darken(p.accent, SOLID_HOVER),
            "L1 hover 是定稿值，非 darken 派生（spec §10 表偏色）"
        );
    }

    #[test]
    fn light_derived_golden_values() {
        let p = Palette::light();
        assert_eq!(p.badge_bg(p.success), Color32::from_rgb(0xE2, 0xEE, 0xE8));
        assert_eq!(p.selection_bg(), Color32::from_rgb(0xDC, 0xE6, 0xF0));
        assert_eq!(p.warn_fg(), Color32::from_rgb(0x5B, 0x44, 0x19));
        assert_eq!(p.busy_ink(), Color32::from_rgb(0x1C, 0x35, 0x4D));
        assert_eq!(p.health_warning_bg(), Color32::from_rgb(0xF3, 0xED, 0xE2));
        assert_eq!(
            p.health_warning_border(),
            Color32::from_rgb(0xDB, 0xCB, 0xAB)
        );
    }

    #[test]
    fn light_caution_group_derived() {
        let p = Palette::light();
        assert_eq!(p.caution.bg, Color32::from_rgb(0xE2, 0xE9, 0xF0));
        assert_eq!(p.caution.hover, Color32::from_rgb(0xD8, 0xE2, 0xEC));
        assert_eq!(p.caution.fg, Color32::from_rgb(0x3E, 0x6F, 0x9E));
        assert_eq!(p.caution.border, Color32::from_rgb(0x3E, 0x6F, 0x9E));
    }

    #[test]
    fn button_palette_full_table() {
        let p = Palette::dark();
        let primary = ButtonStyle::Primary.palette(p);
        assert_eq!(primary.bg, Color32::from_rgb(0x5C, 0x91, 0xC7));
        assert_eq!(primary.hover, Color32::from_rgb(0x54, 0x85, 0xB7));
        assert_eq!(primary.fg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(primary.border, None);

        let caution = ButtonStyle::Caution.palette(p);
        assert_eq!(caution.bg, Color32::from_rgb(0x1A, 0x2A, 0x3D));
        assert_eq!(caution.hover, Color32::from_rgb(0x23, 0x37, 0x4F));
        assert_eq!(caution.fg, Color32::from_rgb(0x58, 0x7A, 0x9D));
        assert_eq!(caution.border, Some(Color32::from_rgb(0x4A, 0x6E, 0x93)));

        let neutral = ButtonStyle::Neutral.palette(p);
        assert_eq!(neutral.bg, Color32::from_rgb(0x15, 0x1E, 0x28));
        assert_eq!(neutral.hover, Color32::from_rgb(0x12, 0x1B, 0x24));
        assert_eq!(neutral.fg, Color32::from_rgb(0xA9, 0xB5, 0xC3));
        assert_eq!(neutral.border, Some(Color32::from_rgb(0x1C, 0x28, 0x35)));
    }

    #[test]
    fn button_palette_light_table() {
        let p = Palette::light();
        let primary = ButtonStyle::Primary.palette(p);
        assert_eq!(primary.bg, Color32::from_rgb(0x3E, 0x76, 0xAC));
        assert_eq!(primary.hover, Color32::from_rgb(0x35, 0x68, 0x9A));
        assert_eq!(primary.fg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(primary.border, None);

        let caution = ButtonStyle::Caution.palette(p);
        assert_eq!(caution.bg, Color32::from_rgb(0xE2, 0xE9, 0xF0));
        assert_eq!(caution.hover, Color32::from_rgb(0xD8, 0xE2, 0xEC));
        assert_eq!(caution.fg, Color32::from_rgb(0x3E, 0x6F, 0x9E));
        assert_eq!(caution.border, Some(Color32::from_rgb(0x3E, 0x6F, 0x9E)));

        let neutral = ButtonStyle::Neutral.palette(p);
        assert_eq!(neutral.bg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(neutral.hover, Color32::from_rgb(0xE5, 0xE5, 0xE5));
        assert_eq!(neutral.fg, Color32::from_rgb(0x47, 0x56, 0x6A));
        assert_eq!(neutral.border, Some(Color32::from_rgb(0xE8, 0xEE, 0xF5)));
    }

    #[test]
    fn dark_palette_has_exactly_twelve_distinct_slots() {
        let mut distinct = slots(Palette::dark()).to_vec();
        distinct.sort_by_key(|c| (c.r(), c.g(), c.b()));
        distinct.dedup();
        assert_eq!(distinct.len(), 12, "Dark（B）色源 12 槽全异");
    }

    #[test]
    fn derived_colors_do_not_occupy_slots() {
        for p in [Palette::dark(), Palette::light()] {
            let slots = slots(p);
            let caution = caution_slots(p);
            let no_dup = |c: Color32| !slots.contains(&c) && !caution.contains(&c);
            for style in [ButtonStyle::Primary, ButtonStyle::Neutral] {
                let b = style.palette(p);
                assert!(no_dup(b.hover), "{style:?} hover 占用了槽/警戒色");
                assert_ne!(b.hover, b.bg, "{style:?} hover 与 bg 相同");
            }
            let caution_palette = ButtonStyle::Caution.palette(p);
            assert_ne!(caution_palette.hover, caution_palette.bg);
            assert!(
                no_dup(p.badge_bg(p.success))
                    && no_dup(p.badge_bg(p.warn))
                    && no_dup(p.badge_bg(p.danger))
            );
            assert!(no_dup(p.selection_bg()));
            assert!(no_dup(p.warn_fg()));
            // dark 的 busy 文字就是 accent 槽本身（现状行为），只有 L1 才是新派生色。
            if !p.dark_mode {
                assert!(no_dup(p.busy_ink()), "L1 busy 派生色不占槽");
            }
            assert!(no_dup(p.health_warning_bg()) && no_dup(p.health_warning_border()));
        }
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
