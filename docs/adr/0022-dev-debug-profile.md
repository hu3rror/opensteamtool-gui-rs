# dev 构建调试信息：limited + dev-full 逃生口

`target/` 曾膨胀至 16G（本次清理后 8.8G），根因之一是 dev profile 默认 `debug = "full"`：Windows/MSVC 下每次 `cargo build` 为主 crate 生成 ~185MB PDB（全量类型与变量符号），主 crate 反复编译累积 12+ 份 hash 后缀 PDB，而 Cargo 除 `cargo clean` 外从不回收旧产物。决定：dev 默认降为 `debug = "limited"` 掐掉类型符号大头；需要完整变量调试时显式改走 `--profile dev-full`。

## 决策

- `[profile.dev] debug = "limited"`：保留行号表与模块级调试信息——断点、单步、panic 回溯（文件名/行号/函数名）完整可用；不生成类型与变量级信息，IDE 中局部变量查看 / watch 表达式不可用。
- 新增 `[profile.dev-full]`（`inherits = "dev"` + `debug = "full"`）作为逃生口：日常 `cargo run` 用默认 dev，需要完整变量调试的疑难场景用 `cargo build --profile dev-full` / `cargo run --profile dev-full`（产物在 `target/dev-full/`）。不改任何默认命令，靠显式 flag 承担完整调试的编译成本。
- 回退方式：删除 Cargo.toml 两个 profile 段即恢复默认。

## 为什么做

- target 增长的主引擎是类型符号：limited 移除类型/变量信息后，单次构建产出的 PDB 体积数量级下降（~185MB/份 → 实测为准），每次 `--all-targets` 检查重复产出的新 PDB 不再是大头，target 增速随之大降——这比「定期清理存量」更治本（清存量不降低每次新增量）。
- 本仓库调试形态以 println/日志 + 断点看行号 + panic 回溯为主，limited 覆盖其中绝大多数场景；完整变量调试是低频分支，用显式 profile 把它的成本（体积 + 编译时间）留给真正需要的时候。
- `cargo test`（test profile 继承 dev）的二进制随之变小，测试行为不受影响；release/CI release 构建完全不受影响。

## 否决的方案

- **`debug = "line-tables-only"`**：同样无变量信息，但模块级信息更少、回溯质量进一步下降，相比 limited 省不了多少体积——收益重复、体验更差。
- **`debug = 0`**：panic 回溯只剩裸地址，排错基本不可用。
- **`[profile.dev] strip = "debuginfo"`**：与调试互斥，是发布手段，放在 dev 里属于错误归属。
- **sccache**：解决 `cargo clean` 后的重编速度，不解决 PDB 产物体积与 target 增速，两者互补不互替。
- **定期 `cargo sweep`**：依赖外部工具，且只清陈旧产物，不降低每次新产物的大小——作为存量清理手段保留价值，但不代替本决策。

## 验收

- `cargo check -p opensteamtool-manager` 通过（配置合法）。
- 下次 `cargo build` 后实测主 crate PDB 体积，与 ~185MB 基线对比并回填本 ADR 的数字。注意：profile 设置变化会使 dev 依赖 fingerprint 失效，切换后首次 build/check 会有一次全量 dev 重编，属一次性成本。`[待手动验证]`
- `cargo run --profile dev-full` 能构建出带完整变量调试信息的二进制（目标 `target/dev-full/`）。
- dev 下断点与回溯仍可用（打断点 + 触发 panic 验证行号映射正确）。`[NEEDS MANUAL VERIFICATION]`