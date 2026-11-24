/**
 * 全局状态：环境自检结果 + 偏好设置
 *
 * 设计原则：单一数据源。侧栏、诊断页、安全中心都从这一个 store 读取，
 * 避免各自调接口导致状态不一致（例如安全中心修完权限，侧栏没更新）。
 */
import { reactive, readonly, ref } from 'vue'
import * as api from './api'

/** 环境摘要（侧栏 + 概览用） */
const env = reactive({
  loaded: false,
  loading: false,
  error: null,
  /** `windows` | `macos` | `linux` */
  platform: '',
  sshLabel: '检测中…',
  sshAvailable: false,
  sshVersion: null,
  gitVersion: null,
  permLabel: '—',
  permCompliant: true,
  agentLabel: '检测中…',
  agentRunning: false,
  /** agent 已加载的密钥数（侧栏精简展示用） */
  agentCount: 0,
  keyCount: 0,
  hasConfig: false,
  /** 全量报告（诊断页用） */
  report: null,
})

/** 偏好设置 */
const prefs = reactive({
  loaded: false,
  /** 后端是否成功读过偏好；false 表示后端未就绪（此时不能劫持到引导页） */
  available: false,
  theme: 'system',
  titlebarStyle: 'auto',
  autoCheckUpdates: false,
  locale: 'zh-CN',
  autoFixPerms: true,
  autoBackupConfig: true,
  auditEnabled: true,
  auditRetentionDays: 90,
  autocheckOnStart: true,
  defaultAlgorithm: 'ed25519',
  defaultRsaBits: 4096,
  defaultComment: '',
  autoAddToAgent: false,
  agentAutostart: false,
  connectTimeoutSecs: 10,
  confirmBeforeConnect: true,
  strictCsprng: true,
  confirmDeleteTyped: true,
  onboardedAt: null,
  density: 'comfortable',
  sidebarCollapsed: false,
  ackDanger: false,
})

/** 把后端返回的 snake/camel 差异统一成 camel（后端已 camelCase，这里做防御） */
function normalize(obj) {
  if (!obj || typeof obj !== 'object') return obj
  return obj
}

/** 语义类：给 pill 组件用 */
function toneOf(kind, ok) {
  if (!ok) return kind === 'agent' ? 'idle' : 'warn'
  return 'ok'
}

/** 刷新环境摘要 */
async function refreshEnv() {
  env.loading = true
  env.error = null
  try {
    const summary = normalize(await api.envSummary())
    env.platform = summary.platform || ''
    env.sshLabel = summary.sshLabel || '未检测到 ssh'
    env.sshAvailable = !!summary.sshAvailable
    env.sshVersion = summary.sshVersion || null
    env.gitVersion = summary.gitVersion || null
    env.permLabel = summary.permLabel || '—'
    env.permCompliant = !!summary.permCompliant
    env.agentLabel = summary.agentLabel || '未知'
    env.agentRunning = !!summary.agentRunning
    env.agentCount = summary.agentCount || 0
    env.keyCount = summary.keyCount || 0
    env.hasConfig = !!summary.hasConfig
    env.loaded = true
  } catch (e) {
    env.error = e
    env.sshLabel = '后端未就绪'
    env.agentLabel = '后端未就绪'
    env.loaded = true
  } finally {
    env.loading = false
  }
}

/** 拉取全量环境报告（诊断页/引导页用） */
async function loadReport(createSshDir = false) {
  const report = normalize(await api.detectEnvironment(createSshDir))
  env.report = report
  env.platform = report.platform || env.platform
  if (report.preferred) {
    env.sshLabel = report.preferred.label
    env.sshAvailable = true
    env.sshVersion = report.preferred.version || null
  } else {
    env.sshLabel = '未检测到 ssh'
    env.sshAvailable = false
  }
  env.gitVersion = report.git?.version || null
  if (report.sshDir) {
    env.permLabel = report.sshDir.permDisplay || '—'
    env.permCompliant = !!report.sshDir.permCompliant
    env.keyCount = report.sshDir.keyCount || 0
    env.hasConfig = !!report.sshDir.hasConfig
  }
  if (report.agent) {
    env.agentRunning = !!report.agent.running
    env.agentCount = report.agent.identities?.length || 0
    env.agentLabel = report.agent.running
      ? report.agent.identities?.length
        ? `agent 已加载 ${report.agent.identities.length} 把密钥`
        : 'agent 已运行，未加载密钥'
      : 'agent 未运行'
  }
  return report
}

/** 单独刷新 agent（安全中心操作后调用） */
async function refreshAgent() {
  try {
    const st = normalize(await api.agentStatus())
    env.agentRunning = !!st.running
    env.agentCount = st.count || 0
    env.agentLabel = st.label || ''
    return st
  } catch {
    return null
  }
}

/** 读取偏好 */
async function loadPrefs() {
  try {
    const p = normalize(await api.readPrefs())
    Object.assign(prefs, p)
    prefs.loaded = true
    prefs.available = true
    return p
  } catch {
    prefs.loaded = true
    prefs.available = false
    return null
  }
}

/** 保存偏好（整对象覆盖） */
async function savePrefs(patch = {}) {
  const payload = { ...toRawPrefs(), ...patch }
  const saved = normalize(await api.writePrefs(payload))
  Object.assign(prefs, saved)
  return saved
}

/** 取出纯偏好对象（去掉 store 自身字段） */
function toRawPrefs() {
  return {
    theme: prefs.theme,
    titlebarStyle: prefs.titlebarStyle,
    autoCheckUpdates: prefs.autoCheckUpdates,
    locale: prefs.locale,
    autoFixPerms: prefs.autoFixPerms,
    autoBackupConfig: prefs.autoBackupConfig,
    auditEnabled: prefs.auditEnabled,
    auditRetentionDays: prefs.auditRetentionDays,
    autocheckOnStart: prefs.autocheckOnStart,
    defaultAlgorithm: prefs.defaultAlgorithm,
    defaultRsaBits: prefs.defaultRsaBits,
    defaultComment: prefs.defaultComment,
    autoAddToAgent: prefs.autoAddToAgent,
    agentAutostart: prefs.agentAutostart,
    connectTimeoutSecs: prefs.connectTimeoutSecs,
    confirmBeforeConnect: prefs.confirmBeforeConnect,
    strictCsprng: true,
    confirmDeleteTyped: prefs.confirmDeleteTyped,
    onboardedAt: prefs.onboardedAt,
    density: prefs.density,
    sidebarCollapsed: prefs.sidebarCollapsed,
    ackDanger: prefs.ackDanger,
  }
}

/** 是否完成了首次引导 */
const onboarded = ref(false)

/** 初始化：并发拉取环境与偏好 */
async function init() {
  const [p] = await Promise.allSettled([loadPrefs(), refreshEnv()])
  if (p.status === 'fulfilled' && p.value?.onboardedAt) {
    onboarded.value = true
  }
  return onboarded.value
}

/** 标记引导完成 */
async function markOnboarded() {
  const at = new Date().toISOString()
  await savePrefs({ onboardedAt: at }).catch(() => {})
  onboarded.value = true
}

/** 计算属性风格的派生量 */
const derived = {
  /** ssh 状态 pill 的语义类 */
  get sshTone() {
    return env.sshAvailable ? 'ok' : 'danger'
  },
  get permTone() {
    return env.permCompliant ? 'ok' : 'warn'
  },
  get agentTone() {
    if (!env.agentRunning) return 'idle'
    return env.agentCount === 0 ? 'warn' : 'ok'
  },
  get gitTone() {
    return env.gitVersion ? 'ok' : 'idle'
  },
  /** 侧栏「环境自检」是否需要提醒 */
  get needsAttention() {
    return !env.sshAvailable || !env.permCompliant
  },
}

export function useEnvStore() {
  return {
    env: readonly(env),
    rawEnv: env,
    prefs: readonly(prefs),
    rawPrefs: prefs,
    onboarded,
    derived,
    refreshEnv,
    loadReport,
    refreshAgent,
    loadPrefs,
    savePrefs,
    init,
    markOnboarded,
    toneOf,
  }
}
