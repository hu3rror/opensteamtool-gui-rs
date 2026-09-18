# 技术栈：egui/eframe（glow），Rust 单二进制

原实现是 Python + tkinter 单文件（PyInstaller 打包 ~15MB），启动有「加载感」；Tauri/WebView2 路线被认为过重（WebView2 内存 100-250MB）。决定：纯 Rust 原生 GUI——egui/eframe + glow 渲染后端，无 Node、无 WebView2、无 Python sidecar。

- **egui/eframe（即时模式）**：原生窗口、毫秒级首帧，适合本工具 3 卡片轻量 UI；glow 后端比 wgpu 更小更轻，无 GPU 时软件渲染兜底。
- **后端逻辑全部 Rust 内联**（无 sidecar）：避免子进程启动开销，重燃加载感。
- **发布形态**：单 exe（目标 1-3MB；release 实测约 6.7MB）+ 同目录 `dlls/`，便携 ZIP 解压即用，无运行时依赖。

验收目标：启动 < 1s、首帧即完整 UI 无白屏；内存 ~50-90MB。工具链：rustc/cargo 1.98+、MSVC、edition 2024；release 配置 `opt-level="z"` + fat LTO + `panic="abort"`。
