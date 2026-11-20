//! 时间工具：统一 RFC3339 与文件名时间戳。
//!
//! 单独成模块是为了让所有落盘时间格式一致，
//! 且避免各处重复依赖 chrono 的时间格式化调用。
//!
//! 本模块是一组**完备的小工具集**：即使当前只有部分函数被调用，
//! 也保留完整实现（例如 `days_since` / `file_created_rfc3339` 目前
//! 由前端直接基于 `createdAt` 计算年龄，暂未在 Rust 侧调用）。
//! 这样时间语义只有一份定义，后续需要时可直接复用而不必重新推导格式。
#![allow(dead_code)]

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 当前时间 RFC3339（本地时区）
pub fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// 文件名安全的时间戳：`20260911-124512`
pub fn file_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// 文件名安全的时间戳（带毫秒）：`20260911-124512-837`
pub fn file_stamp_ms() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string()
}

/// 展示用的时间：`2026-09-11 12:45`
pub fn display_now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// 展示用的日期：`2026-09-11`
pub fn display_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 把 SystemTime 转为 RFC3339
pub fn system_time_rfc3339(t: SystemTime) -> Option<String> {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
}

/// 读取文件修改时间并转 RFC3339
pub fn file_time_rfc3339(p: &Path) -> Option<String> {
    let meta = std::fs::metadata(p).ok()?;
    let mtime = meta.modified().ok()?;
    system_time_rfc3339(mtime)
}

/// 读取文件创建时间（系统不支持创建时间时回退到修改时间）
pub fn file_created_rfc3339(p: &Path) -> Option<String> {
    let meta = std::fs::metadata(p).ok()?;
    if let Ok(t) = meta.created() {
        return system_time_rfc3339(t);
    }
    let mtime = meta.modified().ok()?;
    system_time_rfc3339(mtime)
}

/// 距今天数（供「密钥年龄」提示）
pub fn days_since(p: &Path) -> Option<i64> {
    let meta = std::fs::metadata(p).ok()?;
    let mtime = meta.modified().ok()?;
    let secs = mtime.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((now - secs).max(0) / 86400)
}

/// 解析 RFC3339 为 Unix 秒（审计日志排序用）
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp())
}

/// 从文件名的 `config.<stamp>.<reason>.bak` 中解析时间戳为展示时间。
/// 解析失败返回空串，前端会回退到文件 mtime。
pub fn stamp_from_backup_name(name: &str) -> Option<String> {
    // config.20260911-124512.save.bak
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let stamp = parts[1];
    let dt = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    Some(dt.format("%Y-%m-%d %H:%M:%S").to_string())
}
