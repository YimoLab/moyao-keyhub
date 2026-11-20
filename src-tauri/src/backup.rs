//! 加密备份与恢复（PRD 3.1.4 备份 / 安全中心）
//!
//! 设计（PRD 7.2）：
//! - 备份内容为 `~/.ssh` 下的密钥与 config，打包为 zip
//! - 用口令派生密钥（Argon2id）后以 AES-256-GCM 加密整个 zip
//! - 口令不落盘、不进日志，仅在内存中参与派生
//! - 恢复前会先把当前 `~/.ssh` 打包另存，保证误操作可回溯
//!
//! 文件格式 `.khbak`：
//! ```text
//! magic  "KHBAK1\0"            8 bytes
//! salt   Argon2 salt           16 bytes
//! nonce  AES-GCM nonce         12 bytes
//! body   AES-256-GCM(zip)      剩余全部
//! ```

use std::io::{Read, Write};
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use argon2::Argon2;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::{KhError, KhResult};
use crate::paths;
use crate::perms;
use crate::time_util;

/// 文件魔数（8 字节：`KHBAK1` 后补两个 0，便于用定长头做快速格式校验）
const MAGIC: &[u8; 8] = b"KHBAK1\0\0";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
/// 备份扩展名
const EXT: &str = "khbak";

/// 备份条目描述
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupItem {
    /// 文件名
    pub name: String,
    /// 绝对路径
    pub path: String,
    /// 美化路径
    pub pretty_path: String,
    /// 字节数
    pub size: u64,
    /// 创建时间 RFC3339
    pub created_at: String,
    /// 是否加密
    pub encrypted: bool,
    /// 归档内包含的文件数（读取时统计，可能为 0 表示未知）
    pub entries: usize,
    /// 人类可读大小
    pub size_label: String,
}

/// 创建备份的选项
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOpts {
    /// 口令；为空则创建明文 zip（不推荐，UI 会二次确认）
    #[serde(default)]
    pub passphrase: String,
    /// 备注，写进文件名
    #[serde(default)]
    pub label: String,
    /// 是否包含私钥（关闭时只备份 config 与公钥）
    #[serde(default = "default_true")]
    pub include_private: bool,
    /// 输出目录；为空则用应用数据目录
    #[serde(default)]
    pub out_dir: String,
}

fn default_true() -> bool {
    true
}

/// 备份结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub name: String,
    pub path: String,
    pub pretty_path: String,
    pub size: u64,
    pub size_label: String,
    pub encrypted: bool,
    pub entries: usize,
    pub file_list: Vec<String>,
}

/* ═══════════════ 加密原语 ═══════════════ */

/// 由口令派生 32 字节密钥（Argon2id）
fn derive_key(pass: &str, salt: &[u8]) -> KhResult<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(pass.as_bytes(), salt, &mut key)
        .map_err(|e| KhError::Crypto(format!("密钥派生失败：{}", e)))?;
    Ok(key)
}

/// 加密明文
fn encrypt(plain: &[u8], pass: &str) -> KhResult<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(pass, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| KhError::Crypto(format!("初始化加密器失败：{}", e)))?;
    let nonce_arr = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce_arr, plain)
        .map_err(|e| KhError::Crypto(format!("加密失败：{}", e)))?;

    let mut out = Vec::with_capacity(8 + SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_arr);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// 解密
fn decrypt(blob: &[u8], pass: &str) -> KhResult<Vec<u8>> {
    if blob.len() < 8 + SALT_LEN + NONCE_LEN {
        return Err(KhError::Crypto("备份文件不完整或已损坏".into()));
    }
    if &blob[..8] != MAGIC {
        return Err(KhError::Crypto(
            "文件格式无法识别，可能不是墨钥生成的备份".into(),
        ));
    }
    let salt = &blob[8..8 + SALT_LEN];
    let nonce = &blob[8 + SALT_LEN..8 + SALT_LEN + NONCE_LEN];
    let body = &blob[8 + SALT_LEN + NONCE_LEN..];

    let key = derive_key(pass, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| KhError::Crypto(format!("初始化解密器失败：{}", e)))?;
    let n = Nonce::from_slice(nonce);
    cipher.decrypt(n, body).map_err(|_| {
        KhError::Crypto("解密失败：口令不正确，或备份文件已被篡改".into())
    })
}

/* ═══════════════ 打包 ═══════════════ */

/// 收集需要备份的文件
fn collect_targets(include_private: bool) -> KhResult<Vec<(std::path::PathBuf, String)>> {
    let dir = paths::ssh_dir()?;
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Err(KhError::NotFound(
            "~/.ssh 目录不存在，没有可备份的内容".into(),
        ));
    }

    let rd = std::fs::read_dir(&dir)?;
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();

        // config 与公钥始终备份
        let is_config = name == "config";
        let is_pub = name.ends_with(".pub");
        let is_known = name == "known_hosts" || name.starts_with("known_hosts");
        let is_authorized = name.starts_with("authorized_keys");

        if is_config || is_pub {
            out.push((p, name));
            continue;
        }
        if is_known || is_authorized {
            // 不备份 known_hosts / authorized_keys：
            // 前者是环境相关状态，后者是服务端授权列表，恢复它们反而会引入风险
            continue;
        }
        if name.starts_with('.') {
            continue;
        }

        if include_private {
            if crate::keys::looks_like_private_key(&p) {
                out.push((p, name));
            }
        }
    }

    // 手动备份审计日志，便于迁移
    if let Ok(ap) = paths::audit_path() {
        if ap.is_file() {
            out.push((ap, "keyhub-audit.log".into()));
        }
    }
    // 偏好
    if let Ok(pp) = paths::prefs_path() {
        if pp.is_file() {
            out.push((pp, "keyhub-prefs.json".into()));
        }
    }

    Ok(out)
}

/// 创建 zip 归档到内存
fn build_zip(files: &[(std::path::PathBuf, String)]) -> KhResult<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600);

        for (src, name) in files {
            let mut f = std::fs::File::open(src).map_err(|e| {
                KhError::Io(format!("无法读取 {}：{}", src.display(), e))
            })?;
            let mut data = Vec::new();
            f.read_to_end(&mut data)?;
            zw.start_file(name.clone(), opts)
                .map_err(|e| KhError::Io(format!("写入压缩条目失败：{e}")))?;
            zw.write_all(&data)?;
        }
        zw.finish()
            .map_err(|e| KhError::Io(format!("压缩收尾失败：{e}")))?;
    }
    Ok(buf.into_inner())
}

/* ═══════════════ 对外接口 ═══════════════ */

/// 创建备份
pub fn create_backup(opts: BackupOpts) -> KhResult<BackupResult> {
    let files = collect_targets(opts.include_private)?;
    if files.is_empty() {
        return Err(KhError::NotFound("没有找到可备份的文件".into()));
    }

    let zip_bytes = build_zip(&files)?;

    let encrypted = !opts.passphrase.trim().is_empty();
    let payload = if encrypted {
        encrypt(&zip_bytes, opts.passphrase.trim())?
    } else {
        zip_bytes
    };

    // 输出路径
    let dir: std::path::PathBuf = if opts.out_dir.trim().is_empty() {
        paths::backup_dir()?
    } else {
        let p = std::path::PathBuf::from(opts.out_dir.trim());
        paths::ensure_dir(&p)?;
        p
    };

    let label = sanitize_label(&opts.label);
    let name = if label.is_empty() {
        format!("keyhub-backup-{}.{}", time_util::file_stamp(), EXT)
    } else {
        format!("keyhub-backup-{}-{}.{}", time_util::file_stamp(), label, EXT)
    };
    let path = dir.join(&name);
    std::fs::write(&path, &payload)?;
    // 备份文件本身也是敏感物，收紧权限（文件 → 600）
    let _ = perms::fix(&path, false);

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let file_list: Vec<String> = files.iter().map(|(_, n)| n.clone()).collect();

    crate::audit::log(
        crate::audit::CAT_BACKUP,
        "backup.create",
        &format!(
            "创建{}备份，包含 {} 个文件",
            if encrypted { "加密" } else { "明文" },
            files.len()
        ),
        "ok",
        &name,
        &paths::pretty(&path),
    );

    Ok(BackupResult {
        name,
        path: path.to_string_lossy().to_string(),
        pretty_path: paths::pretty(&path),
        size,
        size_label: human_size(size),
        encrypted,
        entries: files.len(),
        file_list,
    })
}

/// 列出全部备份
pub fn list_backups() -> KhResult<Vec<BackupItem>> {
    let dir = paths::backup_dir()?;
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return Ok(out),
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if !name.ends_with(EXT) {
            continue;
        }
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        // 读前 8 字节判断是否加密
        let encrypted = {
            let mut head = [0u8; 8];
            std::fs::File::open(&p)
                .and_then(|mut f| f.read_exact(&mut head))
                .map(|_| &head == MAGIC)
                .unwrap_or(false)
        };
        out.push(BackupItem {
            name,
            path: p.to_string_lossy().to_string(),
            pretty_path: paths::pretty(&p),
            size,
            size_label: human_size(size),
            created_at: time_util::file_time_rfc3339(&p).unwrap_or_default(),
            encrypted,
            entries: 0,
            // 已加密时无法在不提供口令的前提下统计条目
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// 校验备份：返回归档内的文件列表（需要口令时提供）
pub fn inspect_backup(name: &str, passphrase: &str) -> KhResult<Vec<String>> {
    let path = safe_backup_path(name)?;
    let blob = std::fs::read(&path)?;
    let zip_bytes = if blob.len() > 8 && &blob[..8] == MAGIC {
        if passphrase.trim().is_empty() {
            return Err(KhError::InvalidInput("该备份已加密，请输入口令".into()));
        }
        decrypt(&blob, passphrase.trim())?
    } else {
        blob
    };
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| KhError::InvalidInput(format!("归档无法解析：{}", e)))?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            names.push(f.name().to_string());
        }
    }
    Ok(names)
}

/// 恢复备份：把归档解回 `~/.ssh`
pub fn restore_backup(name: &str, passphrase: &str, overwrite: bool) -> KhResult<BackupResult> {
    let path = safe_backup_path(name)?;
    let blob = std::fs::read(&path)?;
    // 先判定加密状态——后面 `blob` 会被 move 进 `zip_bytes`
    let encrypted = blob.len() > 8 && &blob[..8] == MAGIC;
    let zip_bytes = if encrypted {
        if passphrase.trim().is_empty() {
            return Err(KhError::InvalidInput("该备份已加密，请输入口令".into()));
        }
        decrypt(&blob, passphrase.trim())?
    } else {
        blob
    };

    // 恢复前先做一次现场备份，保证可回溯
    let _ = create_backup(BackupOpts {
        passphrase: String::new(),
        label: "before-restore".into(),
        include_private: true,
        out_dir: String::new(),
    });

    let dir = paths::ssh_dir()?;
    paths::ensure_dir(&dir)?;

    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| KhError::InvalidInput(format!("归档无法解析：{}", e)))?;

    let mut restored = Vec::new();
    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| KhError::InvalidInput(format!("读取归档条目失败：{}", e)))?;
        let raw_name = f.name().to_string();

        // 归档条目名只允许纯文件名，防止 zip slip
        let fname = Path::new(&raw_name)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| KhError::InvalidInput(format!("归档条目名非法：{}", raw_name)))?;
        if fname.is_empty() || fname.contains("..") {
            continue;
        }

        // 审计日志与偏好回写到应用数据目录
        let target = if fname == "keyhub-audit.log" {
            paths::audit_path()?
        } else if fname == "keyhub-prefs.json" {
            paths::prefs_path()?
        } else {
            let p = dir.join(&fname);
            if p.exists() && !overwrite {
                continue;
            }
            p
        };

        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        std::fs::write(&target, &data)?;
        // 恢复的密钥必须重置权限（文件 → 600）
        if fname != "keyhub-audit.log" && fname != "keyhub-prefs.json" {
            let _ = perms::fix(&target, false);
        }
        restored.push(fname);
    }

    // 目录权限也收紧（目录 → 700）
    let _ = perms::fix(&dir, true);

    crate::audit::log(
        crate::audit::CAT_BACKUP,
        "backup.restore",
        &format!("从备份恢复 {} 个文件", restored.len()),
        "warn",
        name,
        &restored.join(", "),
    );

    let size = restored.len() as u64;
    Ok(BackupResult {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        pretty_path: paths::pretty(&path),
        size,
        size_label: human_size(size),
        encrypted,
        entries: restored.len(),
        file_list: restored,
    })
}

/// 删除备份
pub fn delete_backup(name: &str) -> KhResult<()> {
    let path = safe_backup_path(name)?;
    std::fs::remove_file(&path)?;
    crate::audit::log(
        crate::audit::CAT_BACKUP,
        "backup.delete",
        "删除备份",
        "warn",
        name,
        "",
    );
    Ok(())
}

/// 估算备份体积与内容（UI 在创建前展示「将备份 N 个文件、约 X」）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub count: usize,
    pub total_size: u64,
    pub size_label: String,
    pub files: Vec<String>,
    pub has_private: bool,
}

/// 预览将要备份的内容
pub fn preview_backup(include_private: bool) -> KhResult<BackupPreview> {
    let files = collect_targets(include_private)?;
    let mut total = 0u64;
    let mut has_private = false;
    let mut names = Vec::new();
    for (p, n) in &files {
        total += std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if crate::keys::looks_like_private_key(p) {
            has_private = true;
        }
        names.push(n.clone());
    }
    Ok(BackupPreview {
        count: files.len(),
        total_size: total,
        size_label: human_size(total),
        files: names,
        has_private,
    })
}

/* ═══════════════ 工具 ═══════════════ */

/// 校验备份名并解析出应用数据目录内的路径（防路径穿越）
fn safe_backup_path(name: &str) -> KhResult<std::path::PathBuf> {
    let safe = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| KhError::InvalidInput("备份名非法".into()))?;
    if safe != name || safe.is_empty() {
        return Err(KhError::InvalidInput("备份名不能包含路径分隔符".into()));
    }
    let p = paths::backup_dir()?.join(&safe);
    if !p.is_file() {
        return Err(KhError::NotFound(format!("备份不存在：{}", name)));
    }
    Ok(p)
}

/// 文件名备注清洗：只留字母数字与连字符
fn sanitize_label(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect()
}

/// 人类可读大小
pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// 备份目录路径（供设置页「打开备份目录」）
pub fn backup_dir_path() -> KhResult<String> {
    Ok(paths::backup_dir()?.to_string_lossy().to_string())
}
