//! 语义色板（ADR-0010）：全仓库唯一的色值来源。
//!
//! 全部颜色收敛为固定语义槽（12 槽 + 警戒组第二蓝例外）；hover / 徽章浅底 /
//! selection 一律由派生函数产出，不写死新色值。ui.rs 只引用本模块，不得出现
//! 内联色值字面量（有源码扫描测试守卫）。

use eframe::egui::Color32;

// ---------- 基础语义槽（12 个，11 个不同色值） ----------

/// 主蓝：主按钮底 / accent bar / 蓝字 / selection 描边。
pub const ACCENT: Color32 = Color32::from_rgb(0x0F, 0x6C, 0xBD);
/// 成功绿：Deploy 按钮底 / 成功状态文字与圆点。
pub const SUCCESS: Color32 = Color32::from_rgb(0x16, 0xA3, 0x4A);
/// 警告琥珀：上游已适配未缓存状态。
pub const WARN: Color32 = Color32::from_rgb(0xB4, 0x53, 0x09);
/// 错误红：错误文案 / 失败圆点。
pub const DANGER: Color32 = Color32::from_rgb(0xDC, 0x26, 0x26);
/// 正文墨色。
pub const INK: Color32 = Color32::from_rgb(0x0F, 0x17, 0x2A);
/// 次级文字。
pub const SUB: Color32 = Color32::from_rgb(0x33, 0x41, 0x55);
/// 弱化文字 / 禁用态 / 灰状态。
pub const WEAK: Color32 = Color32::from_rgb(0x64, 0x74, 0x8B);
/// 面板底（兼 TextEdit / hover 底；旧 FILL_SECONDARY 并入）。
pub const PANEL: Color32 = Color32::from_rgb(0xF8, 0xF9, 0xFA);
/// 卡片底。
pub const CARD: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// 反色白（深底按钮文字；与卡片底同值不同槽）。
pub const WHITE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// 卡片边框（兼灰徽章底）。
pub const BORDER: Color32 = Color32::from_rgb(0xE2, 0xE8, 0xF0);
/// 控件描边（输入框 / 次按钮）。
pub const ENTRY: Color32 = Color32::from_rgb(0xCB, 0xD5, 0xE1);

// ---------- 警戒组（唯一写死例外，仅「退出 Steam 并卸载补丁」使用） ----------

/// 警戒浅底。
pub const CAUTION_BG: Color32 = Color32::from_rgb(0xF0, 0xF9, 0xFF);
/// 警戒 hover 底。
pub const CAUTION_HOVER: Color32 = Color32::from_rgb(0xE0, 0xF2, 0xFE);
/// 警戒文字（第二蓝）。
pub const CAUTION_FG: Color32 = Color32::from_rgb(0x02, 0x84, 0xC7);
/// 警戒描边。
pub const CAUTION_BORDER: Color32 = Color32::from_rgb(0x7D, 0xD3, 0xFC);

// ---------- 派生函数 ----------

/// 实心按钮 hover 暗化比例（Neutral 为 0.9）。
const SOLID_HOVER: f32 = 0.92;
const NEUTRAL_HOVER: f32 = 0.9;

/// 暗化：`f` 为保留比例（1.0 不变，0.92 ≈ blend 黑 8%）。与 `blend` 的四舍五入不同，
/// 这里直接截断（原型视觉基准，金样测试锁定）。
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

/// 兼容性徽章浅底 = 语义色 10% + 白 90%（灰徽章直接用 `BORDER` 槽）。
pub fn badge_bg(base: Color32) -> Color32 {
    blend(base, WHITE, 0.9)
}

/// selection 底 = accent 18% + 白（派生，不占槽）。
pub fn selection_bg() -> Color32 {
    blend(ACCENT, WHITE, 0.82)
}

/// accent 的 hover 派生（Primary 按钮 hover / 控件按下态描边共用）。
pub fn accent_hover() -> Color32 {
    darken(ACCENT, SOLID_HOVER)
}

// ---------- 按钮样式（4 类，解析表即唯一配色来源） ----------

/// 按钮解析结果：精确底色 / hover / 文字色 / 描边。
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
    /// 绿实心：应用补丁并启动 Steam。
    Deploy,
    /// accent 蓝实心：下载并解压新版本 / 保存 / 确认。
    Primary,
    /// 天蓝描边：仅「退出 Steam 并卸载补丁」（警戒组唯一使用者）。
    Caution,
    /// 无填充中性（白底描边）：其余全部按钮。
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

    /// 定稿槽表（唯一依据）：基础语义槽全集 + 警戒组。新增语义色必须追加到
    /// 对应数组并同步 ADR-0010 槽表，否则长度/去重断言失败。
    const SLOTS: [Color32; 12] = [
        ACCENT, SUCCESS, WARN, DANGER, INK, SUB, WEAK, PANEL, CARD, WHITE, BORDER, ENTRY,
    ];
    const CAUTION_GROUP: [Color32; 4] = [CAUTION_BG, CAUTION_HOVER, CAUTION_FG, CAUTION_BORDER];

    /// 金样值：darken 保留比例（含边界 1.0 / 0.0）。
    #[test]
    fn darken_golden_values() {
        assert_eq!(darken(Color32::WHITE, 1.0), Color32::WHITE);
        assert_eq!(darken(Color32::WHITE, 0.9), Color32::from_rgb(229, 229, 229));
        assert_eq!(darken(SUCCESS, 0.92), Color32::from_rgb(0x14, 0x95, 0x44));
        assert_eq!(darken(ACCENT, 0.92), Color32::from_rgb(0x0D, 0x63, 0xAD));
        assert_eq!(darken(ACCENT, 0.0), Color32::from_rgb(0, 0, 0));
    }

    /// 金样值：blend 端点（0% / 100%）与典型混合。
    #[test]
    fn blend_golden_values() {
        assert_eq!(blend(SUCCESS, WHITE, 0.0), SUCCESS);
        assert_eq!(blend(SUCCESS, WHITE, 1.0), WHITE);
        assert_eq!(blend(SUCCESS, WHITE, 0.9), Color32::from_rgb(232, 246, 237));
        assert_eq!(blend(ACCENT, WHITE, 0.82), Color32::from_rgb(212, 229, 243));
    }

    /// 派生辅助（徽章浅底 / selection 底）金样值。
    #[test]
    fn derived_helpers_golden_values() {
        assert_eq!(badge_bg(SUCCESS), Color32::from_rgb(232, 246, 237));
        assert_eq!(selection_bg(), Color32::from_rgb(212, 229, 243));
    }

    /// 按钮样式解析全表：每个样式 → 精确 (bg, hover, fg, border)，直接编码定稿解析表。
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

    /// 色板不变量：基础槽恰 12 个；同值仅 card/white 一组（11 个不同色值）。
    #[test]
    fn palette_has_exactly_twelve_slots() {
        assert_eq!(SLOTS.len(), 12);
        let mut distinct = SLOTS.to_vec();
        distinct.sort_by_key(|c| (c.r(), c.g(), c.b()));
        distinct.dedup();
        assert_eq!(distinct.len(), 11, "仅 card/white 允许同值不同槽");
    }

    /// 色板不变量：派生色不占槽——各按钮 hover / 徽章浅底 / selection 底
    /// 均不同于任何基础槽与警戒组色值（Caution 的 hover 本身是写死例外组成员）。
    #[test]
    fn derived_colors_do_not_occupy_slots() {
        let no_dup = |c: Color32| !SLOTS.contains(&c) && !CAUTION_GROUP.contains(&c);
        for style in [ButtonStyle::Deploy, ButtonStyle::Primary, ButtonStyle::Neutral] {
            let p = style.palette();
            assert!(no_dup(p.hover), "{style:?} hover 占用了槽/警戒色");
            assert_ne!(p.hover, p.bg, "{style:?} hover 与 bg 相同");
        }
        let caution = ButtonStyle::Caution.palette();
        assert_ne!(caution.hover, caution.bg);
        assert!(no_dup(badge_bg(SUCCESS)) && no_dup(badge_bg(WARN)) && no_dup(badge_bg(DANGER)));
        assert!(no_dup(selection_bg()));
    }

    /// 色板不变量：theme.rs 之外不得出现内联色值构造——颜色只能来自本模块槽与派生。
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
                // 构造/常量引用即色值来源；纯类型用法（如签名里的 egui::Color32）不含这些标记。
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