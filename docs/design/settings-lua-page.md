# OpenSteamTool Manager

## Settings — Lua Config Page Design Reference

**Target:** Rust + egui / Windows Native Desktop GUI
**Scope:** Settings dialog — Lua Config tab
**Languages:** `zh` / `en`
**Theme:** Dark / Light（随全局主题，ADR-0016 语义槽）

---

# 1. 定位

「Lua 配置」页签是设置对话框的第四个页签（通用 / Steam / Lua 配置 / 关于），承载 Lua
功能域的说明与操作入口。本次只提供「打开目录」操作与说明文案，为未来 Lua 配置清单的
浏览/管理功能预留结构（见 §5），不呈现为 Mod Manager。

术语对齐上游 OpenSteamTool（README_ZH 原句）：目录称「Lua 目录」、文件称
「.lua 文件 / Lua 脚本」、功能域称「Lua 配置」（GLOSSARY「Lua 配置」）。避免自创词
（「Lua 清单」已被更名，入 Avoid）。

# 2. 页签栏

四页签按钮（88×28，Primary/Neutral 切换，间距 4lp）：通用 / Steam / Lua 配置 / 关于。
「Lua 配置」置于 Steam 之后、关于之前——它依赖 Steam 路径（按钮可用性），与 Steam
页签相邻。

# 3. 页签内容（三段式）

自上而下：

1. **标题**：`card_title`（13.5 strong，ink 色；左侧 10×2 竖条用 border 色，与其余
   页签同款）。文案 = 页签标签「Lua 配置」/ "Lua Config"（共用 i18n 键
   `settings_tab_lua`）。标题下 10lp。
2. **说明**：三行，12.5 / `palette.sub`，行距 4lp，超宽自动换行（垂直布局默认）；布局测试
   钉死说明行右缘不溢出内容右界：
   - 声明解锁：`addappid` / `addtoken` / `setManifestid` 声明要解锁的 App 与 Depot
   - 热重载：添加、修改、删除或覆盖 `.lua` 文件自动热重载，无需重启 Steam
   - 放置位置：将下载的 `.lua` 文件放入 `<Steam>/config/lua` 目录
   三行文案逐条对齐上游 README_ZH 原句意译（核心解锁 / 热重载 / Lua Sources）。
   说明块下 8lp。
3. **操作**：左对齐「打开 Lua 目录」/ "Open Lua Folder" 图标按钮
   （ActionRow 层：40 高、图标 15、inset 12、label_x 39；Folder 图标；Neutral
   chrome，禁用态文字与图标用 `palette.weak`）。宽度 = 39 + 文案宽 + 12。
   可用性谓词与托盘「重启 Steam」「打开 Lua 目录」同族：Steam 路径有效即可点
   （`path_actions_enabled`）。

# 4. 交互

- 点击「打开 Lua 目录」：Steam 路径有效 → `dll::ensure_lua_dir` 创建
  `<Steam>/config/lua/`（缺失时，与部署同行为）→ 壳侧 `cmd /C start "" ""` 打开
  资源管理器（`updater::open_path`）。失败仅记日志（ADR 日志级契约），不弹错误
  对话框。无新 AppEvent / AppEffect（ADR-0019）。
- 页签切换：切出 Steam 页签时先提交路径编辑（既有行为），Lua 配置页签无编辑态。

# 5. 未来扩展位（预留，不渲染）

未来的 Lua 配置浏览/管理（枚举 `.lua` 文件、增删、查看 `addappid` 声明）落点：
操作按钮与页签底部之间留出纵向扩展空间（ScrollArea 内自然增长）。本次不渲染任何
占位元素。扩展时沿用本页间距节奏：标题 10lp → 内容 4lp 行距 → 段间 8–16lp
（`settings_general` 小节间 16lp 先例）。

# 6. 视觉约束（ui-visual-polish 采纳部分）

本项目不引入 Radix 色板（主题语义槽由 ADR-0016 定稿，本页只消费 `Palette` 语义槽）。
采纳其余惯例：

- ≤3 字号（13.5 标题 / 12.5 说明 / 按钮 13.5），不低于 12px。
- 层级靠字重、颜色（ink / sub / weak）与间距建立，不增加字号档。
- 无装饰左缘线（`card_title` 的 10×2 竖条是既有小节头统一样式，本页沿用；不新增
  其它装饰线）、无「·」分隔符、无过渡动画。
- 按钮圆角 8（既有 `paint_button_chrome` 值），克制统一。

# 7. 验收（#60）

- 布局宽度测试 `settings_lua_config_tab_fits_dialog_width`：`SETTINGS_DIALOG_WIDTH`
  下 zh/en 双语言零溢出（说明行与按钮右缘均不超内容右界）。
- 四页签行不溢出：`settings_tab_row_fits_dialog_width` 含第四个页签标签。
- 托盘菜单项更名「打开 Lua 目录」（位置不变）：人工冒烟 `[NEED-VERIFY]`。
- 门禁：`cargo test --workspace` / `check` / `clippy` / `fmt --all --check`。
