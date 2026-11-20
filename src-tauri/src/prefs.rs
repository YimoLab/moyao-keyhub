//! 用户偏好（PRD 3.1.5 设置页）
//!
//! 严格落在应用数据目录，绝不写入 `~/.ssh`，
//! 也绝不上传。字段与前端设置页逐项对应。

use serde::{Deserialize, Serialize};

use crate::error::{KhError, KhResult};
use crate::paths;

/// 用户偏好
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    /// 主题：`light` | `dark` | `system`
    pub theme: String,
    /// 标题栏风格：`auto`（Windows/Linux 右上按钮、macOS 左上交通灯）| `windows` | `mac`
    pub titlebar_style: String,
    /// 启动时自动检查更新（默认关闭，仅在用户开启后访问 GitHub 版本文件）
    pub auto_check_updates: bool,
    /// 语言：`zh-CN` | `en-US`
    pub locale: String,
    /// 是否自动为新密钥设置 600 权限
    pub auto_fix_perms: bool,
    /// 保存 config 前是否自动备份
    pub auto_backup_config: bool,
    /// 是否记录审计日志
    pub audit_enabled: bool,
    /// 审计日志保留天数（0 表示不清理）
    pub audit_retention_days: u32,
    /// 是否在启动时执行环境自检
    pub autocheck_on_start: bool,
    /// 默认密钥算法：`ed25519` | `rsa`
    pub default_algorithm: String,
    /// RSA 默认位数
    pub default_rsa_bits: u32,
    /// 默认注释（通常是邮箱）
    pub default_comment: String,
    /// 生成密钥后是否写入 ssh-agent
    pub auto_add_to_agent: bool,
    /// 是否启用 ssh-agent 开机自启
    pub agent_autostart: bool,
    /// 连通测试超时（秒）
    pub connect_timeout_secs: u32,
    /// 是否在连通测试前确认（PRD：发起网络请求需二次确认）
    pub confirm_before_connect: bool,
    /// 是否把密码学随机源固定为系统 CSPRNG 严格模式（始终 true，仅作展示位）
    pub strict_csprng: bool,
    /// 删除密钥时是否要求输入文件名确认
    pub confirm_delete_typed: bool,
    /// 最近一次引导完成时间
    pub onboarded_at: Option<String>,
    /// 界面密度：`comfortable` | `compact`
    pub density: String,
    /// 侧栏是否折叠
    pub sidebar_collapsed: bool,
    /// 确认过的危险操作级别（用于「不再提示」）
    pub ack_danger: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: "system".into(),
            titlebar_style: "auto".into(),
            auto_check_updates: false,
            locale: "zh-CN".into(),
            auto_fix_perms: true,
            auto_backup_config: true,
            audit_enabled: true,
            audit_retention_days: 90,
            autocheck_on_start: true,
            default_algorithm: "ed25519".into(),
            default_rsa_bits: 4096,
            default_comment: String::new(),
            auto_add_to_agent: false,
            agent_autostart: false,
            connect_timeout_secs: 10,
            confirm_before_connect: true,
            strict_csprng: true,
            confirm_delete_typed: true,
            onboarded_at: None,
            density: "comfortable".into(),
            sidebar_collapsed: false,
            ack_danger: false,
        }
    }
}

/// 读取偏好。文件不存在或损坏时返回默认值。
pub fn read_prefs() -> KhResult<Prefs> {
    let path = paths::prefs_path()?;
    if !path.is_file() {
        return Ok(Prefs::default());
    }
    let txt = std::fs::read_to_string(&path)?;
    match serde_json::from_str::<Prefs>(&txt) {
        Ok(p) => Ok(sanitize(p)),
        // 损坏时不阻塞用户，退回默认值并在下次写入时覆盖
        Err(_) => Ok(Prefs::default()),
    }
}

/// 写入偏好（原子写）
pub fn write_prefs(p: &Prefs) -> KhResult<Prefs> {
    let p = sanitize(p.clone());
    let path = paths::prefs_path()?;
    let dir = path
        .parent()
        .ok_or_else(|| KhError::InvalidInput("偏好路径异常".into()))?;
    paths::ensure_dir(dir)?;
    let txt = serde_json::to_string_pretty(&p)
        .map_err(|e| KhError::InvalidInput(format!("偏好序列化失败：{}", e)))?;
    let tmp = path.with_file_name("prefs.json.tmp");
    std::fs::write(&tmp, txt.as_bytes())?;
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    std::fs::rename(&tmp, &path).map_err(|e| KhError::Io(e.to_string()))?;
    Ok(p)
}

/// 局部更新偏好（只覆盖传入的非 None 字段）
pub fn patch_prefs(patch: Prefs) -> KhResult<Prefs> {
    let mut cur = read_prefs()?;
    let json_cur = serde_json::to_value(&cur).unwrap_or_default();
    let json_new = serde_json::to_value(&patch).unwrap_or_default();
    if let (serde_json::Value::Object(mut a), serde_json::Value::Object(b)) = (json_cur, json_new) {
        // 前端只传改动字段，未传字段保持原值。
        // 由于 Prefs 有 default，无法区分「未传」与「显式传默认值」，
        // 这里采用整体覆盖策略：调用方传完整对象即可，语义更可预测。
        for (k, v) in b {
            a.insert(k, v);
        }
        if let Ok(merged) = serde_json::from_value::<Prefs>(serde_json::Value::Object(a)) {
            cur = merged;
        }
    }
    write_prefs(&cur)
}

/// 重置为默认偏好
pub fn reset_prefs() -> KhResult<Prefs> {
    write_prefs(&Prefs::default())
}

/// 规整非法取值，避免前端笔误导致行为异常
fn sanitize(mut p: Prefs) -> Prefs {
    if !matches!(p.theme.as_str(), "light" | "dark" | "system") {
        p.theme = "system".into();
    }
    if !matches!(p.locale.as_str(), "zh-CN" | "en-US") {
        p.locale = "zh-CN".into();
    }
    if !matches!(p.titlebar_style.as_str(), "auto" | "windows" | "mac") {
        p.titlebar_style = "auto".into();
    }
    if !matches!(p.default_algorithm.as_str(), "ed25519" | "rsa") {
        p.default_algorithm = "ed25519".into();
    }
    if !matches!(p.default_rsa_bits, 2048 | 3072 | 4096) {
        p.default_rsa_bits = 4096;
    }
    if !matches!(p.density.as_str(), "comfortable" | "compact") {
        p.density = "comfortable".into();
    }
    // 连通超时限制在 3-120 秒，防止用户填 0 导致立即失败
    if p.connect_timeout_secs < 3 {
        p.connect_timeout_secs = 3;
    }
    if p.connect_timeout_secs > 120 {
        p.connect_timeout_secs = 120;
    }
    // 审计保留期上限 10 年
    if p.audit_retention_days > 3650 {
        p.audit_retention_days = 3650;
    }
    // 红线：始终严格随机源，不允许关闭
    p.strict_csprng = true;
    p
}
