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

## 验收

- 设置 — 通用页签「主题」小节：三态下拉（跟随系统 / 深色 / 浅色），即改即存、
  即时生效、重启恢复；双语文案。
- System 模式：启动读 OS 深浅；运行中 OS 切换 UI 即时跟随；原生标题栏联动
  （`sync_window_theme` 默认开启）。
- 浅色主题观感对照原型 L1 变体验收；深色主题与切换前 byte-identical（金样锁定）。
- `cargo test --workspace` / `cargo check --workspace --all-targets` /
  `cargo clippy --workspace --all-targets` / `cargo fmt --all --check` 全绿；
  L1 观感、System 运行时跟随、标题栏联动为人工验证项。
