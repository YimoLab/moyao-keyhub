//! 路径解析：`~/.ssh` 与程序自身的应用数据目录。
//!
//! 红线（PRD 5.3）：密钥与 config 一律存于用户原生 `~/.ssh`；
//! 程序自身配置（偏好、备份、审计日志）存于系统应用数据目录，
//! 程序绝不另存私钥副本。

use std::path::{Path, PathBuf};

use crate::error::{KhError, KhResult};

/// 应用数据目录名（跨平台统一为 KeyHub）
const APP_DIR: &str = "KeyHub";

/// 用户主目录。Windows 优先 USERPROFILE，Unix 用 HOME。
pub fn home_dir() -> KhResult<PathBuf> {
    if let Some(h) = dirs::home_dir() {
        return Ok(h);
    }
    // 兜底：直接读环境变量，避免极端环境下拿不到 home
    for key in ["USERPROFILE", "HOME"] {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Ok(PathBuf::from(v));
            }
        }
    }
    Err(KhError::NotFound(
        "无法定位用户主目录，请检查 USERPROFILE / HOME 环境变量".into(),
    ))
}

/// `~/.ssh` 目录
pub fn ssh_dir() -> KhResult<PathBuf> {
    Ok(home_dir()?.join(".ssh"))
}

/// `~/.ssh/config` 文件
pub fn ssh_config_path() -> KhResult<PathBuf> {
    Ok(ssh_dir()?.join("config"))
}

/// 程序应用数据目录（不存在则创建）
pub fn app_data_dir() -> KhResult<PathBuf> {
    let base = dirs::data_dir()
        .or_else(|| dirs::config_dir())
        .ok_or_else(|| KhError::NotFound("无法定位系统应用数据目录".into()))?;
    let dir = base.join(APP_DIR);
    ensure_dir(&dir)?;
    Ok(dir)
}

/// 程序偏好文件
pub fn prefs_path() -> KhResult<PathBuf> {
    Ok(app_data_dir()?.join("prefs.json"))
}

/// 审计日志文件
pub fn audit_path() -> KhResult<PathBuf> {
    Ok(app_data_dir()?.join("audit.log"))
}

/// 全量备份输出目录
pub fn backup_dir() -> KhResult<PathBuf> {
    let dir = app_data_dir()?.join("backups");
    ensure_dir(&dir)?;
    Ok(dir)
}

/// config 自动备份目录（保存/回滚前生成）
pub fn config_backup_dir() -> KhResult<PathBuf> {
    let dir = app_data_dir()?.join("config-backups");
    ensure_dir(&dir)?;
    Ok(dir)
}

/// 确保目录存在（递归创建）
pub fn ensure_dir(p: &Path) -> KhResult<()> {
    if p.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(p)?;
    Ok(())
}

/// 把路径美化用于展示：把 home 前缀替换为 `~`
pub fn pretty(p: &Path) -> String {
    if let Ok(home) = home_dir() {
        if let Ok(rel) = p.strip_prefix(&home) {
            let s = rel.to_string_lossy().replace('\\', "/");
            return format!("~/{}", s);
        }
    }
    p.to_string_lossy().to_string()
}

/// 展开以 `~/` 开头的路径为绝对路径
pub fn expand(path: &str) -> KhResult<PathBuf> {
    let t = path.trim();
    if t == "~" {
        return home_dir();
    }
    if let Some(rest) = t.strip_prefix("~/").or_else(|| t.strip_prefix("~\\")) {
        return Ok(home_dir()?.join(rest));
    }
    Ok(PathBuf::from(t))
}

/// 校验文件名字段：只允许字母、数字、下划线、连字符与点。
/// 直接沿用设计稿的校验规则，同时防御路径穿越（拒绝 `/`、`\`、`..`）。
pub fn validate_filename(name: &str) -> KhResult<()> {
    let n = name.trim();
    if n.is_empty() {
        return Err(KhError::InvalidInput("文件名不能为空".into()));
    }
    if n.len() > 128 {
        return Err(KhError::InvalidInput("文件名过长（最多 128 字符）".into()));
    }
    if n.starts_with('.') {
        return Err(KhError::InvalidInput(
            "文件名不能以点开头（避免生成隐藏文件）".into(),
        ));
    }
    let ok = n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !ok {
        return Err(KhError::InvalidInput(
            "文件名仅允许字母、数字、下划线、连字符与点".into(),
        ));
    }
    // 防御路径穿越
    if n.contains("..") {
        return Err(KhError::InvalidInput("文件名不能包含连续的 .".into()));
    }
    Ok(())
}
