//! 墨钥 KeyHub · Tauri 后端入口
//!
//! 架构分层（对应 PRD 附录交付物 1「分层架构图」）：
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │  表现层  src/views/*.vue + src/components/*.vue           │
//! │  Vue3 + Vite，纯本地渲染，不含业务规则                     │
//! ├──────────────────────────────────────────────────────────┤
//! │  桥接层  src/lib/api.js                                   │
//! │  invoke 封装；把 Rust 的 "CODE:msg" 拆成 e.code/e.message │
//! ├──────────────────────────────────────────────────────────┤
//! │  命令层  src-tauri/src/lib.rs  ← 本文件                    │
//! │  #[tauri::command] 薄适配：参数校验 + 调用领域模块          │
//! ├──────────────────────────────────────────────────────────┤
//! │  领域层  env / keys / config / test / agent / git /        │
//! │          backup / audit / prefs                           │
//! │  业务规则的唯一归属地                                      │
//! ├──────────────────────────────────────────────────────────┤
//! │  基础层  paths / perms / error / time_util                 │
//! │  路径解析、跨平台权限、统一错误模型、时间格式               │
//! ├──────────────────────────────────────────────────────────┤
//! │  系统层  OpenSSH (ssh/ssh-keygen/ssh-add) · icacls · chmod │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! 安全红线在本层的体现：
//! - 不注册任何 fs 通配命令，前端无法任意读写磁盘
//! - 所有写操作的目标路径都由 `paths` 模块在 Rust 侧计算，前端只传文件名
//! - 敏感操作（读私钥、删密钥、回滚、恢复备份、清空 agent）执行前写审计日志

mod agent;
mod audit;
mod backup;
mod config;
mod conn_test;
mod env;
mod error;
mod git;
mod keys;
mod paths;
mod perms;
mod prefs;
mod time_util;

use error::KhResult;

use agent::AgentStatus;
use audit::{AuditEntry, AuditQuery, AuditStats};
use backup::{BackupItem, BackupOpts, BackupPreview, BackupResult};
use config::{CheckIssue, ConfigBackup, ConfigDoc, HostEntry, SaveResult};
use conn_test::{TestHistoryItem, TestOpts, TestResult};
use env::{EnvReport, EnvSummary, InstallGuide};
use git::GitConfig;
use keys::{GenerateOpts, KeyMeta};
use perms::PermInfo;
use prefs::Prefs;

/* ═══════════════════════════════════════════════════════════════
 * 1. 环境自检（PRD 3.1.1）
 * ═══════════════════════════════════════════════════════════════ */

/// 全量环境自检。`createSshDir` 为 true 时会在缺失时创建 `~/.ssh`。
#[tauri::command]
fn detect_environment(create_ssh_dir: Option<bool>) -> KhResult<EnvReport> {
    env::detect_environment(create_ssh_dir.unwrap_or(false))
}

/// 侧栏状态卡用的精简摘要
#[tauri::command]
fn env_summary() -> KhResult<EnvSummary> {
    env::quick_summary()
}

/// 分平台安装引导
#[tauri::command]
fn get_install_guide() -> InstallGuide {
    env::install_guide()
}

/// Windows 一键启用系统 OpenSSH 客户端
#[tauri::command]
fn enable_windows_openssh() -> KhResult<String> {
    let r = env::enable_windows_openssh();
    audit::log(
        audit::CAT_SECURITY,
        "env.enableOpenSsh",
        "尝试一键启用 Windows OpenSSH 客户端",
        if r.is_ok() { "ok" } else { "fail" },
        "",
        &r.as_ref().err().map(|e| e.to_string()).unwrap_or_default(),
    );
    r
}

/// 确保 `~/.ssh` 存在（引导流程用）
#[tauri::command]
fn ensure_ssh_dir() -> KhResult<String> {
    Ok(paths::pretty(&env::ensure_ssh_dir()?))
}

/// 关键路径一览（设置页展示）
#[tauri::command]
fn get_paths() -> KhResult<serde_json::Value> {
    let ssh = paths::ssh_dir()?;
    Ok(serde_json::json!({
        "sshDir": ssh.to_string_lossy(),
        "prettySshDir": paths::pretty(&ssh),
        "config": paths::ssh_config_path()?.to_string_lossy(),
        "prettyConfig": paths::pretty(&paths::ssh_config_path()?),
        "appData": paths::app_data_dir()?.to_string_lossy(),
        "backups": paths::backup_dir()?.to_string_lossy(),
        "configBackups": paths::config_backup_dir()?.to_string_lossy(),
        "audit": paths::audit_path()?.to_string_lossy(),
        "prefs": paths::prefs_path()?.to_string_lossy(),
        "platform": env::current_platform(),
        "home": paths::home_dir()?.to_string_lossy(),
    }))
}

/* ═══════════════════════════════════════════════════════════════
 * 2. 密钥管理（PRD 3.1.2）
 * ═══════════════════════════════════════════════════════════════ */

/// 扫描 `~/.ssh` 下的全部密钥
#[tauri::command]
fn scan_keys() -> KhResult<Vec<KeyMeta>> {
    keys::scan_keys()
}

/// 生成密钥
#[tauri::command]
fn generate_key(opts: GenerateOpts) -> KhResult<KeyMeta> {
    let algo = opts.algorithm.clone();
    let filename = opts.filename.clone();
    let r = keys::generate_key(&opts);
    audit::log(
        audit::CAT_KEY,
        "key.generate",
        &format!("生成密钥 {}", filename),
        if r.is_ok() { "ok" } else { "fail" },
        &filename,
        &format!(
            "算法 {}；{}",
            algo,
            r.as_ref().err().map(|e| e.to_string()).unwrap_or_default()
        ),
    );
    r
}

/// 读取公钥全文（可复制分发）
#[tauri::command]
fn read_public_key(filename: String) -> KhResult<String> {
    keys::read_public_key(&filename)
}

/// 读取私钥全文。
///
/// 这是最高敏感度的操作：仅在用户显式点击「查看私钥」并二次确认后调用。
/// 每次调用都写审计日志，且返回的字符串由前端在关闭弹窗时立即丢弃。
#[tauri::command]
fn read_private_key(filename: String) -> KhResult<String> {
    let r = keys::read_private_key(&filename);
    audit::log(
        audit::CAT_KEY,
        "key.readPrivate",
        &format!("读取私钥明文 {}", filename),
        if r.is_ok() { "warn" } else { "fail" },
        &filename,
        "用户在界面上显式请求查看私钥",
    );
    r
}

/// 删除密钥（连同 .pub）
#[tauri::command]
fn delete_key(filename: String) -> KhResult<Vec<String>> {
    let r = keys::delete_key(&filename);
    audit::log(
        audit::CAT_KEY,
        "key.delete",
        &format!("删除密钥 {}", filename),
        if r.is_ok() { "warn" } else { "fail" },
        &filename,
        &r.as_ref()
            .map(|v| v.join(", "))
            .unwrap_or_else(|e| e.to_string()),
    );
    r
}

/// 给私钥设置 / 修改口令
#[tauri::command]
fn set_key_passphrase(filename: String, old_passphrase: String, new_passphrase: String) -> KhResult<()> {
    let r = keys::set_passphrase(&filename, &old_passphrase, &new_passphrase);
    audit::log(
        audit::CAT_KEY,
        "key.setPassphrase",
        &format!("为 {} 设置口令保护", filename),
        if r.is_ok() { "ok" } else { "fail" },
        &filename,
        // 口令绝不进日志
        "",
    );
    r
}

/// 移除私钥口令
#[tauri::command]
fn remove_key_passphrase(filename: String, old_passphrase: String) -> KhResult<()> {
    let r = keys::remove_passphrase(&filename, &old_passphrase);
    audit::log(
        audit::CAT_KEY,
        "key.removePassphrase",
        &format!("移除 {} 的口令保护", filename),
        if r.is_ok() { "warn" } else { "fail" },
        &filename,
        "",
    );
    r
}

/* ═══════════════════════════════════════════════════════════════
 * 3. SSH Config（PRD 3.1.3）
 * ═══════════════════════════════════════════════════════════════ */

/// 读取并解析 config
#[tauri::command]
fn read_config() -> KhResult<ConfigDoc> {
    config::read_config()
}

/// 保存 config 全文（Raw 模式）
#[tauri::command]
fn write_config(content: String) -> KhResult<SaveResult> {
    let r = config::write_config(&content);
    audit::log(
        audit::CAT_CONFIG,
        "config.writeRaw",
        "以原文模式保存 SSH config",
        if r.is_ok() { "ok" } else { "fail" },
        "config",
        &r.as_ref()
            .map(|s| {
                format!(
                    "{} 行；备份 {}",
                    s.lines,
                    s.backup.clone().unwrap_or_else(|| "无".into())
                )
            })
            .unwrap_or_else(|e| e.to_string()),
    );
    r
}

/// 保存结构化 Host 列表（Form 模式，只重写托管块）
#[tauri::command]
fn write_hosts(hosts: Vec<HostEntry>) -> KhResult<SaveResult> {
    let n = hosts.len();
    let r = config::write_hosts(&hosts);
    audit::log(
        audit::CAT_CONFIG,
        "config.writeHosts",
        &format!("保存 {} 条 Host 配置", n),
        if r.is_ok() { "ok" } else { "fail" },
        "config",
        "",
    );
    r
}

/// 校验 Host 列表（不写盘）
#[tauri::command]
fn validate_hosts(hosts: Vec<HostEntry>) -> Vec<CheckIssue> {
    config::validate_hosts(&hosts)
}

/// 校验一段 config 原文（不写盘）
#[tauri::command]
fn validate_config(content: String) -> KhResult<Vec<CheckIssue>> {
    let (hosts, _) = config::parse_hosts(&content)?;
    Ok(config::validate_hosts(&hosts))
}

/// 列出 config 备份
#[tauri::command]
fn list_config_backups() -> KhResult<Vec<ConfigBackup>> {
    config::list_backups()
}

/// 回滚 config 到指定备份
#[tauri::command]
fn rollback_config(name: String) -> KhResult<SaveResult> {
    let r = config::rollback_config(&name);
    audit::log(
        audit::CAT_CONFIG,
        "config.rollback",
        &format!("回滚 config 到 {}", name),
        if r.is_ok() { "warn" } else { "fail" },
        &name,
        "",
    );
    r
}

/// 读取某个备份的全文（预览）
#[tauri::command]
fn read_config_backup(name: String) -> KhResult<String> {
    config::read_backup(&name)
}

/// 删除一个 config 备份
#[tauri::command]
fn delete_config_backup(name: String) -> KhResult<()> {
    config::delete_backup(&name)
}

/// 生成示例 config（首次使用引导）
#[tauri::command]
fn sample_config() -> String {
    config::sample_config()
}

/// 提取 config 中被引用的 IdentityFile 列表（密钥库标记「已被 Host 引用」）
#[tauri::command]
fn referenced_identities() -> Vec<String> {
    config::referenced_identities()
}

/// 生成一个不与现有 Host 冲突的别名建议（新增 Host 时预填）
#[tauri::command]
fn suggest_alias(base: String) -> String {
    config::suggest_alias(&base)
}

/// config 备份目录的绝对路径（设置页展示）
#[tauri::command]
fn config_backup_dir_path() -> KhResult<String> {
    Ok(config::backup_dir_path()?.to_string_lossy().to_string())
}

/* ═══════════════════════════════════════════════════════════════
 * 4. 权限管理（PRD 3.1.4 / 安全中心）
 * ═══════════════════════════════════════════════════════════════ */

/// 审计 `~/.ssh` 与其下文件的权限
#[tauri::command]
fn audit_permissions() -> KhResult<Vec<PermInfo>> {
    perms::audit_tree()
}

/// 权限标准值文案（Unix 下为 "700" / "600"；Windows 下仅供参考）
#[tauri::command]
fn perm_standard_label(is_dir: bool) -> String {
    perms::standard_label(is_dir).to_string()
}

/// 修复单个路径的权限
#[tauri::command]
fn fix_path_permission(path: String, is_dir: bool) -> KhResult<()> {
    let p = paths::expand(&path)?;
    let r = perms::fix(&p, is_dir);
    audit::log(
        audit::CAT_PERM,
        "perm.fix",
        &format!("修复权限 {}", paths::pretty(&p)),
        if r.is_ok() { "ok" } else { "fail" },
        &paths::pretty(&p),
        if is_dir { "目录 700" } else { "文件 600" },
    );
    r
}

/// 一键修复 `~/.ssh` 全部权限
#[tauri::command]
fn fix_all_permissions() -> KhResult<usize> {
    let r = perms::fix_tree();
    audit::log(
        audit::CAT_PERM,
        "perm.fixAll",
        "一键修复 ~/.ssh 全部权限",
        if r.is_ok() { "ok" } else { "fail" },
        "~/.ssh",
        &r.as_ref().map(|n| format!("修复 {} 项", n)).unwrap_or_else(|e| e.to_string()),
    );
    r
}

/* ═══════════════════════════════════════════════════════════════
 * 5. 连通性测试（PRD 3.1.6）
 * ═══════════════════════════════════════════════════════════════ */

/// 执行连通测试（会发起真实网络连接，前端需先二次确认）
#[tauri::command]
fn test_connection(opts: TestOpts) -> KhResult<TestResult> {
    conn_test::test_connection(opts)
}

/// 预览将要执行的 ssh 命令（不联网）
#[tauri::command]
fn preview_test_command(opts: TestOpts, timeout_secs: Option<u32>) -> String {
    conn_test::preview_command(opts, timeout_secs.unwrap_or(10))
}

/// 连通测试历史
#[tauri::command]
fn read_test_history(limit: Option<u32>) -> KhResult<Vec<TestHistoryItem>> {
    conn_test::read_history(limit.unwrap_or(20))
}

/// 清空连通测试历史
#[tauri::command]
fn clear_test_history() -> KhResult<()> {
    conn_test::clear_history()
}

/// ssh 客户端是否可用
#[tauri::command]
fn ssh_available() -> bool {
    conn_test::ssh_available()
}

/// 查看某主机在 known_hosts 中的记录
#[tauri::command]
fn known_hosts_entries(host: String) -> Vec<String> {
    conn_test::known_hosts_entries(&host)
}

/// 从 known_hosts 移除某主机
#[tauri::command]
fn remove_known_host(host: String) -> KhResult<usize> {
    conn_test::remove_known_host(&host)
}

/// 当前平台
#[tauri::command]
fn platform() -> String {
    conn_test::platform()
}

/* ═══════════════════════════════════════════════════════════════
 * 6. ssh-agent（PRD 3.1.4）
 * ═══════════════════════════════════════════════════════════════ */

/// agent 状态
#[tauri::command]
fn agent_status() -> AgentStatus {
    agent::status()
}

/// 启动 agent
#[tauri::command]
fn agent_start() -> KhResult<AgentStatus> {
    agent::start_agent()
}

/// 停止 agent
#[tauri::command]
fn agent_stop() -> KhResult<AgentStatus> {
    agent::stop_agent()
}

/// 把密钥加入 agent
#[tauri::command]
fn agent_add_key(filename: String, lifetime: Option<u32>) -> KhResult<AgentStatus> {
    agent::add_key(&filename, lifetime.unwrap_or(0))
}

/// 从 agent 移除密钥
#[tauri::command]
fn agent_remove_key(filename: String) -> KhResult<AgentStatus> {
    agent::remove_key(&filename)
}

/// 清空 agent
#[tauri::command]
fn agent_remove_all() -> KhResult<AgentStatus> {
    agent::remove_all()
}

/// 设置 agent 开机自启
#[tauri::command]
fn agent_set_autostart(enabled: bool) -> KhResult<AgentStatus> {
    agent::set_autostart(enabled)
}

/// 预览 agent 相关命令（透明化）
#[tauri::command]
fn agent_preview_cmd(action: String) -> String {
    agent::preview_cmd(&action)
}

/* ═══════════════════════════════════════════════════════════════
 * 7. Git 全局配置（PRD 3.1.4 后半）
 * ═══════════════════════════════════════════════════════════════ */

/// 读取 Git 全局配置
#[tauri::command]
fn read_git_config() -> KhResult<GitConfig> {
    git::read_git_config()
}

/// 设置 Git 全局用户
#[tauri::command]
fn write_git_user(name: String, email: String) -> KhResult<GitConfig> {
    git::set_user(&name, &email)
}

/// 为某域名指定密钥
#[tauri::command]
fn git_set_host_key(host: String, identity: String, user: String) -> KhResult<GitConfig> {
    git::set_host_key(&host, &identity, &user)
}

/// 移除某域名的密钥映射
#[tauri::command]
fn git_remove_host_key(host: String) -> KhResult<GitConfig> {
    git::remove_host_key(&host)
}

/// 设置全局 core.sshCommand
#[tauri::command]
fn git_set_ssh_command(command: String) -> KhResult<GitConfig> {
    git::set_core_ssh_command(&command)
}

/// 清除墨钥写入的 Git 配置
#[tauri::command]
fn git_clear_managed() -> KhResult<GitConfig> {
    git::clear_managed()
}

/// git 是否可用
#[tauri::command]
fn git_available() -> bool {
    git::git_available()
}

/// 由密钥推荐 core.sshCommand
#[tauri::command]
fn git_recommend_ssh_command(identity: String) -> String {
    git::recommend_ssh_command(&identity)
}

/* ═══════════════════════════════════════════════════════════════
 * 8. 加密备份（PRD 3.1.4 备份）
 * ═══════════════════════════════════════════════════════════════ */

/// 预览将备份的内容
#[tauri::command]
fn preview_backup(include_private: Option<bool>) -> KhResult<BackupPreview> {
    backup::preview_backup(include_private.unwrap_or(true))
}

/// 创建备份
#[tauri::command]
fn create_backup(opts: BackupOpts) -> KhResult<BackupResult> {
    backup::create_backup(opts)
}

/// 列出备份
#[tauri::command]
fn list_backups() -> KhResult<Vec<BackupItem>> {
    backup::list_backups()
}

/// 查看备份内容
#[tauri::command]
fn inspect_backup(name: String, passphrase: Option<String>) -> KhResult<Vec<String>> {
    backup::inspect_backup(&name, &passphrase.unwrap_or_default())
}

/// 恢复备份
#[tauri::command]
fn restore_backup(name: String, passphrase: Option<String>, overwrite: Option<bool>) -> KhResult<BackupResult> {
    backup::restore_backup(
        &name,
        &passphrase.unwrap_or_default(),
        overwrite.unwrap_or(false),
    )
}

/// 删除备份
#[tauri::command]
fn delete_backup(name: String) -> KhResult<()> {
    backup::delete_backup(&name)
}

/// 备份目录路径
#[tauri::command]
fn backup_dir_path() -> KhResult<String> {
    backup::backup_dir_path()
}

/* ═══════════════════════════════════════════════════════════════
 * 9. 审计日志（PRD 3.1.5）
 * ═══════════════════════════════════════════════════════════════ */

/// 读取审计日志
#[tauri::command]
fn read_audit_log(query: Option<AuditQuery>) -> KhResult<Vec<AuditEntry>> {
    audit::read_audit(query.unwrap_or_default())
}

/// 审计统计
#[tauri::command]
fn audit_stats() -> KhResult<AuditStats> {
    audit::audit_stats()
}

/// 导出审计日志为 CSV
#[tauri::command]
fn export_audit_csv(out_dir: Option<String>) -> KhResult<String> {
    let p = audit::export_csv(out_dir)?;
    Ok(paths::pretty(std::path::Path::new(&p)))
}

/// 按保留期清理审计日志
#[tauri::command]
fn prune_audit(days: u32) -> KhResult<usize> {
    audit::prune(days)
}

/// 清空审计日志
#[tauri::command]
fn clear_audit() -> KhResult<()> {
    audit::clear_audit()
}

/// 前端主动写入一条审计（如「用户复制了公钥」）
#[tauri::command]
fn append_audit(
    category: String,
    action: String,
    message: String,
    result: String,
    target: Option<String>,
    detail: Option<String>,
) -> KhResult<()> {
    audit::append(
        &category,
        &action,
        &message,
        &result,
        &target.unwrap_or_default(),
        &detail.unwrap_or_default(),
    )
}

/* ═══════════════════════════════════════════════════════════════
 * 10. 偏好设置（PRD 3.1.5）
 * ═══════════════════════════════════════════════════════════════ */

/// 读取偏好
#[tauri::command]
fn read_prefs() -> KhResult<Prefs> {
    prefs::read_prefs()
}

/// 写入偏好
#[tauri::command]
fn write_prefs(prefs: Prefs) -> KhResult<Prefs> {
    prefs::write_prefs(&prefs)
}

/// 局部更新偏好
#[tauri::command]
fn patch_prefs(patch: Prefs) -> KhResult<Prefs> {
    prefs::patch_prefs(patch)
}

/// 重置偏好
#[tauri::command]
fn reset_prefs() -> KhResult<Prefs> {
    prefs::reset_prefs()
}

/* ═══════════════════════════════════════════════════════════════
 * 启动
 * ═══════════════════════════════════════════════════════════════ */

/// 应用入口（由 main.rs 调用）
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|_app| {
            // 启动即记录一次会话，便于审计追溯（不记录任何隐私内容）
            audit::log(
                audit::CAT_SECURITY,
                "app.start",
                "墨钥启动",
                "ok",
                "",
                &format!(
                    "平台 {}；版本 {}",
                    env::current_platform(),
                    env!("CARGO_PKG_VERSION")
                ),
            );
            // 按偏好清理过期审计
            if let Ok(p) = prefs::read_prefs() {
                if p.audit_enabled && p.audit_retention_days > 0 {
                    let _ = audit::prune(p.audit_retention_days);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 环境
            detect_environment,
            env_summary,
            get_install_guide,
            enable_windows_openssh,
            ensure_ssh_dir,
            get_paths,
            // 密钥
            scan_keys,
            generate_key,
            read_public_key,
            read_private_key,
            delete_key,
            set_key_passphrase,
            remove_key_passphrase,
            // config
            read_config,
            write_config,
            write_hosts,
            validate_hosts,
            validate_config,
            list_config_backups,
            rollback_config,
            read_config_backup,
            delete_config_backup,
            sample_config,
            referenced_identities,
            suggest_alias,
            config_backup_dir_path,
            // 权限
            audit_permissions,
            perm_standard_label,
            fix_path_permission,
            fix_all_permissions,
            // 连通测试
            test_connection,
            preview_test_command,
            read_test_history,
            clear_test_history,
            ssh_available,
            known_hosts_entries,
            remove_known_host,
            platform,
            // agent
            agent_status,
            agent_start,
            agent_stop,
            agent_add_key,
            agent_remove_key,
            agent_remove_all,
            agent_set_autostart,
            agent_preview_cmd,
            // git
            read_git_config,
            write_git_user,
            git_set_host_key,
            git_remove_host_key,
            git_set_ssh_command,
            git_clear_managed,
            git_available,
            git_recommend_ssh_command,
            // 备份
            preview_backup,
            create_backup,
            list_backups,
            inspect_backup,
            restore_backup,
            delete_backup,
            backup_dir_path,
            // 审计
            read_audit_log,
            audit_stats,
            export_audit_csv,
            prune_audit,
            clear_audit,
            append_audit,
            // 偏好
            read_prefs,
            write_prefs,
            patch_prefs,
            reset_prefs,
        ])
        .run(tauri::generate_context!())
        .expect("墨钥启动失败");
}
