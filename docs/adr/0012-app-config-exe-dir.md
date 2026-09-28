# 应用配置：exe 同目录 `config.toml`，便携优先

首次启动引导（wizard）与设置页需要把语言、Steam 路径跨会话持久化（issue #26/#28）。
决定：新增 `config` 模块，配置落盘到 **exe 同目录 `config.toml`**，不落 `%APPDATA%`。

## 取舍

- **便携优先**：本工具按便携版分发（`dlls/`、`cache/` 都在 exe 旁，整个目录拷贝即
  迁移）；配置同目录，设置随目录带走，不污染用户 profile，也不受多账号环境干扰。
- **代价与接受**：exe 目录可能只读（如装在 Program Files）或两个实例并存。写失败按
  「尽力持久化」处理（记日志不中断操作），单实例便携分发下可接受；`%APPDATA%` 的
  写入可靠性换取不了「拷贝目录即迁移」这一便携核心价值。

## 格式与语义

- `version = 1`：格式版本，未来迁移判据；版本不符按「未配置」降级（旧程序读新文件
  不猜字段），不 panic。
- `steam_path`：空 = 未设置，启动回退注册表检测；检测结果不自动写回（配置只反映
  用户显式选择，检测是建议不是选择）。
- `language = "auto" | "zh" | "en"`：三态，`auto` 在启动时解析为系统语言
  （`GetUserDefaultUILanguage`），`zh`/`en` 固定；顶栏切换把偏好钉到另一侧并持久化。

## 一致性与错误语义

- **不存派生状态**：补丁是否已下载是文件系统事实（`dlls/` 三个目标 DLL 是否齐全，
  ADR-0011），配置永远不存第二份，避免两处事实漂移。
- **原子写**：复用 `fsutil::write_atomic`（同目录临时文件 + rename），任何时刻观察
  不到半截 `config.toml`。
- **类型化错误、启动降级**：缺失文件 = 未配置（默认值）；损坏 TOML / 字段非法 =
  `ConfigError::Parse`；版本不符 = `ConfigError::UnsupportedVersion`。启动路径一律
  降级默认值继续，不 panic 不崩溃；错误保留类型供 wizard/设置页在 UI 显式呈现。
- **严格解析**：非法内容整体拒绝（降级默认值），不做部分读取——宽容会悄悄吞掉
  写错的文件，严格让调用方显式决策。

## 范围

wizard 的三步编排、设置页的三态选择器与「保存路径无效时引导修复」随各自 ticket
落地（#26 拆分）；本 ADR 只固定配置的位置、格式与读写语义。

## 验收

单测覆盖 load/save 往返（三语言 × 有/无路径）、缺文件默认值、损坏/字段非法 →
类型化 Parse 错误、版本不符 → UnsupportedVersion、原子写无残留临时文件、language
三态解析/生效/顶栏切换表；`cargo test`、`cargo check --all-targets`、`cargo clippy
--all-targets` 通过。
