//! 设置对话框的状态模块：配置编辑器 + OnlineFix 预设。
//!
//! 两个纯状态机（[`ConfigEditorState`] / [`OnlineFixState`]）：不碰 egui、不碰 i18n。
//! 错误类型化（[`ConfigEditError`] / [`OfError`]），UI 层按当前语言映射文案；
//! 依赖（Steam 路径、进程运行态、共享 SteamState）从方法参数注入。
//! 渲染层（egui 模态对话框本体）仍在 `ui::App`，本模块只持有状态与 handler。

use crate::config_editor::{self, ConfigError};
use crate::external::ManifestAccessor;
use crate::onlinefix::{self, ActiveOnlineFixGame, CandidateGame, VdfError};
use crate::steam_state::SteamState;
use std::path::{Path, PathBuf};

// ===================== 配置编辑器状态 =====================

/// 配置编辑器的类型化错误（UI 层按分支映射本地化文案）。
pub enum ConfigEditError {
    /// 读盘失败（`<Steam>/opensteamtool.toml`）。携带平台错误消息。
    Load(String),
    /// TOML 语法校验失败（带行列定位）。
    Validation(ConfigError),
    /// 原子写盘失败。携带平台错误消息。
    Save(String),
}

/// 「配置编辑器」状态：缓冲文本、载入标志、校验/写盘错误、保存成功提示。
pub struct ConfigEditorState {
    /// 编辑器缓冲：磁盘内容载入后在此编辑，保存前不落盘。
    pub text: String,
    /// 是否已从磁盘载入（避免每帧重读覆盖用户编辑）。
    loaded: bool,
    /// 最近一次载入/校验/写盘失败的原始错误（None = 无错误）。
    pub err: Option<ConfigEditError>,
    /// 最近一次保存成功（短暂显示「已保存」，再次编辑即清除）。
    pub saved: bool,
    /// 是否含未保存的编辑（编辑置位；载入/保存成功/载入模板后清除）。
    /// 用于「从示例模板创建」覆盖前的确认判断。
    pub dirty: bool,
}

impl ConfigEditorState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            loaded: false,
            err: None,
            saved: false,
            dirty: false,
        }
    }

    /// 标记待载入（打开对话框时调用）：首帧渲染时 [`Self::ensure_loaded`] 读盘。
    pub fn mark_unloaded(&mut self) {
        self.loaded = false;
        self.err = None;
        self.saved = false;
        self.dirty = false;
    }

    /// 首次渲染时读盘（只一次）。文件不存在 → 清空缓冲（UI 显示「从示例模板创建」引导）；
    /// 其他读盘错误 → 类型化 `Load` 错误。
    pub fn ensure_loaded(&mut self, path: &Path) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        match std::fs::read_to_string(path) {
            Ok(text) => self.text = text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.text.clear(),
            Err(e) => {
                self.text.clear();
                self.err = Some(ConfigEditError::Load(e.to_string()));
            }
        }
        self.dirty = false; // 读盘/清空后与磁盘一致。
    }

    /// 保存：先校验（错误带行列定位），通过后原子写入。
    pub fn save(&mut self, path: &Path) {
        match config_editor::validate(&self.text) {
            Err(e) => {
                self.err = Some(ConfigEditError::Validation(e));
                self.saved = false;
            }
            Ok(()) => match config_editor::write_atomic(path, &self.text) {
                Ok(()) => {
                    self.err = None;
                    self.saved = true;
                    self.dirty = false;
                }
                Err(e) => {
                    self.err = Some(ConfigEditError::Save(e.to_string()));
                    self.saved = false;
                }
            },
        }
    }

    /// 编辑器内容变更 → 清除「已保存」提示、标记未保存编辑。
    pub fn mark_edited(&mut self) {
        self.saved = false;
        self.dirty = true;
    }

    /// 从上游示例模板填充（文件缺失时的引导入口）。
    pub fn fill_template(&mut self) {
        self.text = config_editor::EXAMPLE_TEMPLATE.to_owned();
        self.loaded = true;
        self.dirty = false;
        self.err = None;
        self.saved = false;
    }
}

// ===================== OnlineFix 预设状态 =====================

/// OnlineFix 操作的类型化错误（UI 层按分支映射本地化文案）。
pub enum OfError {
    /// AppID 输入不是数字。
    InvalidAppid,
    /// Steam 进程组在运行（写入门闩拦截）。
    WriteBlocked,
    /// `localconfig.vdf` 读写失败。
    Vdf(VdfError),
}

/// 当前选中 (账号, AppID) 的展示状态。
pub enum OfStatus {
    Enabled,
    Disabled,
    /// 刚点击「复制参数」（短暂显示）。
    Copied,
    Error(OfError),
}

/// 「OnlineFix 启动预设」状态：可用账号、AppID 输入/候选、生效游戏看板与写入门闩。
pub struct OnlineFixState {
    /// 可用账号（`userdata/*/config/localconfig.vdf`，打开对话框时刷新）。
    pub accounts: Vec<PathBuf>,
    /// 选中账号下标。
    pub account_idx: usize,
    /// 手动输入/候选取用的 AppID。
    pub appid: String,
    /// Lua 扫描的候选 AppID + ACF 名称映射（ACF 缺失/畸变 name 为 None，UI 回退纯数字）。
    pub candidates: Vec<CandidateGame>,
    /// 当前生效的 OnlineFix 游戏（选中账号反查；看板展示，SPEC AC6）。
    pub active: Option<ActiveOnlineFixGame>,
    /// 当前选中 (账号, AppID) 的展示状态。
    status: Option<OfStatus>,
    /// 上次计算状态时的 (账号 idx, AppID)，避免每帧重读 VDF。
    status_key: Option<(usize, String)>,
}

impl OnlineFixState {
    pub fn new() -> Self {
        Self {
            accounts: Vec::new(),
            account_idx: 0,
            appid: String::new(),
            candidates: Vec::new(),
            active: None,
            status: None,
            status_key: None,
        }
    }

    /// 打开对话框时刷新账号、候选（Lua AppID + ACF 名称）与生效游戏看板。
    /// 进程组运行态写入门闩每次写前实时判定，不在此采样。
    pub fn refresh(&mut self, steam_dir: &Path, manifest: &dyn ManifestAccessor) {
        self.accounts = onlinefix::account_vdf_paths(steam_dir);
        self.account_idx = self.account_idx.min(self.accounts.len().saturating_sub(1));
        self.candidates = onlinefix::scan_lua_appids(steam_dir)
            .into_iter()
            .map(|appid| CandidateGame {
                appid,
                name: onlinefix::resolve_game_name(manifest, steam_dir, appid),
            })
            .collect();
        // 反查生效游戏并同步输入框（进入页签/打开对话框共用入口）。
        self.refresh_active(steam_dir, manifest);
        self.status = None;
        self.status_key = None;
    }

    /// 反查当前选中账号的生效游戏，同步 `appid` 输入框（SPEC AC6「进入页签自动同步」）。
    /// 启用/停用/换账号后由 UI 层再次调用以刷新看板。
    pub fn refresh_active(&mut self, steam_dir: &Path, manifest: &dyn ManifestAccessor) {
        let active_appid = self
            .accounts
            .get(self.account_idx)
            .and_then(|vdf| onlinefix::get_active_game_file(vdf));
        self.active = active_appid.map(|appid| ActiveOnlineFixGame {
            appid,
            name: onlinefix::resolve_game_name(manifest, steam_dir, appid),
        });
        // 生效游戏存在时覆盖输入框（「进入页签自动同步」；停用后无生效游戏则不打扰当前输入）。
        if let Some(appid) = active_appid {
            let id = appid.to_string();
            if self.appid.trim() != id {
                self.appid = id;
                self.status_key = None;
            }
        }
    }

    /// 账号展示名：`userdata/<id>/config/localconfig.vdf` → `<id>`。
    pub fn account_name(vdf: &Path) -> String {
        vdf.parent()
            .and_then(|c| c.parent())
            .and_then(|u| u.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_owned())
            .unwrap_or_else(|| vdf.display().to_string())
    }

    /// 当前展示状态（渲染时按当前语言映射文案）。
    pub fn status(&self) -> Option<&OfStatus> {
        self.status.as_ref()
    }

    /// 选中账号（换账号后状态需重读）。
    pub fn select_account(&mut self, idx: usize) {
        self.account_idx = idx;
        self.status_key = None;
    }

    /// AppID 输入变化 → 清缓存键待重读。
    pub fn appid_changed(&mut self) {
        self.status_key = None;
    }

    /// 重算展示状态（仅当选中的 (账号, AppID) 变化时读盘）。
    pub fn refresh_status(&mut self) {
        let key = (self.account_idx, self.appid.trim().to_owned());
        if self.status_key.as_ref() == Some(&key) {
            return;
        }
        self.status_key = Some(key.clone());
        let Ok(appid) = key.1.parse::<u32>() else {
            return; // 输入未成形：不显示状态。
        };
        let Some(vdf) = self.accounts.get(key.0).cloned() else {
            return;
        };
        self.status = Some(match onlinefix::is_onlinefix(&vdf, appid) {
            Ok(true) => OfStatus::Enabled,
            Ok(false) => OfStatus::Disabled,
            Err(e) => OfStatus::Error(OfError::Vdf(e)),
        });
    }

    /// 启用 OnlineFix（写入 `-onlinefix`）；成功后在界面内直接更新状态，键置空待下帧复读。
    pub fn enable(&mut self, steam_dir: &Path, steam_running: bool, steam: &SteamState) {
        let Some((idx, appid)) = self.prepare_write(steam_dir, steam_running, steam) else {
            return;
        };
        let vdf = &self.accounts[idx];
        match onlinefix::set_onlinefix(vdf, appid) {
            Ok(()) => {
                self.status = Some(OfStatus::Enabled);
                self.status_key = None;
            }
            Err(e) => {
                self.status = Some(OfStatus::Error(OfError::Vdf(e)));
                self.status_key = None;
            }
        }
    }

    /// 停用 OnlineFix（移除 `-onlinefix`）。
    pub fn disable(&mut self, steam_dir: &Path, steam_running: bool, steam: &SteamState) {
        let Some((idx, appid)) = self.prepare_write(steam_dir, steam_running, steam) else {
            return;
        };
        let vdf = &self.accounts[idx];
        match onlinefix::clear_onlinefix(vdf, appid) {
            Ok(()) => {
                self.status = Some(OfStatus::Disabled);
                self.status_key = None;
            }
            Err(e) => {
                self.status = Some(OfStatus::Error(OfError::Vdf(e)));
                self.status_key = None;
            }
        }
    }

    /// 复制参数：仅标记「已复制」（剪贴板副作用由 UI 层执行）。
    pub fn mark_copied(&mut self) {
        self.status = Some(OfStatus::Copied);
    }

    /// 写前准备（enable/disable 共用）：门闩复查 → AppID 解析 → 账号越界检查。
    /// 返回 (账号下标, AppID)；任一前置不满足时已置相应错误状态并返回 None。
    fn prepare_write(&mut self, steam_dir: &Path, steam_running: bool, steam: &SteamState) -> Option<(usize, u32)> {
        if self.write_blocked(steam_dir, steam_running, steam) {
            return None;
        }
        let Ok(appid) = self.appid.trim().parse::<u32>() else {
            self.status = Some(OfStatus::Error(OfError::InvalidAppid));
            self.status_key = None;
            return None;
        };
        self.accounts.get(self.account_idx)?;
        Some((self.account_idx, appid))
    }

    /// 写入门闩：对话框打开期间 Steam 可能已启动，逐次复查进程组。
    /// 返回 true 表示被拦截（status 已置 `WriteBlocked`，UI 层据此提示「请先关闭 Steam」）。
    fn write_blocked(&mut self, steam_dir: &Path, steam_running: bool, steam: &SteamState) -> bool {
        // 快速判定（仅看 steam.exe，2s 缓存）短路；否则实时复查进程组（含残留 webhelper 孤儿）。
        if steam_running || steam.group_running(steam_dir) {
            self.status = Some(OfStatus::Error(OfError::WriteBlocked));
            self.status_key = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ConfigEditorState ----

    #[test]
    fn ensure_loaded_reads_disk_once() {
        let dir = std::env::temp_dir().join(format!("ost_cfg_state_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opensteamtool.toml");
        std::fs::write(&path, "url = \"opensteamtool\"\n").unwrap();

        let mut st = ConfigEditorState::new();
        st.ensure_loaded(&path);
        assert_eq!(st.text, "url = \"opensteamtool\"\n");
        assert!(st.err.is_none());
        assert!(!st.dirty); // 载入后与磁盘一致。
        // 再调不重读：磁盘已改也不覆盖缓冲（避免每帧重读）。
        std::fs::write(&path, "other = 1\n").unwrap();
        st.ensure_loaded(&path);
        assert_eq!(st.text, "url = \"opensteamtool\"\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ensure_loaded_missing_clears_text() {
        let dir = std::env::temp_dir().join(format!("ost_cfg_state_miss_{}", std::process::id()));
        let path = dir.join("opensteamtool.toml"); // 目录不存在 → NotFound
        let mut st = ConfigEditorState::new();
        st.text = "dirty".into();
        st.ensure_loaded(&path);
        assert!(st.text.is_empty());
        assert!(st.err.is_none()); // NotFound 走模板引导，不算错误
    }

    #[test]
    fn save_validates_toml() {
        let dir = std::env::temp_dir().join(format!("ost_cfg_state_sv_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opensteamtool.toml");

        let mut st = ConfigEditorState::new();
        st.text = "[broken".into(); // 非法 TOML
        st.save(&path);
        assert!(matches!(st.err, Some(ConfigEditError::Validation(_))));
        assert!(!st.saved);
        assert!(!path.exists(), "校验失败不应落盘");

        st.text = "url = \"opensteamtool\"\n".into();
        st.save(&path);
        assert!(st.err.is_none());
        assert!(st.saved);
        assert!(!st.dirty); // 保存成功后无未保存修改。
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "url = \"opensteamtool\"\n"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mark_edited_clears_saved_and_sets_dirty() {
        let mut st = ConfigEditorState::new();
        st.saved = true;
        st.mark_edited();
        assert!(!st.saved);
        assert!(st.dirty);
    }

    #[test]
    fn fill_template_loads_example() {
        let mut st = ConfigEditorState::new();
        st.fill_template();
        assert!(st.text.contains("[manifest]"));
        assert!(st.err.is_none());
        assert!(!st.saved);
        assert!(!st.dirty); // 载入模板后无未保存修改。
    }

    // ---- OnlineFixState ----

    /// 构造一个含目标 AppID 空块的合法 localconfig.vdf（与 onlinefix.rs 测试同构）。
    fn sample_vdf(appid: u32) -> String {
        format!(
            r#""UserLocalConfigStore"
{{
    "Software"
    {{
        "Valve"
        {{
            "Steam"
            {{
                "Apps"
                {{
                    "{appid}"
                    {{
                    }}
                }}
            }}
        }}
    }}
}}"#
        )
    }

    fn tmp_steam(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ost_of_state_{}_{}", name, std::process::id()))
    }

    #[test]
    fn enable_blocks_on_running_steam_without_writing() {
        let dir = tmp_steam("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let mut st = OnlineFixState::new();
        st.accounts = vec![dir.join("localconfig.vdf")];
        st.appid = "480".into();
        let steam = SteamState::new();
        // steam_running=true → 写门闩短路拦截，不触盘。
        st.enable(&dir, true, &steam);
        assert!(matches!(
            st.status(),
            Some(OfStatus::Error(OfError::WriteBlocked))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enable_rejects_invalid_appid() {
        let dir = tmp_steam("badid");
        std::fs::create_dir_all(&dir).unwrap();
        let mut st = OnlineFixState::new();
        st.accounts = vec![dir.join("localconfig.vdf")];
        st.appid = "abc".into();
        let steam = SteamState::new();
        st.enable(&dir, false, &steam);
        assert!(matches!(
            st.status(),
            Some(OfStatus::Error(OfError::InvalidAppid))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enable_writes_onlinefix_to_vdf() {
        let dir = tmp_steam("write");
        std::fs::create_dir_all(&dir).unwrap();
        let vdf = dir.join("localconfig.vdf");
        std::fs::write(&vdf, sample_vdf(480)).unwrap();
        let mut st = OnlineFixState::new();
        st.accounts = vec![vdf.clone()];
        st.appid = "480".into();
        let steam = SteamState::new();
        st.enable(&dir, false, &steam);
        assert!(matches!(st.status(), Some(OfStatus::Enabled)));
        let content = std::fs::read_to_string(&vdf).unwrap();
        assert!(
            content.contains("-onlinefix"),
            "vdf 应含 -onlinefix:\n{content}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn select_and_appid_change_clear_status_cache() {
        let dir = tmp_steam("cache");
        std::fs::create_dir_all(&dir).unwrap();
        let vdf = dir.join("localconfig.vdf");
        std::fs::write(&vdf, sample_vdf(480)).unwrap();
        let mut st = OnlineFixState::new();
        st.accounts = vec![vdf, PathBuf::from("other/vdf")];
        st.appid = "480".into();
        st.refresh_status();
        assert!(matches!(st.status(), Some(OfStatus::Disabled)));

        st.select_account(1);
        st.appid_changed();
        st.refresh_status();
        // 选中账号切换后重读（other/vdf 不存在 → 无状态或错误），缓存键已换不返回旧态。
        assert!(
            !matches!(st.status(), Some(OfStatus::Disabled)),
            "换账号后不应复用旧账号状态"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn account_name_derives_userdata_id() {
        let vdf = Path::new(r"C:\Steam\userdata\12345678\config\localconfig.vdf");
        assert_eq!(OnlineFixState::account_name(vdf), "12345678");
    }

    // ---- T7：ACF 名称映射 + 生效游戏反查（#11） ----

    /// 可控 ACF 读取桩：appid → ACF 内容（None = 缺失；由调用方决定合规/畸变）。
    struct MockManifest {
        acfs: std::collections::HashMap<u32, Option<String>>,
    }
    impl ManifestAccessor for MockManifest {
        fn read_manifest_content(&self, _steam_dir: &Path, appid: u32) -> Option<String> {
            self.acfs.get(&appid).cloned().flatten()
        }
    }

    /// 构造含多 AppID 子块 + LaunchOptions 的 localconfig.vdf。
    fn sample_vdf_multi(entries: &[(u32, Option<&str>)]) -> String {
        let mut s = String::from(
            "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"Apps\"\n\t\t\t\t{\n",
        );
        for (appid, launch) in entries {
            s.push_str(&format!("\t\t\t\t\t\"{appid}\"\n\t\t\t\t\t{{\n"));
            if let Some(v) = launch {
                s.push_str(&format!("\t\t\t\t\t\t\"LaunchOptions\"\t\t\"{v}\"\n"));
            }
            s.push_str("\t\t\t\t\t}\n");
        }
        s.push_str("\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n");
        s
    }

    #[test]
    fn refresh_maps_candidate_names_via_manifest() {
        let dir = tmp_steam("names");
        std::fs::create_dir_all(&dir).unwrap();
        // 候选 AppID 由 Lua 扫描注入（config/lua/*.lua）。
        let lua = dir.join("config").join("lua");
        std::fs::create_dir_all(&lua).unwrap();
        std::fs::write(
            lua.join("app.lua"),
            "addappid(367520)\naddappid(480)\naddappid(9999)\n",
        )
        .unwrap();
        let mut manifest = MockManifest {
            acfs: std::collections::HashMap::new(),
        };
        // 合规 ACF → 名称；畸变 ACF → 纯数字回退；缺失 ACF → 纯数字回退。
        manifest.acfs.insert(
            367520,
            Some("\"AppState\"\n{\n\t\"name\"\t\t\"空洞骑士\"\n}\n".into()),
        );
        manifest.acfs.insert(480, Some("garbage".into()));
        let mut st = OnlineFixState::new();
        st.refresh(&dir, &manifest);
        let by_id = |id: u32| st.candidates.iter().find(|c| c.appid == id).unwrap();
        assert_eq!(by_id(367520).name.as_deref(), Some("空洞骑士"));
        assert_eq!(by_id(480).name, None, "畸变 ACF 回退纯数字");
        assert_eq!(by_id(9999).name, None, "缺失 ACF 回退纯数字");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refresh_active_finds_marked_game_and_syncs_input() {
        let dir = tmp_steam("active");
        std::fs::create_dir_all(&dir).unwrap();
        let vdf = dir.join("localconfig.vdf");
        // 多 AppID：仅 1361510 带 -onlinefix。
        std::fs::write(
            &vdf,
            sample_vdf_multi(&[(1361510, Some("-onlinefix")), (480, None)]),
        )
        .unwrap();
        let mut manifest = MockManifest {
            acfs: std::collections::HashMap::new(),
        };
        manifest.acfs.insert(
            1361510,
            Some("\"AppState\"\n{\n\t\"name\"\t\t\"双人成行\"\n}\n".into()),
        );
        let mut st = OnlineFixState::new();
        st.accounts = vec![vdf.clone()];
        st.appid = "0".into(); // 会被同步覆盖。
        st.refresh_active(&dir, &manifest);
        let active = st.active.as_ref().expect("应反查到生效游戏");
        assert_eq!(active.appid, 1361510);
        assert_eq!(active.name.as_deref(), Some("双人成行"));
        // 进入页签自动同步 AppID 输入框。
        assert_eq!(st.appid, "1361510");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refresh_active_after_disable_clears_board() {
        let dir = tmp_steam("active-none");
        std::fs::create_dir_all(&dir).unwrap();
        let vdf = dir.join("localconfig.vdf");
        std::fs::write(&vdf, sample_vdf_multi(&[(480, None)])).unwrap();
        let manifest = MockManifest {
            acfs: std::collections::HashMap::new(),
        };
        let mut st = OnlineFixState::new();
        st.accounts = vec![vdf];
        st.appid = "480".into();
        st.refresh_active(&dir, &manifest);
        assert!(st.active.is_none(), "无标记 → 无看板");
        // 无生效游戏时输入框不被覆盖。
        assert_eq!(st.appid, "480");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refresh_active_follows_account_switch() {
        let dir = tmp_steam("switch");
        std::fs::create_dir_all(&dir).unwrap();
        let vdf_a = dir.join("a.vdf");
        let vdf_b = dir.join("b.vdf");
        std::fs::write(&vdf_a, sample_vdf_multi(&[(1361510, Some("-onlinefix"))])).unwrap();
        std::fs::write(&vdf_b, sample_vdf_multi(&[(480, None)])).unwrap();
        let manifest = MockManifest {
            acfs: std::collections::HashMap::new(),
        };
        let mut st = OnlineFixState::new();
        st.accounts = vec![vdf_a, vdf_b];
        st.refresh_active(&dir, &manifest);
        assert_eq!(st.active.as_ref().map(|a| a.appid), Some(1361510));
        // 换账号 → 反查新账号（无标记 → 看板消失，输入框保留）。
        st.select_account(1);
        st.refresh_active(&dir, &manifest);
        assert!(st.active.is_none());
        assert_eq!(st.appid, "1361510", "换账号后无生效游戏不覆盖输入框");
        std::fs::remove_dir_all(&dir).ok();
    }
}
