# 兼容性体检：哈希探针 + compat_flow 编排状态机

上游 OpenSteamTool 自重构后不再把特征码打包进 DLL：注入器在 Steam 启动时计算本地核心 DLL 的 SHA-256，并按通道（`steam-monitor` 仓库分支 = 通道）拉取匹配的 TOML 签名/IPC 规约。本工具在 Card 1 提供兼容性体检指示与离线缓存预热，前提是**零阻塞 UI**。

## 决策

- **镜像链**：用户自定义模板 `<Steam>/opensteamtool.toml` 的 `[remote].url_template` 存在即**替代**（非追加）内置源，不再回退 GitHub/jsDelivr；否则 GitHub Raw → jsDelivr CDN。占位符 `{channel}`/`{component}`/`{sha256}`。
- **探针只读、不进忙碌门禁**：compat 探针/刷新/预热是只读后台操作（零点击、不冻结 UI），与交互类互斥（ADR-0007）无关；互斥由编排层在途去重承担。
- **三权分立**：算子层 `compat.rs` 无状态探测、编排层 `compat_flow.rs` 纯状态机（无 IO，App 只喂事件、执行返回的 Effect、渲染展示态）、渲染层 `ui.rs`。路径防抖（路径事件流程内去重）与陈旧丢弃（代数戳）都在编排层完成。
- **每代数至多一次全量网络探测**：刷新预算按体检代数（`Epoch`）计，产出即置位、无隐式重试——弱网下不因复检/预热事件反复打上游镜像；刷新完成无论成败（含 `NetworkError`）都视为已消耗本代数预算。路径变更推进代数，迟到结果按代数戳丢弃。
- **快速体检完全零网络**：缓存命中直接绿（`CompatibleOffline`）；未命中乐观假定 `RemoteAvailable{cached:false}`（琥珀 + 预热可点），后台刷新补齐网络事实（短路项可升级、404 项降级 `IncompatiblePending`）。
- **验证缓存**：后台确认适配的项持久化到 `<exe>/cache/verified.toml`（`{target → sha256}`），下次启动零网络绿灯；只记 Found（404/错误不缓存，避免上游补发签名后本地失真）；无时间有效期，DLL 哈希变化自动失效重验。
- **自动预热**：体检落定 Online 且无预热进行中时自动触发下载，失败静默（徽章保持、手动入口保留），成功复用「缓存已就绪」提示；预热目标以探针携带的 `signature_cached` 存在性事实为准，编排层不落盘检查。

实现规格（通道映射表、状态判定矩阵、模块接口、UI 布局、词表、验收）见 SPEC.md §7.1–7.9。
