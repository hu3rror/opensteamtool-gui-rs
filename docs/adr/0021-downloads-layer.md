# 下载层：first_match + 策略化 agent + 原子落盘（downloads 模块）

两套「取字节 → 落盘」实现并存：`updater` 单 URL 下载（非 2xx 一律为网络错误）、`compat` 镜像链下载（404 记尾续链、全败返最后一个错误）——404 与网络错误语义两套、顺序处理两套；且 `compat` 直调 `crate::updater::download_agent()` 跨模块绕 seam，updater 未暴露「取字节 → 落盘」抽象。决策：提炼深模块 `downloads` 统一两条链路（issue #47）。

## 决策

- **downloads 深模块**（公开面仅四项）：`first_match(urls, policy) -> Result<Vec<u8>, DownloadError>`、`Policy::{Small, Large}`、`write_atomic(path, bytes)` 与 `DownloadError::{NotFound404, Network(String)}`。其余为模块私有（含注入 seam `first_match_with`，与 `probe_all`/`probe_all_with` 同构）。
- **404 权威语义**：与既有 HEAD 探针（`probe_urls` 的「404 优先于网络错误」）对齐——404 续链不终止，其余错误记尾续链；链耗尽时链上曾见 404 → 报 404，否则报最后一个错误；空链报 `no URLs`（保现状）。辩护：404 是「上游确无此文件」的确定性信号，网络错误是暂时性的，确定性信号优先（镜像 CDN 抖动不应把「上游未适配」错报成「网络不可用」）。
- **策略化 agent 两档**（现状 updater 下载档 10s/600s 与镜像 TOML 共用是错误归属）：
  - `Small`：connect 10s / global 30s / body 30s，body 上限 ureq 默认——镜像链签名 TOML 实测 4-6 KB，30s 两个数量级余量；
  - `Large`：connect 10s / global 10min / body 10min，body 上限显式 512MB——GitHub 更新 zip 经 302 到 CDN，慢网络 body 阶段可远超 30s，且 zip 必超 ureq 10MB 默认上限。
- **原子落盘**：落盘门面复用 `fsutil::write_atomic`（临时文件 + rename）。`updater::extract_update` 的逐 DLL 写入与末尾 `version.txt`、`compat::write_cache_file` 全部经 `downloads::write_atomic`——消除「写盘中途失败留下『存在但截断』的文件、被文件本位判据（`is_file`）误判为完整」的中间态。`write_atomic` 不创建父目录（目录缺失以 `NotFound` 报错），目录存在性由调用方负责——`compat::write_cache_file` 与 `updater::extract_update` 均已在写入前 `create_dir_all`；本合同取代 #47 计划文本中的「parent-dir creation」。
- **落点**：`updater::download_and_extract` = `first_match(&[zip_url], Large)` + 解压（单 URL 即长度为 1 的链）；`compat::precache` = `first_match(&urls, Small)` + 原子写；`compat::download_first` 与 `updater::download_agent`（pub(crate)）删除。
- **边界**：镜像链 URL 构建 `build_urls` 留 compat（ADR-0006 镜像链语义不并入 downloads，downloads 只消费序列）；HEAD 探针（`probe_urls`/`head_probe`/`RemoteOutcome`/`probe_agent`）留 compat（三值结果喂体检决策矩阵）；updater 的 API JSON 查询（`check_update`/`check_app_update`）不入 downloads。
- **外层错误类型不变**：`UpdateError` / `CompatError` 保留，`DownloadError` 在调用点映射。
- **行为变化**（有意为之，spec 记录在案）：整链失败时 404 优先于最后一个网络错误；zip 非 2xx 文案由「download HTTP {code}」统一为「HTTP {code}」；签名 TOML 下载超时 600s → 30s；DLL 与 version.txt 改原子写；下载请求统一附带浏览器 UA（原 updater zip 下载带、compat 镜像链不带，现两者一致）；compat 预热错误文案变化（404 不再附 URL，非 404 状态码以「HTTP {code}」呈现而非 ureq 原始文本）。

## 为什么做

- 404/网络错误语义双套是核心张力：下载链的错误报告（last-err 无优先级）与 HEAD 探针（404 权威）互相矛盾，用户看到的失败归类取决于走哪条链路。
- 为「取字节 → 原子落盘」提供唯一测试 seam：GET 下载链的 404 回退次序与错误选择此前零测试（`probe_urls_prefers_404_over_error` 只覆盖 HEAD 视角）。
- 消除跨模块绕 seam（compat → updater 内部函数），模块边界诚实化。

## 否决的方案

- **共享 GET/HEAD 抽象**：探针要三值结果（Found/404/Error）喂决策矩阵，下载只要「字节或错误」，诉求不同构，强行抽象是过拟合；规则一致性由本 ADR 约束而非共享代码。
- **整组原子**（staging 目录 / 组事务回滚 3 DLL + version.txt）：单文件原子写已消除截断文件中间态；组事务为极小概率故障引入复杂度，不值得。
- **`download_to_file` 组合原语**（取字节+落盘一步）：updater 隔 zip 解压用不上，只为 compat 设计会使接口半空。
- **HEAD 探针迁入 downloads**：探针语义是体检域（`decide` 矩阵耦合），且其 404 权威已有测试，迁移面大、收益为零。

## 验收

- `cargo test --workspace`：254 通过 / 0 失败 / 2 忽略（新增 downloads 9 项：链回退次序、404 优先、错误记尾、空链、短路、原子写往返、父目录缺失报错）。
- `cargo check --all-targets` / `cargo clippy --all-targets` / `cargo fmt --all --check` 全绿。
- `precache_e2e_known_hash`（真实网络）验证 Small 档首 URL 命中短路与原子落盘。
- 行为等价手动验收：镜像链预热（成功 / 404 / 网络错误路径）与补丁更新下载在生产环境按预期工作（验收人 = 用户）。