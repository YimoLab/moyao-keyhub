//! 密钥可视化管理（PRD 3.1.2）
//!
//! 关键安全约束（PRD 7.1）：
//! - 墨钥不存储私钥副本，仅读取 ~/.ssh 下真实文件
//! - 私钥明文只在显式请求时读取，且每次调用都写审计日志
//! - 生成密钥调用系统原生 ssh-keygen，保证与 OpenSSH 完全兼容

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::env;
use crate::error::{KhError, KhResult};
use crate::paths;
use crate::perms::{self, FILE_MODE};

/// 单个密钥的元信息（不含私钥内容）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMeta {
    /// 文件名（不含路径、不含 .pub）
    pub filename: String,
    /// 私钥绝对路径
    pub path: String,
    /// 美化路径（~/...）
    pub pretty_path: String,
    /// 算法（ED25519 / RSA-4096 / ECDSA 等）
    pub algorithm: String,
    /// 密钥长度（RSA 才有意义）
    pub bits: Option<u32>,
    /// SHA256 指纹
    pub fingerprint: Option<String>,
    /// 公钥首字段，如 ssh-ed25519
    pub key_type: Option<String>,
    /// 注释（通常是邮箱）
    pub comment: Option<String>,
    /// 创建时间（RFC3339）
    pub created_at: Option<String>,
    /// 文件修改时间（RFC3339）
    pub modified_at: Option<String>,
    /// 文件大小（字节）
    pub size: u64,
    /// 权限串
    pub perm_display: Option<String>,
    /// 权限是否合规
    pub perm_compliant: bool,
    /// 是否设置了口令保护
    pub has_passphrase: bool,
    /// 是否存在对应 .pub
    pub has_public: bool,
}

/// 生成密钥的入参
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOpts {
    /// `ed25519` | `rsa`
    pub algorithm: String,
    /// RSA 位数（2048 / 3072 / 4096），ED25519 忽略
    #[serde(default)]
    pub bits: Option<u32>,
    /// 文件名（不含扩展名）
    pub filename: String,
    /// 注释（邮箱 / 用途）
    #[serde(default)]
    pub comment: String,
    /// 口令，留空则不设
    #[serde(default)]
    pub passphrase: Option<String>,
}

/* ═══════════════════════════════════════════════════════════════
 * 判定与解析
 * ═══════════════════════════════════════════════════════════════ */

/// 判断一个文件是否是 OpenSSH 私钥（读首行）
pub fn looks_like_private_key(path: &Path) -> bool {
    let Ok(f) = std::fs::File::open(path) else {
        return false;
    };
    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(f);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return false;
    }
    let f = first.trim();
    f.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----")
        || f.starts_with("-----BEGIN RSA PRIVATE KEY-----")
        || f.starts_with("-----BEGIN EC PRIVATE KEY-----")
        || f.starts_with("-----BEGIN DSA PRIVATE KEY-----")
        || f.starts_with("-----BEGIN PRIVATE KEY-----")
        || f.starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----")
}

/// 由公钥首行解析算法与注释
fn parse_pubkey_line(line: &str) -> (Option<String>, Option<String>, Option<u32>) {
    let mut it = line.split_whitespace();
    let kt = it.next().map(|s| s.to_string());
    let _blob = it.next();
    // 注释可能含空格，全部拼回
    let rest: Vec<&str> = it.collect();
    let comment = if rest.is_empty() {
        None
    } else {
        Some(rest.join(" "))
    };

    let bits = kt.as_deref().and_then(|t| match t {
        "ssh-rsa" => Some(2048), // 实际位数需解析 blob，这里给常见默认
        "ecdsa-sha2-nistp256" => Some(256),
        "ecdsa-sha2-nistp384" => Some(384),
        "ecdsa-sha2-nistp521" => Some(521),
        _ => None,
    });

    (kt, comment, bits)
}

/// 把密钥类型串规范为展示用算法名
fn algo_label(key_type: Option<&str>, bits: Option<u32>) -> String {
    match key_type {
        Some("ssh-ed25519") => "ED25519".into(),
        Some("ssh-rsa") => match bits {
            Some(b) => format!("RSA-{b}"),
            None => "RSA".into(),
        },
        Some(t) if t.starts_with("ecdsa-sha2-") => "ECDSA".into(),
        Some("ssh-dss") => "DSA".into(),
        Some("sk-ssh-ed25519@openssh.com") => "ED25519-SK".into(),
        Some("sk-ecdsa-sha2-nistp256@openssh.com") => "ECDSA-SK".into(),
        Some(other) => other.to_string(),
        None => "UNKNOWN".into(),
    }
}

/// 用 ssh-keygen -lf 取指纹；失败时返回 None
fn fingerprint_of(keygen: &Path, pub_or_priv: &Path) -> Option<String> {
    let out = perms::hidden_command(&keygen.to_string_lossy())
        .args(["-l", "-f", &pub_or_priv.to_string_lossy()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // 形如 "256 SHA256:xxxx comment (ED25519)"
    for tok in s.split_whitespace() {
        if let Some(idx) = tok.find("SHA256:") {
            return Some(tok[idx..].to_string());
        }
    }
    None
}

/// 判断私钥是否受口令保护。
/// 通过尝试用空口令读取私钥；若 ssh-keygen 拒绝，说明有口令。
fn detect_passphrase(keygen: &Path, priv_path: &Path) -> bool {
    // 用 fingerprint_of 直接对私钥取指纹：
    //   - 无口令私钥：直接输出指纹
    //   - 有口令私钥：ssh-keygen 会尝试读公钥文件；若公钥存在则仍能出指纹
    // 因此改用更可靠的判定：读取私钥首行后的加密标记。
    // OpenSSH 私钥在加密时 base64 载荷开头包含 ciphername（如 aes256-ctr）。
    // 更简单的稳健做法：调用 `ssh-keygen -y -P "" -f key`，空口令能导出公钥即为无口令。
    let out = perms::hidden_command(&keygen.to_string_lossy())
        .args(["-y", "-P", "", "-f", &priv_path.to_string_lossy()])
        .output();

    match out {
        Ok(o) => !o.status.success(), // 失败 = 有口令
        Err(_) => false,
    }
}

/// 从公钥文件读首行
fn read_pub_first_line(pub_path: &Path) -> Option<String> {
    std::fs::read_to_string(pub_path)
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|l| !l.is_empty())
}

/// 系统文件时间 → RFC3339
fn ts_to_rfc3339(t: std::io::Result<std::time::SystemTime>) -> Option<String> {
    let st = t.ok()?;
    let dt: chrono::DateTime<chrono::Local> = st.into();
    Some(dt.to_rfc3339())
}

/* ═══════════════════════════════════════════════════════════════
 * 扫描
 * ═══════════════════════════════════════════════════════════════ */

/// 扫描 `~/.ssh` 下全部私钥，构建元信息列表
pub fn scan_keys() -> KhResult<Vec<KeyMeta>> {
    let dir = paths::ssh_dir()?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let keygen = env::find_ssh_keygen();
    let mut out = Vec::new();

    let rd = std::fs::read_dir(&dir)?;
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();

        // 跳过公钥、已知文件
        if filename.ends_with(".pub")
            || filename.starts_with('.')
            || matches!(
                filename.as_str(),
                "known_hosts"
                    | "known_hosts.old"
                    | "authorized_keys"
                    | "config"
                    | "environment"
            )
        {
            continue;
        }

        if !looks_like_private_key(&path) {
            continue;
        }

        let meta = build_meta(&dir, &filename, &path, keygen.as_deref());
        out.push(meta);
    }

    // 按创建时间倒序（新的在前），无时间的排后
    out.sort_by(|a, b| {
        b.created_at
            .as_deref()
            .unwrap_or("")
            .cmp(a.created_at.as_deref().unwrap_or(""))
    });

    Ok(out)
}

/// 为单个密钥文件构建元信息
fn build_meta(
    dir: &Path,
    filename: &str,
    path: &Path,
    keygen: Option<&Path>,
) -> KeyMeta {
    let pub_path = dir.join(format!("{filename}.pub"));
    let has_public = pub_path.is_file();

    // 元数据
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let created_at = meta
        .as_ref()
        .and_then(|m| ts_to_rfc3339(m.created()));
    let modified_at = meta
        .as_ref()
        .and_then(|m| ts_to_rfc3339(m.modified()));

    // 权限
    let (perm_display, perm_compliant) = match perms::stat(path, false) {
        Ok(p) => (Some(p.display), p.compliant),
        Err(_) => (None, false),
    };

    // 公钥解析
    let pub_line = if has_public {
        read_pub_first_line(&pub_path)
    } else {
        None
    };
    let (key_type, comment, mut bits) = match pub_line.as_deref() {
        Some(l) => parse_pubkey_line(l),
        None => (None, None, None),
    };

    // 无公钥时用 ssh-keygen 反推（有口令的私钥拿不到，属正常）
    let (fingerprint, final_key_type) = {
        let target = if has_public { &pub_path } else { path };
        let fp = keygen.and_then(|k| fingerprint_of(k, target));
        (fp, key_type.clone())
    };

    // RSA 位数：从指纹输出里拿更准（形如 "3072 SHA256:..."）
    if final_key_type.as_deref() == Some("ssh-rsa") {
        if let (Some(k), true) = (keygen, has_public) {
            if let Some(b) = rsa_bits(k, &pub_path) {
                bits = Some(b);
            }
        }
    }

    let has_passphrase = match keygen {
        Some(k) => detect_passphrase(k, path),
        None => false,
    };

    KeyMeta {
        filename: filename.to_string(),
        path: path.to_string_lossy().to_string(),
        pretty_path: paths::pretty(path),
        algorithm: algo_label(final_key_type.as_deref(), bits),
        bits,
        fingerprint,
        key_type: final_key_type,
        comment,
        created_at,
        modified_at,
        size,
        perm_display,
        perm_compliant,
        has_passphrase,
        has_public,
    }
}

/// 从 ssh-keygen -l 输出中提取 RSA 位数
fn rsa_bits(keygen: &Path, pub_path: &Path) -> Option<u32> {
    let out = perms::hidden_command(&keygen.to_string_lossy())
        .args(["-l", "-f", &pub_path.to_string_lossy()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    s.split_whitespace().next()?.parse::<u32>().ok()
}

/* ═══════════════════════════════════════════════════════════════
 * 生成
 * ═══════════════════════════════════════════════════════════════ */

/// 调用 ssh-keygen 生成密钥（PRD 3.1.2）
pub fn generate_key(opts: &GenerateOpts) -> KhResult<KeyMeta> {
    paths::validate_filename(&opts.filename)?;
    env::ensure_ssh_dir()?;

    let dir = paths::ssh_dir()?;
    let target = dir.join(&opts.filename);
    let pub_path = dir.join(format!("{}.pub", opts.filename));

    if target.exists() {
        return Err(KhError::AlreadyExists(format!(
            "{} 已存在，请换一个文件名",
            opts.filename
        )));
    }

    let keygen = env::find_ssh_keygen().ok_or_else(|| {
        KhError::SshMissing("未找到 ssh-keygen，请先完成环境准备（见「环境自检」）".into())
    })?;

    let algo = opts.algorithm.to_lowercase();
    let (t_arg, extra): (String, Vec<String>) = match algo.as_str() {
        "ed25519" => ("ed25519".into(), vec![]),
        "rsa" => {
            let bits = opts.bits.unwrap_or(4096);
            if !matches!(bits, 2048 | 3072 | 4096) {
                return Err(KhError::InvalidInput(
                    "RSA 位数仅支持 2048 / 3072 / 4096".into(),
                ));
            }
            ("rsa".into(), vec!["-b".into(), bits.to_string()])
        }
        "ecdsa" => ("ecdsa".into(), vec!["-b".into(), "521".into()]),
        other => {
            return Err(KhError::InvalidInput(format!(
                "不支持的算法：{other}（可选 ed25519 / rsa / ecdsa）"
            )))
        }
    };

    let comment = if opts.comment.trim().is_empty() {
        format!("keyhub:{}", opts.filename)
    } else {
        opts.comment.trim().to_string()
    };

    let pass = opts
        .passphrase
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // ssh-keygen -q -t <algo> [-b bits] -C <comment> -f <path> -N <pass>
    let mut cmd = perms::hidden_command(&keygen.to_string_lossy());
    cmd.arg("-q").arg("-t").arg(&t_arg);
    for e in &extra {
        cmd.arg(e);
    }
    cmd.arg("-C")
        .arg(&comment)
        .arg("-f")
        .arg(&target)
        .arg("-N")
        .arg(pass.as_deref().unwrap_or(""));

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let out = cmd
        .output()
        .map_err(|e| KhError::CommandFailed(format!("无法执行 ssh-keygen：{e}")))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // 清理可能的半成品文件
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&pub_path);
        return Err(KhError::CommandFailed(if err.is_empty() {
            "ssh-keygen 执行失败，请检查目标路径是否可写".into()
        } else {
            format!("ssh-keygen 执行失败：{err}")
        }));
    }

    // 生成后立刻把权限收紧为标准值（PRD 3.1.4）
    let _ = perms::fix(&target, false);
    if pub_path.exists() {
        let _ = perms::fix(&pub_path, false);
    }

    debug_assert!(FILE_MODE == 0o600);

    Ok(build_meta(&dir, &opts.filename, &target, Some(&keygen)))
}

/* ═══════════════════════════════════════════════════════════════
 * 读取
 * ═══════════════════════════════════════════════════════════════ */

/// 在 `~/.ssh` 内安全解析文件名（防路径穿越）
fn resolve_in_ssh(filename: &str) -> KhResult<PathBuf> {
    let n = filename.trim();
    if n.is_empty() {
        return Err(KhError::InvalidInput("文件名不能为空".into()));
    }
    if n.contains('/') || n.contains('\\') || n.contains("..") {
        return Err(KhError::InvalidInput("文件名不合法".into()));
    }
    let dir = paths::ssh_dir()?;
    let p = dir.join(n);
    // 二次确认解析后的路径仍在 ~/.ssh 内
    if !p.starts_with(&dir) {
        return Err(KhError::InvalidInput("文件名不合法".into()));
    }
    Ok(p)
}

/// 读取单个密钥的元信息（供 agent 等模块复用）
pub fn describe_key(path: &Path) -> KhResult<KeyMeta> {
    if !path.is_file() {
        return Err(KhError::NotFound(format!(
            "密钥文件不存在：{}",
            path.to_string_lossy()
        )));
    }
    let dir = path
        .parent()
        .ok_or_else(|| KhError::InvalidInput("密钥路径异常".into()))?;
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| KhError::InvalidInput("密钥文件名异常".into()))?;
    let keygen = env::find_ssh_keygen();
    Ok(build_meta(dir, &filename, path, keygen.as_deref()))
}

/// 读取公钥全文（可公开分发）
pub fn read_public_key(filename: &str) -> KhResult<String> {
    let dir = paths::ssh_dir()?;
    let priv_path = resolve_in_ssh(filename)?;
    let pub_path = dir.join(format!("{filename}.pub"));

    if pub_path.is_file() {
        return Ok(std::fs::read_to_string(&pub_path)?.trim().to_string());
    }

    // 没有 .pub 时用 ssh-keygen -y 导出
    let keygen = env::find_ssh_keygen().ok_or_else(|| {
        KhError::SshMissing("未找到 ssh-keygen，无法导出公钥".into())
    })?;
    let out = perms::hidden_command(&keygen.to_string_lossy())
        .args(["-y", "-f", &priv_path.to_string_lossy()])
        .output()
        .map_err(|e| KhError::CommandFailed(format!("无法执行 ssh-keygen：{e}")))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(KhError::CommandFailed(format!(
            "导出公钥失败（若私钥设有口令，请先在 ssh-agent 中加载）：{err}"
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 读取私钥明文（高敏感，调用方须已完成二次确认）
pub fn read_private_key(filename: &str) -> KhResult<String> {
    let path = resolve_in_ssh(filename)?;
    if !path.is_file() {
        return Err(KhError::NotFound(format!("{filename} 不存在")));
    }
    if !looks_like_private_key(&path) {
        return Err(KhError::InvalidInput(format!(
            "{filename} 不是 OpenSSH 私钥文件"
        )));
    }
    Ok(std::fs::read_to_string(&path)?)
}

/* ═══════════════════════════════════════════════════════════════
 * 删除
 * ═══════════════════════════════════════════════════════════════ */

/// 删除密钥（含 .pub）。返回被删除的文件列表。
pub fn delete_key(filename: &str) -> KhResult<Vec<String>> {
    let priv_path = resolve_in_ssh(filename)?;
    let dir = paths::ssh_dir()?;
    let pub_path = dir.join(format!("{filename}.pub"));
    let mut removed = Vec::new();

    if priv_path.is_file() {
        std::fs::remove_file(&priv_path)?;
        removed.push(filename.to_string());
    }
    if pub_path.is_file() {
        std::fs::remove_file(&pub_path)?;
        removed.push(format!("{filename}.pub"));
    }

    if removed.is_empty() {
        return Err(KhError::NotFound(format!("{filename} 不存在")));
    }
    Ok(removed)
}

/* ═══════════════════════════════════════════════════════════════
 * 口令设置（PRD 3.2.2）
 * ═══════════════════════════════════════════════════════════════ */

/// 为私钥设置 / 修改口令。等效 `ssh-keygen -p`，写入前自动备份 .bak。
pub fn set_passphrase(filename: &str, old_pass: &str, new_pass: &str) -> KhResult<()> {
    let path = resolve_in_ssh(filename)?;
    if !path.is_file() {
        return Err(KhError::NotFound(format!("{filename} 不存在")));
    }
    if new_pass.len() < 8 {
        return Err(KhError::InvalidInput("新口令至少需要 8 位".into()));
    }

    let keygen = env::find_ssh_keygen().ok_or_else(|| {
        KhError::SshMissing("未找到 ssh-keygen，无法设置口令".into())
    })?;

    // 1) 备份原文件
    let bak = path.with_extension("bak");
    std::fs::copy(&path, &bak)?;
    let _ = perms::fix(&bak, false);

    // 2) 执行 ssh-keygen -p -P <old> -N <new> -f <file>
    let out = perms::hidden_command(&keygen.to_string_lossy())
        .args([
            "-q",
            "-p",
            "-P",
            old_pass,
            "-N",
            new_pass,
            "-f",
            &path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| KhError::CommandFailed(format!("无法执行 ssh-keygen：{e}")))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // 失败则回滚备份
        let _ = std::fs::copy(&bak, &path);
        return Err(KhError::CommandFailed(if err.is_empty() {
            "设置口令失败，原口令可能不正确".into()
        } else {
            format!("设置口令失败：{err}")
        }));
    }

    // 3) 收紧权限
    let _ = perms::fix(&path, false);
    Ok(())
}

/// 撤销口令（等效 ssh-keygen -p -N ""），同样先备份
pub fn remove_passphrase(filename: &str, old_pass: &str) -> KhResult<()> {
    let path = resolve_in_ssh(filename)?;
    if !path.is_file() {
        return Err(KhError::NotFound(format!("{filename} 不存在")));
    }
    let keygen = env::find_ssh_keygen().ok_or_else(|| {
        KhError::SshMissing("未找到 ssh-keygen".into())
    })?;

    let bak = path.with_extension("bak");
    std::fs::copy(&path, &bak)?;

    let out = perms::hidden_command(&keygen.to_string_lossy())
        .args(["-q", "-p", "-P", old_pass, "-N", "", "-f", &path.to_string_lossy()])
        .output()
        .map_err(|e| KhError::CommandFailed(format!("无法执行 ssh-keygen：{e}")))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let _ = std::fs::copy(&bak, &path);
        return Err(KhError::CommandFailed(format!(
            "撤销口令失败：{err}"
        )));
    }

    let _ = perms::fix(&path, false);
    Ok(())
}
