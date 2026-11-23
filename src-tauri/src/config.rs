//! SSH Config 可视化编辑（PRD 3.1.3）
//!
//! 能力：
//! 1. 解析 `~/.ssh/config` 为结构化 Host 列表（保留非托管内容）
//! 2. 智能校验：Host 重复、IdentityFile 不存在、端口格式、User 合法性
//! 3. 安全保存：写入前自动备份，支持一键回滚；保存后自动 600 权限
//! 4. Raw / Form 双模式：Raw 模式直接编辑全文，Form 模式仅改动托管块
//!
//! 设计约束（PRD 7.2）：只碰 `~/.ssh/config`，不做任何网络请求。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::env;
use crate::error::{KhError, KhResult};
use crate::paths;
use crate::perms;
use crate::time_util;

/// 托管块标记：由墨钥写入的 Host 段落会被这两行包裹，便于精准替换
const MGR_BEGIN: &str = "# ─── KeyHub managed block begin ───";
const MGR_END: &str = "# ─── KeyHub managed block end ───";

/// 单个 Host 的托管字段
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostEntry {
    /// 唯一 id（前端用，写入时忽略），格式 `h_<timestamp>_<idx>`
    #[serde(default)]
    pub id: String,
    /// Host 别名（必填）
    pub alias: String,
    /// HostName，可空
    #[serde(default)]
    pub hostname: String,
    /// 端口，0 或空表示未显式配置
    #[serde(default)]
    pub port: u32,
    /// 登录用户
    #[serde(default)]
    pub user: String,
    /// IdentityFile 路径（可能含 ~）
    #[serde(default)]
    pub identity: String,
    /// 备注（写进注释，仅托管块支持）
    #[serde(default)]
    pub note: String,
    /// 额外参数：本行原文里未被识别的键值对，原样保留
    #[serde(default)]
    pub extra: Vec<[String; 2]>,
    /// 该 Host 是否由墨钥托管（决定 Form 模式能否改写）
    #[serde(default)]
    pub managed: bool,
    /// 在原文中的起始行（0 基，仅解析结果有效）
    #[serde(default)]
    pub line: usize,
}

/// 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDoc {
    /// 是否存在 config 文件
    pub exists: bool,
    /// 绝对路径
    pub path: String,
    /// 美化路径
    pub pretty_path: String,
    /// 权限串
    pub perm_display: Option<String>,
    /// 权限是否合规（未加密的 config 允许 600 / 644，但 600 更佳）
    pub perm_compliant: bool,
    /// 结构化 Host 列表
    pub hosts: Vec<HostEntry>,
    /// 文件全文（Raw 模式编辑用）
    pub raw: String,
    /// 是否有墨钥托管块
    pub has_managed_block: bool,
    /// 全局段（Host * 之后、第一个具体 Host 之前的顶层指令）原文行
    pub globals: Vec<String>,
}

/// 校验问题
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckIssue {
    /// `error` | `warn` | `info`
    pub level: String,
    /// 关联的 Host 别名，空表示全局
    #[serde(default)]
    pub alias: String,
    /// 问题描述
    pub message: String,
    /// 修复建议
    #[serde(default)]
    pub hint: String,
}

/// 备份条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigBackup {
    /// 备份文件名
    pub name: String,
    /// 绝对路径
    pub path: String,
    /// 美化路径
    pub pretty_path: String,
    /// 触发原因：save / rollback / edit
    pub reason: String,
    /// 创建时间 RFC3339
    pub created_at: String,
    /// 文件大小
    pub size: u64,
    /// 行数
    pub lines: usize,
}

/// 保存结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    /// 备份文件名（用于提示"已备份为 xxx"）
    pub backup: Option<String>,
    /// 写入后的行数
    pub lines: usize,
    /// 写入后的字节数
    pub bytes: usize,
}

// ─────────────────────────── 解析 ───────────────────────────

/// 读取并解析 config。文件不存在时返回空骨架而非报错。
pub fn read_config() -> KhResult<ConfigDoc> {
    let path = paths::ssh_config_path()?;
    let pretty = paths::pretty(&path);

    if !path.exists() {
        return Ok(ConfigDoc {
            exists: false,
            path: path.to_string_lossy().to_string(),
            pretty_path: pretty,
            perm_display: None,
            perm_compliant: true,
            hosts: Vec::new(),
            raw: String::new(),
            has_managed_block: false,
            globals: Vec::new(),
        });
    }

    let raw = std::fs::read_to_string(&path)?;
    let (hosts, globals) = parse_hosts(&raw)?;
    let st = perms::stat(&path, false).ok();
    let has_managed = raw.contains(MGR_BEGIN);

    Ok(ConfigDoc {
        exists: true,
        path: path.to_string_lossy().to_string(),
        pretty_path: pretty,
        perm_display: st.as_ref().map(|s| s.display.clone()),
        perm_compliant: st.as_ref().map(|s| s.compliant).unwrap_or(true),
        hosts,
        raw,
        has_managed_block: has_managed,
        globals,
    })
}

/// 把 config 文本解析为 Host 列表 + 全局段。
///
/// 解析规则（对齐 OpenSSH）：
/// - `Host` 行开始一个新段落，`Host a b` 取第一个别名作为展示别名，其余进 extra
/// - 指令大小写不敏感，`Key = Value` 与 `Key Value` 都接受
/// - `#` 开头为注释；行尾 `#` 不视为注释（OpenSSH 也不支持行尾注释）
/// - `Match` 段整段归入 extra，不拆解
pub fn parse_hosts(raw: &str) -> KhResult<(Vec<HostEntry>, Vec<String>)> {
    let mut hosts: Vec<HostEntry> = Vec::new();
    let mut globals: Vec<String> = Vec::new();
    let mut cur: Option<HostEntry> = None;
    let mut in_managed = false;
    let mut pending_note = String::new();

    for (idx, line) in raw.lines().enumerate() {
        let trimmed = line.trim();

        // 托管块标记
        if trimmed == MGR_BEGIN {
            in_managed = true;
            continue;
        }
        if trimmed == MGR_END {
            in_managed = false;
            continue;
        }

        // 注释行：托管块内 `# 别名 · 备注 · 由墨钥管理` 作为备注来源
        if trimmed.starts_with('#') {
            if in_managed {
                if let Some(rest) = trimmed.trim_start_matches('#').trim().strip_prefix("───") {
                    let body = rest.trim().trim_end_matches("───").trim();
                    // 形如 `github-work · 工作账号 · 由墨钥管理`
                    let parts: Vec<&str> = body.split('·').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        pending_note = parts[1].to_string();
                    }
                }
            }
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        let (key, value) = split_kv(trimmed);

        if key.eq_ignore_ascii_case("host") {
            // 落库上一个
            if let Some(h) = cur.take() {
                hosts.push(h);
            }
            if value.trim() == "*" {
                // 通配段：作为全局段处理，不进入 Host 列表
                cur = None;
                globals.push(line.to_string());
                continue;
            }
            let mut aliases: Vec<String> = value.split_whitespace().map(|s| s.to_string()).collect();
            let alias = aliases.first().cloned().unwrap_or_default();
            let mut extra: Vec<[String; 2]> = Vec::new();
            for a in aliases.drain(1..) {
                extra.push(["HostAlias".into(), a]);
            }
            let note = std::mem::take(&mut pending_note);
            cur = Some(HostEntry {
                id: format!("h_{}_{}", idx, hosts.len()),
                alias,
                managed: in_managed,
                note,
                extra,
                line: idx,
                ..Default::default()
            });
            continue;
        }

        // 无 Host 上下文 → 全局指令
        let Some(h) = cur.as_mut() else {
            globals.push(line.to_string());
            continue;
        };

        match key.to_ascii_lowercase().as_str() {
            "hostname" => h.hostname = value.to_string(),
            "port" => h.port = value.trim().parse::<u32>().unwrap_or(0),
            "user" => h.user = value.to_string(),
            "identityfile" => h.identity = value.to_string(),
            _ => h.extra.push([key.to_string(), value.to_string()]),
        }
    }

    if let Some(h) = cur.take() {
        hosts.push(h);
    }

    Ok((hosts, globals))
}

/// 拆 `Key Value` / `Key=Value` / `Key = Value`
fn split_kv(line: &str) -> (&str, &str) {
    if let Some(eq) = line.find('=') {
        let k = line[..eq].trim();
        let v = line[eq + 1..].trim();
        // 只有 key 部分不含空格时才认定是 `=` 形式，避免误切 `ProxyCommand x=y`
        if !k.contains(' ') && !k.is_empty() {
            return (k, v);
        }
    }
    match line.find(char::is_whitespace) {
        Some(sp) => (line[..sp].trim(), line[sp..].trim()),
        None => (line.trim(), ""),
    }
}

/// 生成结构化 Host 列表对应的托管块文本
pub fn render_managed_block(hosts: &[HostEntry]) -> String {
    let mut out = String::new();
    out.push_str(MGR_BEGIN);
    out.push('\n');
    for h in hosts {
        let note = if h.note.trim().is_empty() {
            "由墨钥管理"
        } else {
            &h.note
        };
        out.push_str(&format!("# ─── {} · {} · 由墨钥管理\n", h.alias, note));
        out.push_str(&format!("Host {}\n", h.alias));
        if !h.hostname.trim().is_empty() {
            out.push_str(&format!("    HostName {}\n", h.hostname.trim()));
        }
        if h.port > 0 {
            out.push_str(&format!("    Port {}\n", h.port));
        }
        if !h.user.trim().is_empty() {
            out.push_str(&format!("    User {}\n", h.user.trim()));
        }
        if !h.identity.trim().is_empty() {
            out.push_str(&format!("    IdentityFile {}\n", h.identity.trim()));
            out.push_str("    IdentitiesOnly yes\n");
        }
        for pair in &h.extra {
            if pair[0] == "HostAlias" {
                continue;
            }
            if pair[0].is_empty() {
                continue;
            }
            out.push_str(&format!("    {} {}\n", pair[0], pair[1]));
        }
    }
    out.push_str(MGR_END);
    out.push('\n');
    out
}

/// 用新的托管块替换原文中的旧托管块；没有旧块则追加到末尾。
pub fn apply_managed(raw: &str, hosts: &[HostEntry]) -> String {
    let block = render_managed_block(hosts);
    let Some(b) = raw.find(MGR_BEGIN) else {
        // 无托管块：追加
        let mut base = raw.trim_end().to_string();
        if !base.is_empty() {
            base.push_str("\n\n");
        }
        base.push_str(&block);
        return base;
    };
    let Some(e) = raw[b..].find(MGR_END) else {
        // 有开始没结束：整段替换到文件尾
        let mut head = raw[..b].to_string();
        if !head.is_empty() && !head.ends_with('\n') {
            head.push('\n');
        }
        head.push_str(&block);
        return head;
    };
    let end = b + e + MGR_END.len();
    // 吃掉结束标记后紧跟的一个换行
    let mut end2 = end;
    if raw[end2..].starts_with("\r\n") {
        end2 += 2;
    } else if raw[end2..].starts_with('\n') {
        end2 += 1;
    }
    let mut out = String::with_capacity(raw.len());
    out.push_str(&raw[..b]);
    out.push_str(&block);
    out.push_str(&raw[end2..]);
    out
}

// ─────────────────────────── 校验 ───────────────────────────

/// 智能校验（PRD 3.1.3「智能校验」）
pub fn validate_hosts(hosts: &[HostEntry]) -> Vec<CheckIssue> {
    let mut issues = Vec::new();
    let ssh = paths::ssh_dir().ok();

    // 1) 别名唯一性与合法性
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for h in hosts {
        let a = h.alias.trim();
        if a.is_empty() {
            issues.push(CheckIssue {
                level: "error".into(),
                alias: String::new(),
                message: "存在未填写别名的 Host".into(),
                hint: "别名是 config 的索引键，必须填写，例如 github-work".into(),
            });
            continue;
        }
        if a.contains('*') || a.contains('?') || a.contains('!') {
            issues.push(CheckIssue {
                level: "warn".into(),
                alias: a.into(),
                message: format!("别名 `{}` 含通配符，可能与其它 Host 产生匹配冲突", a),
                hint: "如需通配匹配，建议改为显式的具体别名并在应用层区分".into(),
            });
        }
        if a.contains(char::is_whitespace) {
            issues.push(CheckIssue {
                level: "error".into(),
                alias: a.into(),
                message: format!("别名 `{}` 含空格，OpenSSH 会将其拆成多个模式", a),
                hint: "使用连字符或下划线代替空格，例如 my-server".into(),
            });
        }
        *seen.entry(a.to_ascii_lowercase()).or_insert(0) += 1;
    }
    for (alias, n) in seen.iter() {
        if *n > 1 {
            issues.push(CheckIssue {
                level: "error".into(),
                alias: alias.clone(),
                message: format!("Host 重复：`{}` 出现了 {} 次", alias, n),
                hint: "OpenSSH 只采用第一个匹配段，重复项会让后续配置静默失效，请删除或重命名".into(),
            });
        }
    }

    for h in hosts {
        let a = h.alias.trim();

        // 2) 端口格式
        if h.port > 65535 {
            issues.push(CheckIssue {
                level: "error".into(),
                alias: a.into(),
                message: format!("端口 {} 超出 1-65535 范围", h.port),
                hint: "改为 22 或服务实际监听端口".into(),
            });
        }

        // 3) 主机名
        if h.hostname.trim().is_empty() {
            issues.push(CheckIssue {
                level: "warn".into(),
                alias: a.into(),
                message: "未配置 HostName，将直接按别名解析域名".into(),
                hint: "若别名只是代号，请补上真实域名或 IP".into(),
            });
        } else if h.hostname.contains(' ') {
            issues.push(CheckIssue {
                level: "error".into(),
                alias: a.into(),
                message: "HostName 含空格".into(),
                hint: "去掉空格，IP 或域名都不能带空格".into(),
            });
        }

        // 4) 用户
        if h.user.trim().is_empty() {
            issues.push(CheckIssue {
                level: "info".into(),
                alias: a.into(),
                message: "未配置 User，将使用当前系统用户名".into(),
                hint: "云端服务器通常需要显式指定，如 root / ubuntu".into(),
            });
        } else if h.user.contains('@') {
            issues.push(CheckIssue {
                level: "error".into(),
                alias: a.into(),
                message: "User 里出现了 @ 符号".into(),
                hint: "User 只填用户名，主机名写在 HostName 字段".into(),
            });
        }

        // 5) IdentityFile 存在性
        let idf = h.identity.trim();
        if idf.is_empty() {
            issues.push(CheckIssue {
                level: "warn".into(),
                alias: a.into(),
                message: "未指定 IdentityFile，将回退到默认密钥或口令登录".into(),
                hint: "建议显式指定密钥，避免认证走到不确定的默认项".into(),
            });
        } else {
            let expanded = paths::expand(idf);
            let found = match expanded {
                Ok(p) if p.is_file() => true,
                _ => ssh
                    .as_ref()
                    .map(|d| {
                        let name = idf.rsplit(['/', '\\']).next().unwrap_or(idf);
                        d.join(name).is_file()
                    })
                    .unwrap_or(false),
            };
            if !found {
                issues.push(CheckIssue {
                    level: "error".into(),
                    alias: a.into(),
                    message: format!("IdentityFile 指向的密钥不存在：{}", idf),
                    hint: "检查路径拼写，或先在密钥库中生成该密钥".into(),
                });
            }
        }
    }

    issues
}

// ─────────────────────────── 备份 / 保存 ───────────────────────────

/// 备份当前 config，返回备份文件名。config 不存在时返回 None。
fn backup_current(reason: &str) -> KhResult<Option<String>> {
    let src = paths::ssh_config_path()?;
    if !src.is_file() {
        return Ok(None);
    }
    let dir = paths::config_backup_dir()?;
    let stamp = time_util::file_stamp();
    let name = format!("config.{}.{}.bak", stamp, reason);
    let dst = dir.join(&name);
    std::fs::copy(&src, &dst)?;
    Ok(Some(name))
}

/// 保存 config 全文。
///
/// 流程（PRD 3.1.3「安全保存」）：
/// 1. 校验内容可解析
/// 2. 自动备份现有文件
/// 3. 原子写入（先写临时文件再 rename，避免中途断电损坏 config）
/// 4. 修复权限为 600
pub fn write_config(content: &str) -> KhResult<SaveResult> {
    // 1) 内容自检：能被解析，且不存在致命结构问题
    let (hosts, _) = parse_hosts(content)?;
    let issues = validate_hosts(&hosts);
    let fatal: Vec<&CheckIssue> = issues.iter().filter(|i| i.level == "error").collect();
    if !fatal.is_empty() {
        // 一次性列出全部错误，并带上 Host 别名与修复建议：
        // 只返回第一条、或只说「校验未通过」，用户无法知道该改哪个 Host。
        let detail = fatal
            .iter()
            .map(|i| {
                let who = if i.alias.trim().is_empty() {
                    String::new()
                } else {
                    format!("Host「{}」：", i.alias.trim())
                };
                if i.hint.is_empty() {
                    format!("{}{}", who, i.message)
                } else {
                    format!("{}{}。{}", who, i.message, i.hint)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Err(KhError::InvalidInput(format!(
            "配置未通过校验（{} 个错误）：\n{}",
            fatal.len(),
            detail
        )));
    }

    // 2) 备份
    let backup = backup_current("save")?;

    // 3) 原子写入
    let path = paths::ssh_config_path()?;
    let dir = path
        .parent()
        .ok_or_else(|| KhError::InvalidInput("config 路径异常".into()))?;
    paths::ensure_dir(dir)?;
    let tmp = dir.join(".config.keyhub.tmp");
    std::fs::write(&tmp, content.as_bytes())?;
    // Windows 上 rename 到已存在文件会失败，先移除目标
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        KhError::Io(e.to_string())
    })?;

    // 4) 权限（config 是文件 → 600）
    let _ = perms::fix(&path, false);

    Ok(SaveResult {
        backup,
        lines: content.lines().count(),
        bytes: content.len(),
    })
}

/// 保存结构化 Host 列表（Form 模式）：只重写托管块，保留其它内容。
pub fn write_hosts(hosts: &[HostEntry]) -> KhResult<SaveResult> {
    let current = read_config()?;
    let updated = apply_managed(&current.raw, hosts);
    write_config(&updated)
}

/// 列出所有 config 备份（按时间倒序）
pub fn list_backups() -> KhResult<Vec<ConfigBackup>> {
    let dir = paths::config_backup_dir()?;
    let mut out: Vec<ConfigBackup> = Vec::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return Ok(out),
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("config.") || !name.ends_with(".bak") {
            continue;
        }
        let lines = std::fs::read_to_string(&p)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        out.push(ConfigBackup {
            name: name.clone(),
            path: p.to_string_lossy().to_string(),
            pretty_path: paths::pretty(&p),
            reason: name
                .trim_end_matches(".bak")
                .rsplit('.')
                .next()
                .unwrap_or("save")
                .to_string(),
            created_at: time_util::file_time_rfc3339(&p).unwrap_or_default(),
            size: std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
            lines,
        });
    }
    // 文件名内嵌时间戳，倒序即为时间倒序
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// 回滚到指定备份（PRD 3.1.3「一键回滚」）。
/// 回滚前会把当前文件另存为一份 `rollback` 备份，保证操作可逆。
pub fn rollback_config(name: &str) -> KhResult<SaveResult> {
    // 防路径穿越：备份名必须是纯文件名
    let safe = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| KhError::InvalidInput("备份名非法".into()))?;
    if safe != name {
        return Err(KhError::InvalidInput("备份名不能包含路径分隔符".into()));
    }

    let dir = paths::config_backup_dir()?;
    let src = dir.join(&safe);
    if !src.is_file() {
        return Err(KhError::NotFound(format!("备份不存在：{}", name)));
    }

    // 回滚前的现场保护
    let _ = backup_current("rollback");

    let content = std::fs::read_to_string(&src)?;
    let path = paths::ssh_config_path()?;
    let tmp = path.with_file_name(".config.keyhub.tmp");
    std::fs::write(&tmp, content.as_bytes())?;
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    std::fs::rename(&tmp, &path).map_err(|e| KhError::Io(e.to_string()))?;
    // 回滚后的 config 同样是文件 → 600
    let _ = perms::fix(&path, false);

    Ok(SaveResult {
        backup: Some(safe),
        lines: content.lines().count(),
        bytes: content.len(),
    })
}

/// 读取某个备份的全文（供预览）
pub fn read_backup(name: &str) -> KhResult<String> {
    let safe = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| KhError::InvalidInput("备份名非法".into()))?;
    if safe != name {
        return Err(KhError::InvalidInput("备份名不能包含路径分隔符".into()));
    }
    let p = paths::config_backup_dir()?.join(&safe);
    if !p.is_file() {
        return Err(KhError::NotFound(format!("备份不存在：{}", name)));
    }
    Ok(std::fs::read_to_string(&p)?)
}

/// 删除一个备份（保留用户主动清理的能力）
pub fn delete_backup(name: &str) -> KhResult<()> {
    let safe = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| KhError::InvalidInput("备份名非法".into()))?;
    if safe != name {
        return Err(KhError::InvalidInput("备份名不能包含路径分隔符".into()));
    }
    let p = paths::config_backup_dir()?.join(&safe);
    if !p.is_file() {
        return Err(KhError::NotFound(format!("备份不存在：{}", name)));
    }
    std::fs::remove_file(&p)?;
    Ok(())
}

/// 提取 config 中引用到的所有 IdentityFile（供密钥库做「被引用」标记）
pub fn referenced_identities() -> Vec<String> {
    let Ok(p) = paths::ssh_config_path() else {
        return Vec::new();
    };
    let Ok(raw) = std::fs::read_to_string(&p) else {
        return Vec::new();
    };
    let Ok((hosts, _)) = parse_hosts(&raw) else {
        return Vec::new();
    };
    hosts
        .iter()
        .filter(|h| !h.identity.trim().is_empty())
        .map(|h| h.identity.trim().to_string())
        .collect()
}

/// 生成一个可用的 Host 别名建议（避免与现有重复）
pub fn suggest_alias(base: &str) -> String {
    let Ok(raw) = std::fs::read_to_string(paths::ssh_config_path().unwrap_or_default()) else {
        return base.to_string();
    };
    let Ok((hosts, _)) = parse_hosts(&raw) else {
        return base.to_string();
    };
    let existing: std::collections::HashSet<String> = hosts
        .iter()
        .map(|h| h.alias.to_ascii_lowercase())
        .collect();
    if !existing.contains(&base.to_ascii_lowercase()) {
        return base.to_string();
    }
    for i in 2..1000 {
        let cand = format!("{}-{}", base, i);
        if !existing.contains(&cand.to_ascii_lowercase()) {
            return cand;
        }
    }
    base.to_string()
}

/// 备份目录绝对路径
pub fn backup_dir_path() -> KhResult<PathBuf> {
    paths::config_backup_dir()
}

/// 生成一份「示例 config」内容（首次使用引导）
pub fn sample_config() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "your-user".into());
    let host = env::hostname().unwrap_or_else(|| "example.com".into());
    format!(
        "# 由墨钥生成的初始配置\n\
         # 修改后会自动备份，可随时一键回滚\n\
         \n\
         Host {}\n\
         \x20   HostName {}\n\
         \x20   User {}\n\
         \x20   Port 22\n\
         \x20   IdentityFile ~/.ssh/id_ed25519\n\
         \x20   IdentitiesOnly yes\n",
        "my-server", host, user
    )
}
