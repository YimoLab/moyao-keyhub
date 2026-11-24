/**
 * 墨钥 KeyHub · 后端桥接层
 *
 * 全部真实能力（环境检测 / 密钥生成 / config 读写 / 权限修复 / 连通测试 /
 * agent / Git / 备份 / 审计 / 偏好）都通过 Tauri invoke 调用 Rust 侧实现。
 *
 * 关键设计：绝不静默降级为假数据。若后端不可用，抛出结构化错误，
 * 由调用方显式呈现「后端未连接」，避免出现"看着成功、其实没写盘"的危险假象。
 *
 * 错误约定：Rust 侧 `KhError` 序列化为 `"CODE:人类可读消息"`，
 * 本层拆成 `err.code`（可编程）与 `err.message`（可直接展示的文案）。
 */
import { invoke } from '@tauri-apps/api/core'

export function isBackendAvailable() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

export class BackendUnavailableError extends Error {
  constructor() {
    super('后端未连接：当前运行在浏览器预览模式，SSH 相关功能不可用。')
    this.name = 'BackendUnavailableError'
    this.code = 'BACKEND_UNAVAILABLE'
  }
}

/**
 * 统一调用入口。
 * @param {string} cmd Rust 侧 #[tauri::command] 名称
 * @param {object} args 参数（键名用 camelCase，Tauri 自动转 snake_case）
 */
async function call(cmd, args = {}) {
  if (!isBackendAvailable()) throw new BackendUnavailableError()
  try {
    return await invoke(cmd, args)
  } catch (err) {
    const raw = typeof err === 'string' ? err : (err && err.message) || String(err)
    const sep = raw.indexOf(':')
    if (sep > 0 && /^[A-Z_]+$/.test(raw.slice(0, sep))) {
      const e = new Error(raw.slice(sep + 1))
      e.code = raw.slice(0, sep)
      throw e
    }
    const e = new Error(raw)
    e.code = 'UNKNOWN'
    throw e
  }
}

/* ═══════════════════════════════════════════════════════════════
 * 1. 环境自检
 * ═══════════════════════════════════════════════════════════════ */

/** 全量环境自检：{ platform, sshClients[], preferred, git, sshDir, agent, needsInstallGuide } */
export function detectEnvironment(createSshDir = false) {
  return call('detect_environment', { createSshDir })
}

/** 侧栏状态卡用的精简摘要 */
export function envSummary() {
  return call('env_summary')
}

/** 平台相关的 OpenSSH 安装引导 */
export function getInstallGuide() {
  return call('get_install_guide')
}

/** 一键启用 Windows OpenSSH 可选功能（仅 Windows） */
export function enableWindowsOpenSsh() {
  return call('enable_windows_openssh')
}

/** 确保 ~/.ssh 存在，返回美化路径 */
export function ensureSshDir() {
  return call('ensure_ssh_dir')
}

/** 关键路径一览 */
export function getPaths() {
  return call('get_paths')
}

/* ═══════════════════════════════════════════════════════════════
 * 2. 密钥管理
 * ═══════════════════════════════════════════════════════════════ */

/** 扫描 ~/.ssh 下全部密钥 */
export function scanKeys() {
  return call('scan_keys')
}

/**
 * 生成密钥。
 * @param {{algorithm:'ed25519'|'rsa', bits?:number, filename:string, comment:string,
 *          passphrase?:string}} opts
 */
export function generateKey(opts) {
  return call('generate_key', { opts })
}

/** 读取公钥全文（可公开分发） */
export function readPublicKey(filename) {
  return call('read_public_key', { filename })
}

/** 读取私钥明文（最高敏感度）。调用方必须先完成二次确认。 */
export function readPrivateKey(filename) {
  return call('read_private_key', { filename })
}

/** 删除密钥（连同 .pub） */
export function deleteKey(filename) {
  return call('delete_key', { filename })
}

/** 设置 / 修改私钥口令 */
export function setKeyPassphrase(filename, oldPassphrase, newPassphrase) {
  return call('set_key_passphrase', { filename, oldPassphrase, newPassphrase })
}

/** 移除私钥口令 */
export function removeKeyPassphrase(filename, oldPassphrase) {
  return call('remove_key_passphrase', { filename, oldPassphrase })
}

/* ═══════════════════════════════════════════════════════════════
 * 3. SSH Config
 * ═══════════════════════════════════════════════════════════════ */

/** 解析 config 为 { exists, path, hosts[], raw, globals[], hasManagedBlock } */
export function readConfig() {
  return call('read_config')
}

/** 以原文模式保存 config（后端自动备份 + 设 600） */
export function writeConfig(content) {
  return call('write_config', { content })
}

/** 以结构化 Host 列表保存（只重写墨钥托管块） */
export function writeHosts(hosts) {
  return call('write_hosts', { hosts })
}

/** 校验 Host 列表，返回问题清单 */
export function validateHosts(hosts) {
  return call('validate_hosts', { hosts })
}

/** 校验一段 config 原文 */
export function validateConfig(content) {
  return call('validate_config', { content })
}

/** 列出 config 历史备份 */
export function listConfigBackups() {
  return call('list_config_backups')
}

/** 回滚 config 到指定备份 */
export function rollbackConfig(name) {
  return call('rollback_config', { name })
}

/** 读取备份全文（预览） */
export function readConfigBackup(name) {
  return call('read_config_backup', { name })
}

/** 删除一个备份 */
export function deleteConfigBackup(name) {
  return call('delete_config_backup', { name })
}

/** 生成示例 config（首次引导） */
export function sampleConfig() {
  return call('sample_config')
}

/** config 中被引用的 IdentityFile 列表（密钥库标记「已被引用」） */
export function referencedIdentities() {
  return call('referenced_identities')
}

/** 生成不与现有 Host 冲突的别名建议 */
export function suggestAlias(base) {
  return call('suggest_alias', { base })
}

/** config 备份目录绝对路径 */
export function configBackupDirPath() {
  return call('config_backup_dir_path')
}

/* ═══════════════════════════════════════════════════════════════
 * 4. 权限
 * ═══════════════════════════════════════════════════════════════ */

/** 体检 ~/.ssh 与相关文件的权限，返回 PermInfo 列表 */
export function auditPermissions() {
  return call('audit_permissions')
}

/** 权限标准值文案（"700" / "600"） */
export function permStandardLabel(isDir) {
  return call('perm_standard_label', { isDir })
}

/** 修复单个路径权限 */
export function fixPathPermission(path, isDir) {
  return call('fix_path_permission', { path, isDir })
}

/** 一键修复全部不合规权限，返回修复项数 */
export function fixAllPermissions() {
  return call('fix_all_permissions')
}

/* ═══════════════════════════════════════════════════════════════
 * 5. 连通性测试
 * ═══════════════════════════════════════════════════════════════ */

/**
 * 执行连通测试（会发起真实网络连接）。
 * @param {{alias?:string, hostname?:string, port?:number, user?:string,
 *          identity?:string, timeoutSecs?:number, verbose?:boolean}} opts
 */
export function testConnection(opts) {
  return call('test_connection', { opts })
}

/** 预览将要执行的 ssh 命令（不联网） */
export function previewTestCommand(opts, timeoutSecs = 10) {
  return call('preview_test_command', { opts, timeoutSecs })
}

/** 连通测试历史 */
export function readTestHistory(limit = 20) {
  return call('read_test_history', { limit })
}

/** 清空测试历史 */
export function clearTestHistory() {
  return call('clear_test_history')
}

/** ssh 客户端是否可用 */
export function sshAvailable() {
  return call('ssh_available')
}

/** 查看某主机在 known_hosts 中的记录 */
export function knownHostsEntries(host) {
  return call('known_hosts_entries', { host })
}

/** 从 known_hosts 移除某主机 */
export function removeKnownHost(host) {
  return call('remove_known_host', { host })
}

/** 当前平台标识 */
export function platform() {
  return call('platform')
}

/* ═══════════════════════════════════════════════════════════════
 * 6. ssh-agent
 * ═══════════════════════════════════════════════════════════════ */

export function agentStatus() {
  return call('agent_status')
}

export function agentStart() {
  return call('agent_start')
}

export function agentStop() {
  return call('agent_stop')
}

export function agentAddKey(filename, lifetime = 0) {
  return call('agent_add_key', { filename, lifetime })
}

export function agentRemoveKey(filename) {
  return call('agent_remove_key', { filename })
}

export function agentRemoveAll() {
  return call('agent_remove_all')
}

export function agentSetAutostart(enabled) {
  return call('agent_set_autostart', { enabled })
}

export function agentPreviewCmd(action) {
  return call('agent_preview_cmd', { action })
}

/* ═══════════════════════════════════════════════════════════════
 * 7. Git 全局配置
 * ═══════════════════════════════════════════════════════════════ */

export function readGitConfig() {
  return call('read_git_config')
}

export function writeGitUser(name, email) {
  return call('write_git_user', { name, email })
}

export function gitSetHostKey(host, identity, user = '') {
  return call('git_set_host_key', { host, identity, user })
}

export function gitRemoveHostKey(host) {
  return call('git_remove_host_key', { host })
}

export function gitSetSshCommand(command) {
  return call('git_set_ssh_command', { command })
}

export function gitClearManaged() {
  return call('git_clear_managed')
}

export function gitAvailable() {
  return call('git_available')
}

export function gitRecommendSshCommand(identity) {
  return call('git_recommend_ssh_command', { identity })
}

/* ═══════════════════════════════════════════════════════════════
 * 8. 加密备份
 * ═══════════════════════════════════════════════════════════════ */

/** 预览将备份的内容 */
export function previewBackup(includePrivate = true) {
  return call('preview_backup', { includePrivate })
}

/**
 * 创建备份。
 * @param {{passphrase?:string, label?:string, includePrivate?:boolean, outDir?:string}} opts
 */
export function createBackup(opts) {
  return call('create_backup', { opts })
}

export function listBackups() {
  return call('list_backups')
}

export function inspectBackup(name, passphrase = '') {
  return call('inspect_backup', { name, passphrase })
}

export function restoreBackup(name, passphrase = '', overwrite = false) {
  return call('restore_backup', { name, passphrase, overwrite })
}

export function deleteBackup(name) {
  return call('delete_backup', { name })
}

export function backupDirPath() {
  return call('backup_dir_path')
}

/* ═══════════════════════════════════════════════════════════════
 * 9. 审计日志
 * ═══════════════════════════════════════════════════════════════ */

/**
 * 读取审计日志。
 * @param {{category?:string, result?:string, keyword?:string, limit?:number}} query
 */
export function readAuditLog(query = {}) {
  return call('read_audit_log', { query })
}

/**
 * 分页读取审计日志。
 * @param {{category?:string, result?:string, keyword?:string, offset?:number, limit?:number}} query
 * @returns {Promise<{entries:Array, total:number}>}
 */
export function readAuditPage(query = {}) {
  return call('read_audit_page', { query })
}

export function auditStats() {
  return call('audit_stats')
}

/** 导出为 CSV，返回美化路径 */
export function exportAuditCsv(outDir = '') {
  return call('export_audit_csv', { outDir })
}

export function pruneAudit(days) {
  return call('prune_audit', { days })
}

export function clearAudit() {
  return call('clear_audit')
}

/** 前端主动留痕（如「用户复制了公钥」） */
export function appendAudit(category, action, message, result = 'ok', target = '', detail = '') {
  return call('append_audit', { category, action, message, result, target, detail })
}

/* ═══════════════════════════════════════════════════════════════
 * 10. 偏好设置
 * ═══════════════════════════════════════════════════════════════ */

export function readPrefs() {
  return call('read_prefs')
}

export function writePrefs(prefs) {
  return call('write_prefs', { prefs })
}

export function patchPrefs(patch) {
  return call('patch_prefs', { patch })
}

export function resetPrefs() {
  return call('reset_prefs')
}

/* ═══════════════════════════════════════════════════════════════
 * 兼容别名（供早期视图代码引用）
 * ═══════════════════════════════════════════════════════════════ */

export const probeAgent = agentStatus
export const addKeyToAgent = agentAddKey
export const removeKeyFromAgent = agentRemoveKey
export const setAgentAutostart = agentSetAutostart
export const fixPermissions = fixAllPermissions
