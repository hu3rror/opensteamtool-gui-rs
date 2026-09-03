//! 构建脚本：把 `app.ico` 嵌入 exe 的 Windows 资源段（.rsrc）。
//!
//! 缺少这一步时，桌面/任务栏/资源管理器的图标从 exe 资源提取得到空白：
//! `ViewportBuilder::with_icon` 只设置运行中窗口的图标（标题栏可见），
//! 而 shell 层图标读取的是 exe 内嵌资源。资源脚本见 `app.rc`。

fn main() {
    embed_resource::compile("app.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed app.ico as Windows resource");
}