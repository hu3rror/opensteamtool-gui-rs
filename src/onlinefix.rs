//! 「OnlineFix 启动预设」模块：读写 `<Steam>/userdata/<账号>/config/localconfig.vdf`
//! 中指定 AppID 的 `LaunchOptions`，实现 `-onlinefix` 参数的启用/停用（持久预设）。
//!
//! 设计要点（issue #1 PR-2）：
//! - **逐字节保真**：以字节行处理，未触及的行原样保留（含非 UTF-8 字节/换行风格/BOM）；
//! - **外科手术式编辑**：只改动目标 `LaunchOptions` 行，缺失的祖先块按需插入；
//! - **安全**：写入前创建时间戳备份；Steam 运行中由 UI 层阻止（本模块不检测进程）；
//! - 不引入第三方 VDF crate；结构仅覆盖 localconfig.vdf 实际形态（引号键 + 嵌套块 + 引号值）。
//!
//! 上游机制：游戏命令行含 `-onlinefix` 时，OpenSteamTool 将 AppID 重写为 480
//! 实现在线修复（Hooks_Misc.cpp）。故本模块只负责把参数写进 Steam 的启动选项。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// OnlineFix 启动参数（附赠「复制」功能用，也是写入 LaunchOptions 的令牌）。
pub const ONLINEFIX_ARG: &str = "-onlinefix";

/// VDF 读写错误：IO 或结构异常（根块缺失等）。
#[derive(Debug)]
pub enum VdfError {
    Io(io::Error),
    /// 结构不符合 localconfig.vdf 的实际形态。携带机器可读的错误码（i18n 映射）。
    Structure(&'static str),
}

impl std::fmt::Display for VdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VdfError::Io(e) => write!(f, "{e}"),
            VdfError::Structure(code) => write!(f, "structure: {code}"),
        }
    }
}

impl From<io::Error> for VdfError {
    fn from(e: io::Error) -> Self {
        VdfError::Io(e)
    }
}

/// 扫描 `<Steam>/config/lua/*.lua` 中的 `addappid(<数字>)`（跳过注释行），
/// 返回去重排序的 AppID 列表。Lua 文件可能非 UTF-8（如 GBK），按 lossy 读取，
/// addappid 行是 ASCII 不受影响。
pub fn scan_lua_appids(steam_dir: &Path) -> Vec<u32> {
    let lua_dir = steam_dir.join("config").join("lua");
    let Ok(entries) = fs::read_dir(&lua_dir) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("lua") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let t = line.trim_start();
            // 跳过注释行（-- / //），避免把 `--addappid(...)` 误收。
            if t.starts_with("--") || t.starts_with("//") {
                continue;
            }
            // addappid 大小写不敏感；提取括号内第一个十进制数字。
            let lower = t.to_ascii_lowercase();
            let Some(pos) = lower.find("addappid") else {
                continue;
            };
            let rest = &t[pos + "addappid".len()..];
            let Some(open) = rest.find('(') else {
                continue;
            };
            let after = &rest[open + 1..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<u32>() {
                ids.push(id);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// 枚举带 `config/localconfig.vdf` 的用户账号（`userdata/<账号>/config/localconfig.vdf`），
/// 按账号名排序返回文件路径。
pub fn account_vdf_paths(steam_dir: &Path) -> Vec<PathBuf> {
    let userdata = steam_dir.join("userdata");
    let Ok(entries) = fs::read_dir(&userdata) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let account = entry.path();
        let vdf = account.join("config").join("localconfig.vdf");
        if vdf.is_file() {
            paths.push(vdf);
        }
    }
    paths.sort();
    paths
}

/// 读取某账号 VDF 中指定 AppID 的 `LaunchOptions` 值（去除两侧空白；未设置 = None）。
/// 纯读取：路径缺失/无该项都不算错误。
pub fn read_launch_options(vdf: &Path, appid: u32) -> Result<Option<String>, VdfError> {
    let bytes = fs::read(vdf)?;
    let doc = Doc::parse(&bytes);
    let Some(appid_block) = doc.descend(&[b"UserLocalConfigStore", b"Software", b"Valve", b"Steam", b"Apps"])? else {
        return Ok(None);
    };
    let Some(appid_block) = doc.find_child_block(appid_block, appid.to_string().as_bytes()) else {
        return Ok(None);
    };
    let Some(kv) = doc.find_kv(appid_block, b"LaunchOptions") else {
        return Ok(None);
    };
    Ok(Some(value_text(&doc.lines[kv]).trim().to_owned()))
}

/// 启用 OnlineFix：把 `-onlinefix` 追加进该 AppID 的 LaunchOptions（保留既有选项、去重）。
/// 文件不存在该 AppID 块/祖先块时按需创建；仅在内容变化时写盘（写前备份）。
pub fn set_onlinefix(vdf: &Path, appid: u32) -> Result<(), VdfError> {
    edit_vdf(vdf, appid, true, |value| add_token(value, ONLINEFIX_ARG))
}

/// 停用 OnlineFix：从 LaunchOptions 移除 `-onlinefix`；选项清空则删除该行。
/// 路径缺失时视为无操作（不创建、不写盘）。
pub fn clear_onlinefix(vdf: &Path, appid: u32) -> Result<(), VdfError> {
    edit_vdf(vdf, appid, false, |value| remove_token(value, ONLINEFIX_ARG))
}

/// 该 AppID 的 LaunchOptions 是否已含 `-onlinefix`（未设置 = false）。纯读取。
pub fn is_onlinefix(vdf: &Path, appid: u32) -> Result<bool, VdfError> {
    Ok(read_launch_options(vdf, appid)?
        .map(|v| has_token(&v, ONLINEFIX_ARG))
        .unwrap_or(false))
}

/// 备份 VDF 到 `<文件名>.bak-<unix秒>`，返回备份路径。
pub fn backup(vdf: &Path) -> io::Result<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!(
        "{}.bak-{stamp}",
        vdf.file_name().and_then(|n| n.to_str()).unwrap_or("localconfig")
    );
    let bak = vdf.with_file_name(name);
    fs::copy(vdf, &bak)?;
    Ok(bak)
}

/// 通用编辑：定位并替换目标行的值；值未变则不写盘。`edit` 对当前值做变换。
/// `create_chain` = true（启用）时，缺失的祖先块/AppID 块按需创建；
/// false（停用）时链缺失视为无操作（不创建、不写盘），清空值则删除该行。
fn edit_vdf(
    vdf: &Path,
    appid: u32,
    create_chain: bool,
    edit: impl Fn(&str) -> String,
) -> Result<(), VdfError> {
    let bytes = fs::read(vdf)?;
    let mut doc = Doc::parse(&bytes);

    // 根块缺失：启用 → 结构错误；停用 → 无操作。
    let Some(mut root) = doc.find_child_block_from(0, b"UserLocalConfigStore") else {
        return if create_chain {
            Err(VdfError::Structure("missing_root_chain"))
        } else {
            Ok(())
        };
    };

    // 下钻 Software → Valve → Steam → Apps；启用时缺失即创建空块。
    let chain: [&[u8]; 4] = [b"Software", b"Valve", b"Steam", b"Apps"];
    for key in chain {
        root = match doc.find_child_block(root, key) {
            Some(b) => b,
            None if create_chain => doc.insert_empty_block(&root, key),
            None => return Ok(()), // 停用且链缺失：无操作。
        };
    }

    let launch_idx = match doc.find_child_block(root, appid.to_string().as_bytes()) {
        Some(block) => doc.find_kv(block, b"LaunchOptions"),
        None => None,
    };
    let has_kv = launch_idx.is_some();
    let current = launch_idx
        .map(|i| value_text(&doc.lines[i]).trim().to_owned())
        .unwrap_or_default();
    let next = edit(&current);
    let changed = !has_kv || next != current;
    if !changed {
        return Ok(()); // 无变化：不写盘（幂等）。
    }

    backup(vdf)?;

    if let Some(idx) = launch_idx {
        if next.is_empty() {
            doc.remove_line(idx); // 选项清空 → 删除该行。
        } else {
            doc.rewrite_value(idx, &next);
        }
    } else {
        // AppID 块或 LaunchOptions 行缺失：仅启用时插入。
        if !create_chain {
            return Ok(()); // 停用且无目标行：无操作。
        }
        let appid_block = doc
            .find_child_block(root, appid.to_string().as_bytes())
            .unwrap_or_else(|| doc.insert_empty_block(&root, appid.to_string().as_bytes()));
        let kv_line = format!(
            "{}\"LaunchOptions\"\t\t\"{}\"",
            indent_tabs(appid_block.indent + 1),
            escape_value(&next),
        )
        .into_bytes();
        doc.insert_lines_before(appid_block.close, &[kv_line]);
    }

    doc.write(vdf)?;
    Ok(())
}

// ---------- 行分类与值处理 ----------

/// 行的种类。
enum LineKind {
    /// 块开头的键（`"key"`，其后行是 `{`）。
    OpenKey,
    /// 键值行（`"key"\t\t"value"`）。
    Kv,
    /// `{` / `}`。
    Brace,
}

/// 行分类结果：`(种类, 键, 值, 前导 tab 数)`。
type LineInfo<'a> = (LineKind, &'a [u8], Option<&'a [u8]>, usize);

/// 解析一行：返回 (种类, 键字节, 值可选, 前导 tab 数)。空行/注释行返回 None。
fn classify(line: &[u8]) -> Option<LineInfo<'_>> {
    let mut indent = 0;
    while indent < line.len() && line[indent] == b'\t' {
        indent += 1;
    }
    let rest = &line[indent..];
    if rest.is_empty() || rest.starts_with(b"//") {
        return None;
    }
    if rest == b"{" || rest == b"}" {
        return Some((LineKind::Brace, rest, None, indent));
    }
    if rest[0] == b'"' {
        // 键：引号对。
        let (key, after_key) = quoted(rest)?;
        let after_trim = trim_tabs_spaces(after_key);
        if after_trim.is_empty() {
            return Some((LineKind::OpenKey, key, None, indent));
        }
        // 值：`"value"`（取行内最后一对引号内内容；可能含 `\"` 转义）。
        if after_trim[0] == b'"' {
            let value = quoted_value(after_trim);
            return Some((LineKind::Kv, key, value, indent));
        }
    }
    None
}

/// 解析 `"key"` 前缀，返回 (key, 剩余部分)。
fn quoted(line: &[u8]) -> Option<(&[u8], &[u8])> {
    if line.first()? != &b'"' {
        return None;
    }
    let mut i = 1;
    let mut prev_escape = false;
    while i < line.len() {
        let b = line[i];
        if b == b'"' && !prev_escape {
            return Some((&line[1..i], &line[i + 1..]));
        }
        prev_escape = b == b'\\' && !prev_escape;
        i += 1;
    }
    None
}

/// 键值行中取值：跳跃空白后要求 `"`，取最后一对引号内的原始字节（含转义）。
fn quoted_value(line: &[u8]) -> Option<&[u8]> {
    let start = line.iter().position(|&b| b == b'"')?;
    let mut i = line.len();
    // 最后一个 `"`（不紧跟反斜杠转义）即为收尾引号。
    let mut backslashes = 0;
    while i > start + 1 {
        i -= 1;
        if line[i] == b'"' && backslashes % 2 == 0 {
            return Some(&line[start + 1..i]);
        }
        if line[i] == b'\\' {
            backslashes += 1;
        } else {
            backslashes = 0;
        }
    }
    None
}

/// 从键值行取值（key 之后的引号值；非键值行返回空串）。
fn value_text(line: &[u8]) -> &str {
    if let Some((LineKind::Kv, _, Some(v), _)) = classify(line) {
        std::str::from_utf8(v).unwrap_or("")
    } else {
        ""
    }
}

/// 值写入时转义引号（VDF 内 `"` 写作 `\"`）。
fn escape_value(v: &str) -> String {
    v.replace('"', "\\\"")
}

/// 去掉行内除键引号内容外的空白后剩余部分。
fn trim_tabs_spaces(mut s: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = s.split_first() {
        if b == b'\t' || b == b' ' {
            s = rest;
        } else {
            break;
        }
    }
    s
}

// 令牌级操作：按双引号感知的分词处理启动选项（保留带引号含空格的路径片段）。

/// 分词：双引号内视为整体，引号外按空白切分。
fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
        let chars = value.chars();
    for c in chars {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

/// 值中是否包含该令牌（按分词比较）。
fn has_token(value: &str, token: &str) -> bool {
    tokenize(value).iter().any(|t| t == token)
}

/// 追加令牌（去重；`-onlinefix` 单令牌）。
fn add_token(value: &str, token: &str) -> String {
    let trimmed = value.trim();
    if has_token(trimmed, token) {
        return trimmed.to_owned();
    }
    if trimmed.is_empty() {
        token.to_owned()
    } else {
        format!("{trimmed} {token}")
    }
}

/// 移除令牌（按分词移除，保留其余顺序与空白规范为单空格）。
fn remove_token(value: &str, token: &str) -> String {
    let kept: Vec<String> = tokenize(value)
        .into_iter()
        .filter(|t| t != token)
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        kept.join(" ")
    }
}

/// 前导 tab 串。
fn indent_tabs(n: usize) -> String {
    "\t".repeat(n)
}

// ---------- 文档模型 ----------

/// localconfig.vdf 文档：字节级行集合 + 换行风格 + BOM 标记。
struct Doc {
    lines: Vec<Vec<u8>>,
    eol: &'static [u8],
    bom: bool,
    /// 原文是否以换行结尾（write 时补回，保证逐字保真）。
    trailing_newline: bool,
}

/// 定位到的块：开括号/闭括号行号 + 本块前导 tab 数。
#[derive(Clone, Copy)]
struct Block {
    open: usize,
    close: usize,
    indent: usize,
}

impl Doc {
    fn parse(bytes: &[u8]) -> Self {
        let mut bom = false;
        let mut body = bytes;
        if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
            bom = true;
            body = rest;
        }
        let mut lines: Vec<Vec<u8>> = Vec::new();
        for part in body.split(|&b| b == b'\n') {
            let line = part.strip_suffix(b"\r").unwrap_or(part);
            lines.push(line.to_vec());
        }
        // 文档是否以换行结尾（split 会多出一个空行）。
        if body.ends_with(b"\n") && lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        let eol: &'static [u8] = if body.contains(&b'\r') { b"\r\n" } else { b"\n" };
        let trailing_newline = body.ends_with(b"\n");
        Doc {
            lines,
            eol,
            bom,
            trailing_newline,
        }
    }

    /// 从根依次下钻块链；任一祖先缺失 → Ok(None)（根缺失也归此）。无 IO。
    fn descend(&self, path: &[&[u8]]) -> Result<Option<Block>, VdfError> {
        let mut from = 0usize;
        let mut current: Option<Block> = None;
        for &key in path {
            current = self.find_child_block_from(from, key);
            let Some(block) = current else {
                return Ok(None);
            };
            from = block.open + 1;
        }
        Ok(current)
    }

    /// 在父块内按名找子块（限定在父块的开闭括号之间扫描）。
    fn find_child_block(&self, parent: Block, key: &[u8]) -> Option<Block> {
        self.find_child_block_from(parent.open + 1, key)
    }

    /// 从指定行起按名找「键行 + 紧随开括号」的块。
    fn find_child_block_from(&self, from: usize, key: &[u8]) -> Option<Block> {
        let mut i = from;
        while i < self.lines.len() {
            match classify(&self.lines[i]) {
                Some((LineKind::OpenKey, k, _, indent)) if k == key => {
                    // 紧邻的下一个有效行必须是 `{`。
                    let open = self.next_meaningful(i + 1)?;
                    // 紧邻行（去前导 tab）必须是 `{`；否则视为普通键行继续扫描。
                    let next_is_open = classify(&self.lines[open])
                        .is_some_and(|(kind, b, _, _)| matches!(kind, LineKind::Brace) && b == b"{");
                    if !next_is_open {
                        i += 1;
                        continue;
                    }
                    let close = self.match_close(open)?;
                    return Some(Block {
                        open,
                        close,
                        indent,
                    });
                }
                Some((LineKind::Brace, other, _, _)) => {
                    // 错过一个闭合块：整块跳过，避免把嵌套同名键误认。
                    if other == b"}" {
                        return None; // 越过父块边界：停止。
                    }
                    if other == b"{" {
                        let close = self.match_close(i)?;
                        i = close + 1;
                        continue;
                    }
                }
                _ => i += 1,
            }
        }
        None
    }

    /// 下一个非空/非注释行号。
    fn next_meaningful(&self, from: usize) -> Option<usize> {
        (from..self.lines.len()).find(|&i| classify(&self.lines[i]).is_some())
    }

    /// 找与 open 括号匹配的闭括号行号。
    fn match_close(&self, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for i in open..self.lines.len() {
            let Some((LineKind::Brace, b, _, _)) = classify(&self.lines[i]) else {
                continue;
            };
            if b == b"{" {
                depth += 1;
            } else if b == b"}" {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    None
    }

    /// 块内找键值行（`"key"\t\t"value"`）。
    fn find_kv(&self, block: Block, key: &[u8]) -> Option<usize> {
        let mut i = block.open + 1;
        while i < block.close {
            if let Some((LineKind::Kv, k, _, _)) = classify(&self.lines[i])
                && k == key
            {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// 在指定行前插入字节行，同时把其后所有行号整体后移（本实现按需返回即可）。
    fn insert_lines_before(&mut self, at: usize, lines_to_insert: &[Vec<u8>]) {
        let mut tail: Vec<Vec<u8>> = self.lines.split_off(at);
        self.lines.extend_from_slice(lines_to_insert);
        self.lines.append(&mut tail);
    }

    /// 在父块末尾插入空子块（`"key"` / `{` / `}`），返回新块的定位。
    /// 插入会移动父块 close 行号，返回前重新按名定位。
    fn insert_empty_block(&mut self, parent: &Block, key: &[u8]) -> Block {
        let key = String::from_utf8_lossy(key).into_owned();
        let lines = vec![
            format!("{}\"{key}\"", indent_tabs(parent.indent + 1)).into_bytes(),
            format!("{}{{", indent_tabs(parent.indent + 1)).into_bytes(),
            format!("{}}}", indent_tabs(parent.indent + 1)).into_bytes(),
        ];
        self.insert_lines_before(parent.close, &lines);
        self.find_child_block(*parent, key.as_bytes())
            .expect("刚插入的空块必可定位")
    }

    /// 改写行值：保留前导缩进 + `"key"`，仅替换引号值。
    fn rewrite_value(&mut self, line_idx: usize, new_value: &str) {
        let line = &self.lines[line_idx];
        let (_, key, _, indent) = classify(line).expect("rewrite 目标必须是键值行");
        let key = key.to_vec();
        let mut rebuilt = Vec::new();
        rebuilt.extend_from_slice(indent_tabs(indent).as_bytes());
        rebuilt.push(b'"');
        rebuilt.extend_from_slice(&key);
        rebuilt.push(b'"');
        rebuilt.extend_from_slice(b"\t\t\"");
        rebuilt.extend_from_slice(escape_value(new_value).as_bytes());
        rebuilt.push(b'"');
        self.lines[line_idx] = rebuilt;
    }

    /// 删除一行（clear 清空选项行时用）。
    fn remove_line(&mut self, line_idx: usize) {
        self.lines.remove(line_idx);
    }

    /// 序列化并原子写盘（临时文件 + rename；写前调用方已备份）。
    fn write(&self, path: &Path) -> io::Result<()> {
        let mut bytes = Vec::with_capacity(self.lines.len() * 48 + 8);
        if self.bom {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        for (i, line) in self.lines.iter().enumerate() {
            bytes.extend_from_slice(line);
            if i + 1 < self.lines.len() {
                bytes.extend_from_slice(self.eol);
            }
        }
        if self.trailing_newline && !self.lines.is_empty() {
            bytes.extend_from_slice(self.eol);
        }
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let tmp = dir.join(format!(
            ".{}.tmp-{}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("localconfig"),
            std::process::id()
        ));
        fs::write(&tmp, &bytes)?;
        match fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost-of-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 构造一个最小但结构完整的 localconfig.vdf（LF）。
    fn sample_vdf(launch: Option<&str>) -> String {
        let mut s = String::from("\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"Apps\"\n\t\t\t\t{\n\t\t\t\t\t\"1361510\"\n\t\t\t\t\t{\n");
        if let Some(v) = launch {
            s.push_str(&format!("\t\t\t\t\t\t\"LaunchOptions\"\t\t\"{v}\"\n"));
        }
        s.push_str("\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n");
        s
    }

    // ---------- 词法 ----------

    #[test]
    fn classify_kv_and_openkey() {
        let (kind, key, val, indent) = classify(b"\t\t\"LaunchOptions\"\t\t\"-console\"").unwrap();
        assert!(matches!(kind, LineKind::Kv));
        assert_eq!(key, b"LaunchOptions");
        assert_eq!(val.unwrap(), b"-console");
        assert_eq!(indent, 2);

        let (kind, key, val, indent) = classify(b"\"Apps\"").unwrap();
        assert!(matches!(kind, LineKind::OpenKey));
        assert_eq!(key, b"Apps");
        assert!(val.is_none());
        assert_eq!(indent, 0);
    }

    #[test]
    fn classify_skips_comment_empty_brace() {
        assert!(classify(b"").is_none());
        assert!(classify(b"// comment").is_none());
        assert!(classify(b"\t// comment").is_none());
        let (kind, b, _, _) = classify(b"{").unwrap();
        assert!(matches!(kind, LineKind::Brace));
        assert_eq!(b, b"{");
    }

    #[test]
    fn quoted_value_handles_escaped_quote() {
        let (_, _, val, _) = classify(b"\"x\"\t\t\"a \\\"b\\\" c\"").unwrap();
        // 原始内字节（含转义）——不unescape，逐字保留。
        assert_eq!(val.unwrap(), b"a \\\"b\\\" c");
    }

    // ---------- 令牌操作 ----------

    #[test]
    fn token_ops_respect_quoted_paths() {
        assert!(has_token("-console -onlinefix", "-onlinefix"));
        assert!(has_token("\"-x y\" -onlinefix", "-onlinefix"));
        assert!(!has_token("-console", "-onlinefix"));
        // 带引号含空格片段保持整体。
        assert_eq!(
            add_token("\"C:/My Game/game.exe\" -beta beta", "-onlinefix"),
            "\"C:/My Game/game.exe\" -beta beta -onlinefix"
        );
        assert_eq!(add_token(" -onlinefix ", "-onlinefix"), "-onlinefix");
        assert_eq!(remove_token("-console -onlinefix", "-onlinefix"), "-console");
        assert_eq!(remove_token("-onlinefix", "-onlinefix"), "");
        assert_eq!(
            remove_token("\"a b\" -onlinefix c", "-onlinefix"),
            "\"a b\" c"
        );
    }

    // ---------- 读取 ----------

    #[test]
    fn read_returns_value_or_none() {
        let dir = tmp_dir("read");
        let vdf = dir.join("localconfig.vdf");
        fs::write(&vdf, sample_vdf(Some("-console -onlinefix"))).unwrap();

        assert_eq!(
            read_launch_options(&vdf, 1361510).unwrap().as_deref(),
            Some("-console -onlinefix")
        );
        // 未设置的 AppID → None。
        assert_eq!(read_launch_options(&vdf, 9999).unwrap(), None);

        fs::write(&vdf, sample_vdf(None)).unwrap();
        assert_eq!(read_launch_options(&vdf, 1361510).unwrap(), None);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_missing_paths_is_ok_none() {
        let dir = tmp_dir("readnone");
        let vdf = dir.join("localconfig.vdf");
        // 无 Apps 的退化文件。
        fs::write(&vdf, "\"UserLocalConfigStore\"\n{\n}\n").unwrap();
        assert_eq!(read_launch_options(&vdf, 1361510).unwrap(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    // ---------- 设置 ----------

    #[test]
    fn set_appends_and_dedups() {
        let dir = tmp_dir("set");
        let vdf = dir.join("localconfig.vdf");
        fs::write(&vdf, sample_vdf(Some("-console"))).unwrap();

        set_onlinefix(&vdf, 1361510).unwrap();
        assert_eq!(
            read_launch_options(&vdf, 1361510).unwrap().as_deref(),
            Some("-console -onlinefix")
        );

        // 幂等：再次设置不重复。
        let before = fs::read(&vdf).unwrap();
        set_onlinefix(&vdf, 1361510).unwrap();
        assert_eq!(
            read_launch_options(&vdf, 1361510).unwrap().as_deref(),
            Some("-console -onlinefix")
        );
        assert_eq!(fs::read(&vdf).unwrap(), before, "幂等不应改写文件");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_creates_appid_block_when_missing() {
        let dir = tmp_dir("setcreate");
        let vdf = dir.join("localconfig.vdf");
        fs::write(&vdf, sample_vdf(None)).unwrap();

        set_onlinefix(&vdf, 9999).unwrap();
        assert_eq!(
            read_launch_options(&vdf, 9999).unwrap().as_deref(),
            Some("-onlinefix")
        );
        // 原有 1361510 不受影响。
        assert_eq!(read_launch_options(&vdf, 1361510).unwrap(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_creates_apps_chain_when_missing() {
        let dir = tmp_dir("setchain");
        let vdf = dir.join("localconfig.vdf");
        // 只有根块、无 Software 链。
        fs::write(
            &vdf,
            "\"UserLocalConfigStore\"\n{\n\t\"Something\"\t\t\"1\"\n}\n",
        )
        .unwrap();

        set_onlinefix(&vdf, 1361510).unwrap();
        assert_eq!(
            read_launch_options(&vdf, 1361510).unwrap().as_deref(),
            Some("-onlinefix")
        );
        // 原内容逐字保留。
        let text = String::from_utf8_lossy(&fs::read(&vdf).unwrap()).into_owned();
        assert!(text.contains("\"Something\"\t\t\"1\""));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_missing_root_block_errors() {
        let dir = tmp_dir("setroot");
        let vdf = dir.join("localconfig.vdf");
        fs::write(&vdf, "\"OtherRoot\"\n{\n}\n").unwrap();
        assert!(matches!(set_onlinefix(&vdf, 1361510), Err(VdfError::Structure(_))));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_preserves_unrelated_lines_verbatim() {
        let dir = tmp_dir("verbatim");
        let vdf = dir.join("localconfig.vdf");
        // 字节级构造 before：样本 + 第二个 appid 块 + 含非法 UTF-8 字节（模拟 GBK 路径值）的键值行。
        let mut bytes = sample_vdf(Some("-console")).into_bytes();
        let needle = b"\t\t\t\t\t\"1361510\"";
        let pos = bytes
            .windows(needle.len())
            .position(|w| w == needle)
            .expect("锚点 1361510 键行存在");
        let mut inject = Vec::new();
        inject.extend_from_slice(b"\t\t\t\t\t\"9999\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LastPlayed\"\t\t\"123\"\n\t\t\t\t\t}\n");
        inject.extend_from_slice(b"\t\t\t\t\t\"GBKKey\"\t\t\"");
        inject.extend_from_slice(&[0xFF, 0xFE]);
        inject.extend_from_slice(b"\"\n");
        bytes.splice(pos..pos, inject);
        fs::write(&vdf, &bytes).unwrap();
        let before = fs::read(&vdf).unwrap();

        set_onlinefix(&vdf, 1361510).unwrap();

        let after = fs::read(&vdf).unwrap();
        let bl: Vec<&[u8]> = before.split(|&b| b == b'\n').collect();
        let al: Vec<&[u8]> = after.split(|&b| b == b'\n').collect();
        assert_eq!(bl.len(), al.len(), "不应增删行");
        let mut changed = 0;
        for (b, a) in bl.iter().zip(al.iter()) {
            if b != a {
                changed += 1;
                assert!(a.windows(b"-onlinefix".len()).any(|w| w == b"-onlinefix"));
            }
        }
        assert_eq!(changed, 1, "仅目标行应被改写");
        assert!(after.windows(2).any(|w| w == [0xFF, 0xFE]));
        let _ = fs::remove_dir_all(&dir);
    }

    // ---------- 清除 ----------

    #[test]
    fn clear_removes_token_and_line_when_empty() {
        let dir = tmp_dir("clear");
        let vdf = dir.join("localconfig.vdf");

        // 情形 1：移除令牌，保留其余选项。
        fs::write(&vdf, sample_vdf(Some("-console -onlinefix"))).unwrap();
        clear_onlinefix(&vdf, 1361510).unwrap();
        assert_eq!(
            read_launch_options(&vdf, 1361510).unwrap().as_deref(),
            Some("-console")
        );

        // 情形 2：仅剩 -onlinefix → 清空后删除该行。
        fs::write(&vdf, sample_vdf(Some("-onlinefix"))).unwrap();
        clear_onlinefix(&vdf, 1361510).unwrap();
        assert_eq!(read_launch_options(&vdf, 1361510).unwrap(), None);
        let text = String::from_utf8_lossy(&fs::read(&vdf).unwrap()).into_owned();
        assert!(!text.contains("LaunchOptions"));

        // 情形 3：未设置的 AppID：无操作且不写盘。
        let before = fs::read(&vdf).unwrap();
        clear_onlinefix(&vdf, 55555).unwrap();
        assert_eq!(fs::read(&vdf).unwrap(), before);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_missing_chain_noop() {
        let dir = tmp_dir("clearnone");
        let vdf = dir.join("localconfig.vdf");
        fs::write(&vdf, "\"UserLocalConfigStore\"\n{\n}\n").unwrap();
        let before = fs::read(&vdf).unwrap();
        clear_onlinefix(&vdf, 1361510).unwrap();
        assert_eq!(fs::read(&vdf).unwrap(), before, "不应创建链或写盘");
        let _ = fs::remove_dir_all(&dir);
    }

    // ---------- 备份 / 换行 / 扫描 ----------

    #[test]
    fn backup_creates_timestamped_copy() {
        let dir = tmp_dir("backup");
        let vdf = dir.join("localconfig.vdf");
        fs::write(&vdf, sample_vdf(Some("-onlinefix"))).unwrap();
        let bak = backup(&vdf).unwrap();
        assert!(bak.exists());
        assert!(bak
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("localconfig.vdf.bak-"));
        assert_eq!(fs::read(&bak).unwrap(), fs::read(&vdf).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crlf_and_bom_roundtrip() {
        let dir = tmp_dir("crlf");
        let vdf = dir.join("localconfig.vdf");
        // BOM + CRLF 文件。
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(sample_vdf(None).replace('\n', "\r\n").as_bytes());
        fs::write(&vdf, &bytes).unwrap();

        set_onlinefix(&vdf, 1361510).unwrap();
        let after = fs::read(&vdf).unwrap();
        assert!(after.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM 保留");
        let text = String::from_utf8_lossy(&after).into_owned();
        assert!(text.contains("\r\n"), "CRLF 保留");
        assert!(text.contains("\"LaunchOptions\"\t\t\"-onlinefix\""));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_lua_appids_skips_comments_and_dedups() {
        let dir = tmp_dir("lua");
        let lua = dir.join("config").join("lua");
        fs::create_dir_all(&lua).unwrap();
        // 混入 GBK 垃圾/chunk 的注释行 + 大小写变体 + 注释掉的 addappid。
        fs::write(
            lua.join("a.lua"),
            "--BI AO 涓绘父鎴
addappid(1030300)
addappid(1030301,0,\"deadbeef\")
AddAppId(3928720)
ADDAPPID(3928722)
--addappid(999999) 注释掉的应跳过
",
        )
        .unwrap();
        fs::write(lua.join("b.txt"), "addappid(777)").unwrap(); // 非 .lua 忽略
        fs::write(lua.join("b.lua"), "addappid(1030300)\n//addappid(666)\n").unwrap();

        let ids = scan_lua_appids(&dir);
        assert_eq!(ids, vec![1030300, 1030301, 3928720, 3928722]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn account_vdf_paths_enumerates_existing() {
        let dir = tmp_dir("accounts");
        let a = dir.join("userdata").join("111").join("config");
        let b = dir.join("userdata").join("222").join("config");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("localconfig.vdf"), sample_vdf(None)).unwrap();
        // 222 无 vdf。

        let paths = account_vdf_paths(&dir);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("111\\config\\localconfig.vdf"));
        let _ = fs::remove_dir_all(&dir);
    }
}