# 深浅主题切换（跟随系统 / 手动三态）

#39 深浅色主题切换功能：设置 — 通用新增「主题」小节（三态下拉：跟随系统 / 深色 / 浅色），
即改即存、即时生效、重启保持；浅色主题使用已定稿 L1 — Ice Mist 色源（spec §10）。
「跟随系统」模式随 OS 深浅运行时自动切换，原生窗口标题栏联动。

> 取代 ADR-0010 的「切换状态」注（未实现）：本 ADR 生效后切换功能已实现，
> 语义槽结构体化为 `Palette { dark(), light() }`（spec §48.2 预留形态，本次落地）。

## 为什么做

- **深浅色是外观的二元维度**：Windows 用户深浅偏好参半；跟随系统是 Windows 11 应用
  的默认预期，手动三态与既有「语言偏好」（auto/zh/en）完全同构，交互心智一致。
- **浅色主题早已定稿**：spec §10 的 L1 — Ice Mist 色源与原型 L1 变体已定稿，只是
  §48.2 规定「本次只落 Dark、切换另行立项」——本 ADR 即该立项的落地。
- **能力零成本**：egui/eframe 0.36 原生提供 `ThemePreference`（System 为默认）、
  `Context::set_theme`、`Options::sync_window_theme`（默认开启，原生标题栏联动）；
  系统深浅由 winit 提供（`ShouldAppsUseDarkMode`，Win10 1809+，含高对比度兜底；
  检测失败时 winit 回退 Light）；`ThemeChanged` 事件支持运行时监听。egui 层
  `system_theme` 缺失时回退 `fallback_theme`（默认 Dark）。全部走既有依赖链，零新依赖。

## 决策

- **config 字段**：新增 `theme`（`"system" | "dark" | "light"`），枚举
  `ThemePreference`（与 `Language` 同构：`parse`/`as_str`，无 `effective`——系统深浅
  解析由 egui 承担，config 不复制检测逻辑）。解析字段级宽容（缺失/类型非法 → System），
  同 `minimize_to_tray` 先例，不 bump CONFIG_VERSION。
- **缺省 System 的升级行为**：老配置无 `theme` 字段 → 跟随系统。**浅色 OS 的老用户
  升级后首次启动会看到浅色主题**——这是有意的行为变化（现代默认），不是配置损坏；
  深色 OS 老用户无感。
- **Palette 结构体化**（spec §48.2）：`Palette { dark(), light() }` 唯二构造点；
  12 语义槽 + 反色白 + 警戒组 4 色全进结构体；`ButtonStyle::palette` 参数化。
- **L1 accent_hover 是定稿槽、非派生**：spec §10 定稿 `#35689A` 是人工偏色值，
  `darken(×0.92)` 派生得 `#396DA0`，两者不同——spec 内部「派生同 Dark 规则」与
  「定稿表」冲突，裁决定稿优先（spec/原型一致）。Dark hover 保持派生（金样不动）；
  不对称在 `Palette` 构造内部消化，UI 层无感。
- **对比度敏感项按模式派生**（原型 L1 经验值）：L1 health warning 前景混黑
  `blend(warn, BLACK, 0.45)`、busy 文字混黑 `blend(accent, BLACK, 0.55)`；Dark 保持
  现状（health fg 混白 0.2、busy = accent 纯色）。`warn_fg` / `busy_ink` /
  `health_warning_bg/border` / `badge_bg` / `selection_bg` 成为 Palette 方法。
- **L1 警戒组派生**：由定稿 Warning Blue `#3E6F9E` 派生（bg 向白混 0.85、hover 0.8、
  fg/border 原色）；具体值按原型 L1 渲染验收，Tunable 微调（金样锁定初值）。
- **双 style 装配**：`install_theme` 把两套 visuals（`build_visuals(Palette::dark/light)`）
  分别写入 `Options.dark_style/light_style`，spacing 走 `all_styles_mut` 统一；
  主题切换/弹出层恒用装配值，不落回 egui 默认。
- **运行时**：App 持有 `theme_pref`（config 镜像）+ `palette`（每帧从 `ctx.theme()`
  同步——System 模式 OS 切深浅由 egui 自动换 style，UI 引用点随之换色，无需自监听）；
  `set_theme` 为唯一写入点（镜像 + `ctx.set_theme` + `persist_config`）。
- **GitHub mark 双态**：`load_github_mark(ctx, dark)`——Dark 烤白、L1 保留黑 mark；
  palette 模式变化时重载（旧 TextureHandle drop 自动释放纹理）。
- **UI 引用点集中改写一次**（spec §48.2 所指的那次）：全仓 `theme::` 常量引用改为
  `self.palette.*`（impl App）或 palette 参数（自由渲染函数）；源码扫描守卫
  （UI 禁内联色值）继续生效。
- **主题不改布局**：切换只影响颜色，主页面间距/位置/字号一律不动（spec §48 第 7 条）。

## 否决的方案

- **只做手动两态**（跟随系统后置）：三态骨架一次到位成本低，且与语言三态同构；
  半套方案会留下二次迁移。
- **默认 dark（保守，维持现状升级）**：与「跟随系统是 modern default」冲突；语言
  auto 缺省先例表明跟随系统是既有预期。接受浅色 OS 老用户升级变浅色的行为差异。
- **L1 accent_hover 按派生规则**（订正 spec 用 `#396DA0`）：偏离定稿观感，且
  `#35689A` 非纯暗化可派生（偏色），强统一得不偿失。
- **引入新槽承载对比度派生**（`warn_fg`/`busy_ink` 进槽）：派生值随色源自动变化，
  进槽后需双份维护；保持「派生不占槽」不变量更稳。
- **自己监听 OS 深浅/注册表直读**：egui/winit 已封装（含高对比度兜底），
  重复实现违背零新依赖与复用纪律。

## 应用图标双态（追加：统一 LOGO，深/浅各一版）

统一品牌标识为同一徽标的深浅双版：`assets/logo.png`（深色版：深蓝底 + 橙红环 + 浅蓝 OST）
与 `assets/logo-light.png`（浅色版），alpha 掩码同构（同一图形仅换配色）。四处消费点
共用同一映射（见 GLOSSARY「应用图标」）：窗口标题栏 + 任务栏按钮、托盘、主页面左上角。

### 决策

- **`brand` 模块为唯一图标源**：`window_icon` / `tray_icon` / `logo_texture` 由主题
  `.dark_mode` 选版，替换原 `main.rs::load_icon` 与 `tray.rs::load_icon` 各自读 `app.ico`
  的重复实现（单一事实源纪律）。
- **主题切换即换版**：三处运行时图标挂在既有 palette 模式变化钩子（与 GitHub mark 共用
  `mark_dark` 判断）；System 模式 OS 切深浅自动跟随，无需新增监听。
- **exe 静态图标固定深色版**：资源管理器 / 快捷方式读 exe 内嵌资源，shell 层无法按主题
  变，只能一份；选深色版（与 `logo.png` 命名与配色一致）。`tools/generate-ico.ps1`
  从 `logo.png` 生成多尺寸 `app.ico`（16/24/32/48/64/128/256，PNG 压缩帧，Vista+），
  取代旧的单尺寸 256（小尺寸靠系统缩放变糊）。
- **初始图标时序**：`ViewportBuilder` 按 config 主题选版（`main.rs::initial_window_icon`，
  System 模式先用深色兜底）；运行中主题切换走 `ViewportCommand::Icon`。平台限制（实测）：
  窗口显示前的 Icon 命令不生效（App::new 期间发送被吞、标题栏沿用 ViewportBuilder 图标），
  故 App 在首帧 update（窗口已显示）再同步一次，覆盖 System 模式按系统深浅校正；托盘在
  `App::new` 内即按初始主题选版。
- **任务栏按钮图标固定深色版（决策，ADR 落定）**：egui 0.36 的 `ViewportCommand::Icon`
  只更新 winit `set_window_icon`（标题栏 Small 图标）；任务栏按钮（Big 图标）由
  `set_taskbar_icon` 控制、仅窗口创建时可设（egui 不映射）——运行中切换主题时任务栏
  按钮恒为 exe 内嵌资源图标（app.ico 深色版）。**接受此限制**（与 exe 静态图标同一
  深色版、自洽）：替代方案需原生 Windows 路径（hwnd + WM_SETICON ICON_BIG 更新大
  图标），成本与收益不成比例（任务栏按钮图标不随主题是 Windows 应用常态）。
- **主页面左上角**：header 30px 格内渲染 20px LOGO 图，替换原手绘 Play mark；行高不变，
  高度测量测试占位同步无需改动。LOGO 自带不透明圆角色块，不做 GitHub mark 式的
  抠底/烤白（原图即定稿）。

### 否决的方案

- **托盘/窗口只在启动时定一次、不做运行时切换**：System 模式 OS 切深浅时图标不跟随，
  与「深/浅主题分别对应不同 LOGO」的意图矛盾。
- **固定深色版不生成浅色版图标**：主页面浅色主题下 header 仍是深色色块，视觉割裂。
- **任务栏按钮固定深色版以换取浅色高亮块对比度**：egui 窗口图标与任务栏按钮一体，
  无法分开；跟随主题优先，浅色主题下按钮可见性偏低的风险由人工验收确认。

### 验收（追加）

- 深/浅主题下四处图标各为对应版本，主题切换（含 System 模式 OS 切换）即时换版。
- `app.ico` 为 7 尺寸多帧；主页面 header 行高与改动前一致（高度测量测试不变）。
- 人工验证项：浅色主题下任务栏按钮图标可见性、托盘 16px 清晰度、两版 LOGO 像素观感。

## 验收

- 设置 — 通用页签「主题」小节：三态下拉（跟随系统 / 深色 / 浅色），即改即存、
  即时生效、重启恢复；双语文案。
- System 模式：启动读 OS 深浅；运行中 OS 切换 UI 即时跟随；原生标题栏联动
  （`sync_window_theme` 默认开启）。
- 浅色主题观感对照原型 L1 变体验收；深色主题与切换前 byte-identical（金样锁定）。
- `cargo test --workspace` / `cargo check --workspace --all-targets` /
  `cargo clippy --workspace --all-targets` / `cargo fmt --all --check` 全绿；
  L1 观感、System 运行时跟随、标题栏联动为人工验证项。
