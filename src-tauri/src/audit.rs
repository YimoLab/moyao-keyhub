//! 审计日志（PRD 3.1.5 安全中心 / 7.2 可追溯）
//!
//! 记录每一次敏感操作，本地追加写入，永不外发。
//! 存储格式为 JSONL（每行一个 JSON 对象），便于追加与流式读取。

use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::error::{KhError, KhResult};
use crate::paths;
use crate::time_util;

/// 审计事件类别
pub const CAT_KEY: &str = "key";
pub const CAT_CONFIG: &str = "config";
pub const CAT_PERM: &str = "perm";
pub const CAT_CONNECT: &str = "connect";
pub const CAT_AGENT: &str = "agent";
pub const CAT_BACKUP: &str = "backup";
pub const CAT_SECURITY: &str = "security";

/// 一条审计记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// 自增序号（读取时按行号填充）
    #[serde(default)]
    pub seq: u64,
    /// RFC3339 时间
    pub at: String,
    /// 类别
    pub category: String,
    /// 动作标识，如 `key.generate`
    pub action: String,
    /// 人类可读描述（中文）
    pub message: String,
    /// 结果：`ok` | `fail` | `warn`
    pub result: String,
    /// 作用对象（文件名 / Host 别名 / 端口等）
    #[serde(default)]
    pub target: String,
    /// 补充细节
    #[serde(default)]
    pub detail: String,
}

/// 审计日志查询条件
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    /// 类别过滤，空表示全部
    #[serde(default)]
    pub category: String,
    /// 结果过滤
    #[serde(default)]
    pub result: String,
    /// 关键字（匹配 message / target / action）
    #[serde(default)]
    pub keyword: String,
    /// 最多返回条数（0 表示不限，默认 500）
    #[serde(default)]
    pub limit: u32,
    /// 跳过前 N 条（分页偏移，仅 read_audit_page 使用）
    #[serde(default)]
    pub offset: u32,
}

fn ensure_file() -> KhResult<std::path::PathBuf> {
    let p = paths::audit_path()?;
    if !p.is_file() {
        std::fs::write(&p, b"")?;
    }
    Ok(p)
}

/// 追加一条审计记录。审计失败不应阻断主流程，因此调用方通常忽略错误。
pub fn append(
    category: &str,
    action: &str,
    message: &str,
    result: &str,
    target: &str,
    detail: &str,
) -> KhResult<()> {
    let entry = AuditEntry {
        seq: 0,
        at: time_util::now_rfc3339(),
        category: category.into(),
        action: action.into(),
        message: message.into(),
        result: result.into(),
        target: target.into(),
        detail: detail.into(),
    };
    let line = serde_json::to_string(&entry)
        .map_err(|e| KhError::InvalidInput(format!("审计序列化失败：{}", e)))?;
    let p = ensure_file()?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// 静默追加：绝不让审计错误冒泡影响主功能
pub fn log(
    category: &str,
    action: &str,
    message: &str,
    result: &str,
    target: &str,
    detail: &str,
) {
    let _ = append(category, action, message, result, target, detail);
}

/// 审计日志分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditPage {
    /// 当前页记录（最新在前）
    pub entries: Vec<AuditEntry>,
    /// 过滤后的总条数（未分页）
    pub total: usize,
}

/// 读取审计日志（倒序，最新在前）
pub fn read_audit(q: AuditQuery) -> KhResult<Vec<AuditEntry>> {
    let mut out = collect_filtered(&q)?;
    let limit = if q.limit == 0 { 500 } else { q.limit as usize };
    out.truncate(limit);
    Ok(out)
}

/// 分页读取审计日志：先过滤出全部匹配项，再按 offset/limit 切片。
pub fn read_audit_page(q: AuditQuery) -> KhResult<AuditPage> {
    let all = collect_filtered(&q)?;
    let total = all.len();
    let offset = q.offset as usize;
    let limit = if q.limit == 0 { 200 } else { q.limit as usize };
    let entries = all.into_iter().skip(offset).take(limit).collect();
    Ok(AuditPage { entries, total })
}

/// 按条件过滤并按时间倒序返回全部记录
fn collect_filtered(q: &AuditQuery) -> KhResult<Vec<AuditEntry>> {
    let p = paths::audit_path()?;
    if !p.is_file() {
        return Ok(Vec::new());
    }
    let txt = std::fs::read_to_string(&p)?;
    let kw = q.keyword.trim().to_lowercase();
    let mut out: Vec<AuditEntry> = Vec::new();

    for (i, line) in txt.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(mut e) = serde_json::from_str::<AuditEntry>(line) else {
            continue;
        };
        e.seq = (i + 1) as u64;

        if !q.category.is_empty() && e.category != q.category {
            continue;
        }
        if !q.result.is_empty() && e.result != q.result {
            continue;
        }
        if !kw.is_empty() {
            let hay = format!("{} {} {}", e.message, e.target, e.action).to_lowercase();
            if !hay.contains(&kw) {
                continue;
            }
        }
        out.push(e);
    }

    out.reverse();
    Ok(out)
}

/// 统计信息（安全中心卡片用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditStats {
    /// 总条数
    pub total: usize,
    /// 今日条数
    pub today: usize,
    /// 失败条数
    pub failed: usize,
    /// 日志文件字节数
    pub bytes: u64,
    /// 最近一条时间
    pub last_at: String,
}

/// 统计审计日志
pub fn audit_stats() -> KhResult<AuditStats> {
    let p = paths::audit_path()?;
    if !p.is_file() {
        return Ok(AuditStats {
            total: 0,
            today: 0,
            failed: 0,
            bytes: 0,
            last_at: String::new(),
        });
    }
    let txt = std::fs::read_to_string(&p)?;
    let today = time_util::display_date();
    let mut total = 0usize;
    let mut td = 0usize;
    let mut failed = 0usize;
    let mut last = String::new();
    for line in txt.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(e) = serde_json::from_str::<AuditEntry>(line) else {
            continue;
        };
        total += 1;
        if e.at.starts_with(&today) {
            td += 1;
        }
        if e.result == "fail" {
            failed += 1;
        }
        last = e.at;
    }
    Ok(AuditStats {
        total,
        today: td,
        failed,
        bytes: std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
        last_at: last,
    })
}

/// 导出为 CSV，返回写入的绝对路径。
/// out_dir 为空时导到系统下载目录（找不到则用应用数据目录）。
pub fn export_csv(out_dir: Option<String>) -> KhResult<String> {
    let entries = read_audit(AuditQuery {
        limit: 0,
        ..Default::default()
    })?;

    let dir: std::path::PathBuf = match out_dir {
        Some(d) if !d.trim().is_empty() => {
            let p = std::path::PathBuf::from(d.trim());
            paths::ensure_dir(&p)?;
            p
        }
        _ => match dirs::download_dir() {
            Some(d) => d,
            None => paths::app_data_dir()?,
        },
    };
    let name = format!("keyhub-audit-{}.csv", time_util::file_stamp());
    let path = dir.join(&name);

    let mut out = String::new();
    // 带 BOM，保证 Excel 打开中文不乱码
    out.push('\u{feff}');
    out.push_str("序号,时间,类别,动作,结果,对象,描述,详情\n");
    // CSV 按时间正序更符合阅读习惯
    for e in entries.iter().rev() {
        out.push_str(&csv_row(&[
            &e.seq.to_string(),
            &e.at,
            &cat_label(&e.category),
            &e.action,
            &result_label(&e.result),
            &e.target,
            &e.message,
            &e.detail,
        ]));
    }
    std::fs::write(&path, out.as_bytes())?;
    log(
        CAT_SECURITY,
        "audit.export",
        &format!("导出审计日志 {} 条", entries.len()),
        "ok",
        &name,
        &paths::pretty(&path),
    );
    Ok(path.to_string_lossy().to_string())
}

/// 清理超过保留期的审计记录，返回删除条数
pub fn prune(days: u32) -> KhResult<usize> {
    if days == 0 {
        return Ok(0);
    }
    let p = paths::audit_path()?;
    if !p.is_file() {
        return Ok(0);
    }
    let txt = std::fs::read_to_string(&p)?;
    let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
    let mut kept: Vec<&str> = Vec::new();
    let mut removed = 0usize;
    for line in txt.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<AuditEntry>(line) {
            Ok(e) => {
                let ok = time_util::parse_rfc3339(&e.at)
                    .map(|ts| ts >= cutoff.timestamp())
                    .unwrap_or(true);
                if ok {
                    kept.push(line);
                } else {
                    removed += 1;
                }
            }
            Err(_) => kept.push(line),
        }
    }
    if removed > 0 {
        let mut s = kept.join("\n");
        if !s.is_empty() {
            s.push('\n');
        }
        std::fs::write(&p, s.as_bytes())?;
    }
    Ok(removed)
}

/// 清空审计日志
pub fn clear_audit() -> KhResult<()> {
    let p = paths::audit_path()?;
    if p.is_file() {
        std::fs::write(&p, b"")?;
    }
    log(
        CAT_SECURITY,
        "audit.clear",
        "清空审计日志",
        "warn",
        "",
        "",
    );
    Ok(())
}

fn csv_row(cells: &[&str]) -> String {
    let escaped: Vec<String> = cells
        .iter()
        .map(|c| {
            let needs = c.contains(',') || c.contains('"') || c.contains('\n');
            if needs {
                format!("\"{}\"", c.replace('"', "\"\""))
            } else {
                (*c).to_string()
            }
        })
        .collect();
    format!("{}\n", escaped.join(","))
}

/// 类别中文标签
pub fn cat_label(c: &str) -> String {
    match c {
        CAT_KEY => "密钥",
        CAT_CONFIG => "配置",
        CAT_PERM => "权限",
        CAT_CONNECT => "连通",
        CAT_AGENT => "Agent",
        CAT_BACKUP => "备份",
        CAT_SECURITY => "安全",
        other => other,
    }
    .into()
}

/// 结果中文标签
pub fn result_label(r: &str) -> String {
    match r {
        "ok" => "成功",
        "fail" => "失败",
        "warn" => "警告",
        other => other,
    }
    .into()
}
