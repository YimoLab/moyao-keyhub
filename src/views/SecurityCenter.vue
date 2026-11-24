<script setup>
/**
 * 安全中心（PRD 3.1.4 / 3.1.5）
 *
 * 组成：
 *   - 安全体检：权限、口令、agent、备份四项，汇总为 0-100 评分（42px mono）
 *   - ssh-agent：状态、已加载密钥、启动/停止、自启开关
 *   - 安全策略：来自偏好的开关（自动修复权限、自动备份、审计）
 *   - 加密备份：创建、列出、恢复、删除
 *   - 审计日志：筛选、CSV 导出、清理
 */
import { ref, computed, onMounted } from 'vue'
import * as api from '../lib/api'
import { useToast } from '../lib/toast'
import { useEnvStore } from '../lib/store'
import Icon from '../design/vue/ui/Icon.vue'
import PageHead from '../design/vue/ui/PageHead.vue'
import SelectMenu from '../design/vue/ui/SelectMenu.vue'
import TablePager from '../design/vue/ui/TablePager.vue'
import { usePager } from '../design/vue/lib/usePager'
import AlertBox from '../design/vue/ui/AlertBox.vue'
import FieldInput from '../design/vue/ui/FieldInput.vue'
import AppModal from '../components/AppModal.vue'
import PageSkeleton from '../components/PageSkeleton.vue'

const toast = useToast()
const store = useEnvStore()

/* ══════════════════ 安全体检 ══════════════════ */
const loading = ref(true)
const loadError = ref(null)
const perms = ref([])
const keys = ref([])
const agent = ref(null)
const backups = ref([])

async function loadAll() {
  loading.value = true
  loadError.value = null
  const results = await Promise.allSettled([
    (async () => (perms.value = (await api.auditPermissions()) || []))(),
    (async () => (keys.value = (await api.scanKeys()) || []))(),
    (async () => (agent.value = await api.agentStatus()))(),
    (async () => (backups.value = (await api.listBackups()) || []))(),
  ])
  const failed = results.filter((r) => r.status === 'rejected')
  if (failed.length) loadError.value = failed[0].reason
  loading.value = false
}

/* 备份列表分页：数据在本地，客户端切片即可 */
const {
  page: backupPage,
  pageSize: backupPageSize,
  paged: pagedBackups,
  total: backupTotal,
} = usePager(computed(() => backups.value))

/** 每页条数变化：回到第一页，避免停留在不存在的页码 */
function onBackupPageSize(n) {
  backupPageSize.value = n
  backupPage.value = 1
}

/* 体检项 */
const checks = computed(() => {
  const dirCheck = perms.value.find((p) => p.isDir && p.name.includes('.ssh'))
  const badPerms = perms.value.filter((p) => p.needFix)
  const noPass = keys.value.filter((k) => !k.hasPassphrase)
  const hasBackup = backups.value.length > 0
  const agentOk = !!agent.value?.running
  // 任一数据源失败时，所有结论都不可信：统一降级为 unknown
  const failed = !!loadError.value

  const items = [
    {
      key: 'dir',
      icon: 'folder',
      title: '~/.ssh 目录权限',
      ok: !dirCheck?.needFix,
      level: dirCheck?.needFix ? 'error' : 'ok',
      value: dirCheck?.display || '—',
      expect: '应为 700',
      detail: dirCheck?.needFix
        ? '目录权限过宽，同机其他用户可能读取你的密钥'
        : '目录仅属主可访问，符合最佳实践',
      action: dirCheck?.needFix ? 'fix-perms' : null,
      actionLabel: '修复为 700',
    },
    {
      key: 'files',
      icon: 'key',
      title: '密钥文件权限',
      ok: badPerms.length === 0,
      level: badPerms.length ? 'error' : 'ok',
      value: badPerms.length ? `${badPerms.length} 项异常` : '全部合规',
      expect: '私钥应为 600',
      detail: badPerms.length
        ? 'OpenSSH 可能拒绝加载权限过宽的私钥，且存在被读取风险'
        : '所有密钥文件仅属主可读写',
      action: badPerms.length ? 'fix-perms' : null,
      actionLabel: `修复 ${badPerms.length} 项`,
    },
    {
      key: 'passphrase',
      icon: 'lock',
      title: '口令保护覆盖',
      ok: noPass.length === 0,
      level: noPass.length ? 'warn' : 'ok',
      value: `${keys.value.length - noPass.length} / ${keys.value.length}`,
      expect: '建议全部设置口令',
      detail: noPass.length
        ? `有 ${noPass.length} 把密钥未设口令，一旦文件被复制即可直接使用`
        : '所有密钥都受口令保护',
      action: noPass.length ? 'review-pass' : null,
      actionLabel: '查看未保护的密钥',
    },
    {
      key: 'agent',
      icon: 'shield',
      title: 'ssh-agent 状态',
      ok: agentOk,
      level: agentOk ? 'ok' : 'info',
      value: agent.value?.running
        ? `已加载 ${agent.value.count} 把`
        : '未运行',
      expect: '建议启用（可托管带口令的密钥）',
      detail: agentOk
        ? 'agent 可用，带口令的密钥无需重复输入'
        : '未运行 agent 时，带口令密钥在 git 等场景会认证失败',
      action: agentOk ? null : 'start-agent',
      actionLabel: '启动 agent',
    },
    {
      key: 'backup',
      icon: 'clock',
      title: '备份状态',
      ok: hasBackup,
      level: hasBackup ? 'ok' : 'warn',
      value: hasBackup ? `${backups.value.length} 份备份` : '无备份',
      expect: '建议至少保留一份加密备份',
      detail: hasBackup
        ? '已有可用备份，误删密钥时可快速恢复'
        : '没有备份。硬盘故障或误删后密钥将无法找回',
      action: hasBackup ? null : 'create-backup',
      actionLabel: '立即创建备份',
    },
  ]

  if (!failed) return items
  return items.map((c) => ({
    ...c,
    ok: null,
    level: 'unknown',
    value: '无法评估',
    detail: '后端未连接，暂时无法读取该项状态',
    action: null,
  }))
})

/** 安全评分：0-100；数据不可用时返回 null，不伪造分数 */
const score = computed(() => {
  if (loadError.value) return null
  let s = 100
  const dirCheck = perms.value.find((p) => p.isDir && p.name.includes('.ssh'))
  if (dirCheck?.needFix) s -= 25
  const badPerms = perms.value.filter((p) => p.needFix && !p.isDir)
  s -= Math.min(25, badPerms.length * 8)
  const noPass = keys.value.filter((k) => !k.hasPassphrase).length
  if (keys.value.length) s -= Math.min(20, Math.round((noPass / keys.value.length) * 20))
  if (!agent.value?.running) s -= 10
  if (!backups.value.length) s -= 15
  return Math.max(0, Math.min(100, s))
})

const scoreLevel = computed(() => {
  if (score.value === null) return { label: '无法评估', tone: 'idle', desc: '后端未连接，当前数据不可用' }
  if (score.value >= 90) return { label: '很好', tone: 'ok', desc: '安全基线全部达标' }
  if (score.value >= 70) return { label: '尚可', tone: 'warn', desc: '有若干项建议改进' }
  return { label: '需要关注', tone: 'danger', desc: '存在明显风险，建议逐项处理' }
})

/** 各维度得分条 */
const bars = computed(() => {
  if (loadError.value) return []
  return [
    { label: '权限', value: permScore.value },
    { label: '口令', value: passScore.value },
    { label: 'Agent', value: agent.value?.running ? 100 : 0 },
    { label: '备份', value: backups.value.length ? 100 : 0 },
  ]
})

const permScore = computed(() => {
  const dirCheck = perms.value.find((p) => p.isDir && p.name.includes('.ssh'))
  const bad = perms.value.filter((p) => p.needFix && !p.isDir).length
  let v = 100
  if (dirCheck?.needFix) v -= 50
  v -= Math.min(50, bad * 15)
  return Math.max(0, v)
})

const passScore = computed(() => {
  if (!keys.value.length) return 100
  const noPass = keys.value.filter((k) => !k.hasPassphrase).length
  return Math.round(((keys.value.length - noPass) / keys.value.length) * 100)
})

/* ══════════════════ 体检动作 ══════════════════ */
const busy = ref('')

async function runCheckAction(action) {
  if (action === 'fix-perms') {
    busy.value = 'fix-perms'
    try {
      const n = await api.fixAllPermissions()
      toast.push(`已修复 ${n} 项权限`, 'success')
      await loadAll()
      await store.refreshEnv()
    } catch (e) {
      toast.push(e.message || '修复失败', 'danger')
    } finally {
      busy.value = ''
    }
  } else if (action === 'start-agent') {
    busy.value = 'start-agent'
    try {
      const st = await api.agentStart()
      toast.push(st.running ? 'ssh-agent 已启动' : '启动未成功', st.running ? 'success' : 'danger')
      await loadAgent()
    } catch (e) {
      toast.push(e.message || '启动失败', 'danger')
    } finally {
      busy.value = ''
    }
  } else if (action === 'create-backup') {
    openBackup()
  } else if (action === 'review-pass') {
    noPassFilter.value = true
    document.getElementById('key-section')?.scrollIntoView({ behavior: 'smooth' })
  }
}

/* ══════════════════ ssh-agent ══════════════════ */
const agentBusy = ref('')

async function loadAgent() {
  try {
    agent.value = await api.agentStatus()
    await store.refreshAgent()
  } catch {
    agent.value = null
  }
}

async function toggleAgent() {
  agentBusy.value = 'toggle'
  try {
    if (agent.value?.running) {
      await api.agentStop()
      toast.push('ssh-agent 已停止', 'info')
    } else {
      await api.agentStart()
      toast.push('ssh-agent 已启动', 'success')
    }
    await loadAgent()
  } catch (e) {
    toast.push(e.message || '操作失败', 'danger')
  } finally {
    agentBusy.value = ''
  }
}

async function toggleAutostart() {
  if (!agent.value) return
  agentBusy.value = 'autostart'
  const next = !agent.value.autostart
  try {
    const st = await api.agentSetAutostart(next)
    agent.value = st
    toast.push(next ? '已启用 agent 开机自启' : '已关闭 agent 开机自启', 'success')
    await store.savePrefs({ agentAutostart: next })
  } catch (e) {
    toast.push(e.message || '设置失败（Windows 下可能需要管理员权限）', 'danger')
  } finally {
    agentBusy.value = ''
  }
}

async function agentAddKey(k) {
  agentBusy.value = k.filename
  try {
    await api.agentAddKey(k.filename, 0)
    toast.push(`已加入 ${k.filename}`, 'success')
    await loadAgent()
  } catch (e) {
    toast.push(e.message || '加入失败', 'danger')
  } finally {
    agentBusy.value = ''
  }
}

async function agentClear() {
  agentBusy.value = 'clear'
  try {
    await api.agentRemoveAll()
    toast.push('已清空 agent 中的全部密钥', 'info')
    await loadAgent()
  } catch (e) {
    toast.push(e.message || '清空失败', 'danger')
  } finally {
    agentBusy.value = ''
  }
}

const noPassFilter = ref(false)

/* ══════════════════ 安全策略 ══════════════════ */
const savingPref = ref('')

async function setPref(key, value) {
  savingPref.value = key
  try {
    await store.savePrefs({ [key]: value })
    toast.push('策略已更新', 'success')
  } catch (e) {
    toast.push(e.message || '保存失败', 'danger')
  } finally {
    savingPref.value = ''
  }
}

/* ══════════════════ 备份 ══════════════════ */
const backupOpen = ref(false)
const backupForm = ref({ passphrase: '', confirm: '', includePrivate: true, label: '' })
const backupPreview = ref(null)
const creating = ref(false)
const showBackupPass = ref(false)

async function openBackup() {
  backupForm.value = { passphrase: '', confirm: '', includePrivate: true, label: '' }
  backupOpen.value = true
  try {
    backupPreview.value = await api.previewBackup(true)
  } catch {
    backupPreview.value = null
  }
}

const backupPassIssue = computed(() => {
  const p = backupForm.value.passphrase
  if (!p) return ''
  if (p.length < 8) return '口令至少 8 位'
  if (p !== backupForm.value.confirm) return '两次输入不一致'
  return ''
})

const canBackup = computed(
  () => !creating.value && (!backupForm.value.passphrase || !backupPassIssue.value)
)

async function doBackup() {
  creating.value = true
  try {
    const res = await api.createBackup({
      passphrase: backupForm.value.passphrase,
      label: backupForm.value.label,
      includePrivate: backupForm.value.includePrivate,
      outDir: '',
    })
    toast.push(`备份已创建（${res.sizeLabel}，${res.entries} 个文件）`, 'success')
    backupOpen.value = false
    await loadBackups()
  } catch (e) {
    toast.push(e.message || '创建备份失败', 'danger')
  } finally {
    creating.value = false
  }
}

async function loadBackups() {
  try {
    backups.value = (await api.listBackups()) || []
  } catch {
    backups.value = []
  }
}

/* 恢复 */
const restoreOpen = ref(false)
const restoreTarget = ref(null)
const restorePass = ref('')
const restoring = ref(false)

function askRestore(b) {
  restoreTarget.value = b
  restorePass.value = ''
  restoreOpen.value = true
}

async function doRestore() {
  if (!restoreTarget.value) return
  restoring.value = true
  try {
    const res = await api.restoreBackup(restoreTarget.value.name, restorePass.value, false)
    toast.push(`已恢复 ${res.entries} 个文件`, 'success')
    restoreOpen.value = false
    await loadAll()
    await store.refreshEnv()
  } catch (e) {
    toast.push(e.message || '恢复失败', 'danger')
  } finally {
    restoring.value = false
  }
}

async function deleteBackup(b) {
  try {
    await api.deleteBackup(b.name)
    toast.push('备份已删除', 'info')
    await loadBackups()
  } catch (e) {
    toast.push(e.message || '删除失败', 'danger')
  }
}

function fmtTime(s) {
  return s ? s.slice(0, 19).replace('T', ' ') : '—'
}

/* ══════════════════ 审计日志 ══════════════════ */
const auditQuery = ref({ category: '', result: '', keyword: '' })
const auditList = ref([])
const auditStat = ref(null)
const auditLoading = ref(false)
/* 审计日志可能很长，走后端分页：只取当前页，同时拿到过滤后的总数 */
const auditPage = ref(1)
const auditPageSize = ref(20)
const auditTotal = ref(0)

async function loadAudit() {
  auditLoading.value = true
  try {
    const res = await api.readAuditPage({
      ...auditQuery.value,
      offset: (auditPage.value - 1) * auditPageSize.value,
      limit: auditPageSize.value,
    })
    const total = res?.total || 0
    const maxPage = Math.max(1, Math.ceil(total / auditPageSize.value))
    // 清空 / 清理后当前页可能越界，收敛到最后一页重新取
    if (auditPage.value > maxPage) {
      auditPage.value = maxPage
      return await loadAudit()
    }
    auditList.value = res?.entries || []
    auditTotal.value = total
  } catch {
    auditList.value = []
    auditTotal.value = 0
  } finally {
    auditLoading.value = false
  }
}

/** 筛选条件变化：回到第一页再查询 */
function reloadAudit() {
  auditPage.value = 1
  loadAudit()
}

function onAuditPage(p) {
  if (p === auditPage.value) return
  auditPage.value = p
  loadAudit()
}

function onAuditPageSize(n) {
  if (n === auditPageSize.value) return
  auditPageSize.value = n
  auditPage.value = 1
  loadAudit()
}

async function loadAuditStats() {
  try {
    auditStat.value = await api.auditStats()
  } catch {
    auditStat.value = null
  }
}

const categoryOptions = [
  { value: '', label: '全部类别' },
  { value: 'key', label: '密钥' },
  { value: 'config', label: '配置' },
  { value: 'perm', label: '权限' },
  { value: 'connect', label: '连通' },
  { value: 'agent', label: 'Agent' },
  { value: 'backup', label: '备份' },
  { value: 'security', label: '安全' },
]

const resultOptions = [
  { value: '', label: '全部结果' },
  { value: 'ok', label: '成功' },
  { value: 'fail', label: '失败' },
  { value: 'warn', label: '警告' },
]

async function exportCsv() {
  try {
    const p = await api.exportAuditCsv('')
    toast.push(`已导出到 ${p}`, 'success', 4000)
  } catch (e) {
    toast.push(e.message || '导出失败', 'danger')
  }
}

const clearAuditOpen = ref(false)

async function doClearAudit() {
  try {
    await api.clearAudit()
    toast.push('审计日志已清空', 'info')
    clearAuditOpen.value = false
    await Promise.all([loadAudit(), loadAuditStats()])
  } catch (e) {
    toast.push(e.message || '清空失败', 'danger')
  }
}

async function pruneAudit() {
  try {
    const n = await api.pruneAudit(store.rawPrefs.auditRetentionDays || 90)
    toast.push(`已清理 ${n} 条过期记录`, 'success')
    await Promise.all([loadAudit(), loadAuditStats()])
  } catch (e) {
    toast.push(e.message || '清理失败', 'danger')
  }
}

/* 类目/结果标签 */
function catLabel(c) {
  return { key: '密钥', config: '配置', perm: '权限', connect: '连通', agent: 'Agent', backup: '备份', security: '安全' }[c] || c
}
function resultLabel(r) {
  return { ok: '成功', fail: '失败', warn: '警告' }[r] || r
}

let auditTimer = null
function scheduleAudit() {
  clearTimeout(auditTimer)
  auditTimer = setTimeout(reloadAudit, 300)
}

onMounted(async () => {
  await Promise.allSettled([loadAll(), loadAudit(), loadAuditStats()])
})
</script>

<template>
  <div class="kh-page">
    <PageHead
      title="安全中心"
      desc="权限、口令、ssh-agent 与备份的整体状态。评分只反映当前可读取到的数据。"
    />

    <PageSkeleton v-if="loading" variant="security" />

    <template v-else>
    <!-- ══════ 顶部：评分 + 体检 ══════ -->
    <div class="kh-sec-hero kh-collapse">
      <!-- 评分卡 -->
      <section class="anx-panel kh-score">
        <div class="anx-panel__body">
          <div class="kh-score__label">安全评分</div>
          <div class="kh-score__num">
            <span class="kh-score__value">{{ score ?? '—' }}</span>
            <span v-if="score !== null" class="kh-score__max">/100</span>
          </div>
          <div class="kh-score__level">
            <span
              class="anx-badge"
              :class="{
                'anx-badge--ok': scoreLevel.tone === 'ok',
                'anx-badge--warn': scoreLevel.tone === 'warn',
                'anx-badge--danger': scoreLevel.tone === 'danger',
                'anx-badge--neutral': scoreLevel.tone === 'idle',
              }"
            >
              {{ scoreLevel.label }}
            </span>
            <span class="anx-t-xs anx-t-muted">{{ scoreLevel.desc }}</span>
          </div>

          <div class="kh-score__bars">
            <div v-for="b in bars" :key="b.label" class="kh-meter">
              <span class="kh-meter__label">{{ b.label }}</span>
              <span class="kh-meter__track">
                <span
                  class="kh-meter__fill"
                  :class="b.value >= 90 ? 'kh-meter__fill--ok' : b.value >= 60 ? 'kh-meter__fill--warn' : 'kh-meter__fill--danger'"
                  :style="{ width: b.value + '%' }"
                />
              </span>
              <span class="kh-meter__value">{{ b.value }}</span>
            </div>
          </div>
        </div>
      </section>

      <!-- 体检清单 -->
      <section class="anx-panel">
        <div class="anx-panel__head">
          <div><h2><Icon name="shield" :size="15" /> 安全体检</h2></div>
          <div class="anx-panel__head-actions">
            <button class="anx-btn anx-btn--ghost anx-btn--sm" :disabled="loading" @click="loadAll">
              <Icon name="refresh" :size="14" /> 重新体检
            </button>
          </div>
        </div>
        <div class="anx-panel__body">
          <div v-if="loading" class="kh-sk-rows">
            <span v-for="i in 5" :key="i" class="anx-sk kh-sk-row" />
          </div>
          <ul v-else class="kh-list">
            <li v-for="c in checks" :key="c.key" class="kh-list__item kh-check-row">
              <span class="kh-check-row__icon" :class="'kh-check-row__icon--' + c.level">
                <Icon :name="c.icon" :size="16" />
              </span>
              <div class="kh-list__body">
                <div class="kh-list__title">
                  {{ c.title }}
                  <span
                    class="anx-badge"
                    :class="
                      c.ok === null
                        ? 'anx-badge--neutral'
                        : c.ok
                          ? 'anx-badge--ok'
                          : c.level === 'error'
                            ? 'anx-badge--danger'
                            : 'anx-badge--warn'
                    "
                  >
                    {{ c.value }}
                  </span>
                </div>
                <p class="kh-list__text">{{ c.detail }}</p>
                <p class="kh-list__text kh-fine">标准：{{ c.expect }}</p>
              </div>
              <button
                v-if="c.action"
                class="anx-btn anx-btn--ghost anx-btn--sm"
                :disabled="busy"
                @click="runCheckAction(c.action)"
              >
                <span v-if="busy && busy === c.action" class="anx-btn__spinner" />
                {{ c.actionLabel }}
              </button>
            </li>
          </ul>
        </div>
      </section>
    </div>

    <!-- ══════ ssh-agent ══════ -->
    <section id="key-section" class="anx-panel">
      <div class="anx-panel__head">
        <div>
          <h2><Icon name="shield" :size="15" /> ssh-agent</h2>
          <p>{{ agent?.label || (loading ? '检测中…' : loadError ? '无法获取 agent 状态' : '未知') }}</p>
        </div>
        <div class="anx-panel__head-actions">
          <label class="anx-switch">
            <input
              type="checkbox"
              :checked="!!agent?.autostart"
              :disabled="agentBusy === 'autostart'"
              @change="toggleAutostart"
            />
            <span class="anx-switch__track" />
            <span class="anx-t-xs anx-t-muted">开机自启</span>
          </label>
          <button class="anx-btn anx-btn--sm" :disabled="agentBusy === 'toggle'" @click="toggleAgent">
            <span v-if="agentBusy === 'toggle'" class="anx-btn__spinner" />
            {{ agent?.running ? '停止' : '启动' }}
          </button>
        </div>
      </div>

      <div class="anx-panel__body kh-agent">
        <div class="kh-agent__status">
          <span
            class="anx-dot"
            :class="agent?.running ? (agent.count ? 'anx-dot--ok' : 'anx-dot--warn') : 'anx-dot--idle'"
          />
          <div class="kh-grow">
            <strong>{{ agent?.label || '未知' }}</strong>
            <div v-if="agent?.sock" class="anx-t-mono anx-t-muted">{{ agent.sock }}</div>
          </div>
        </div>

        <!-- 已加载密钥 -->
        <div v-if="agent?.keys?.length" class="kh-agent__keys">
          <div class="kh-row-between">
            <span class="anx-t-sm anx-t-strong">已加载的密钥</span>
            <button class="anx-btn anx-btn--ghost anx-btn--sm" :disabled="agentBusy === 'clear'" @click="agentClear">
              全部清空
            </button>
          </div>
          <ul class="kh-list">
            <li v-for="(k, i) in agent.keys" :key="i" class="kh-list__item kh-agent__key">
              <Icon name="key" :size="14" />
              <code class="kh-agent__fp">{{ k.fingerprint }}</code>
              <span class="anx-t-xs anx-t-muted kh-grow">{{ k.comment }}</span>
              <span class="anx-tag">{{ k.algorithm || 'SSH' }}</span>
            </li>
          </ul>
        </div>

        <!-- 未加载：给出可加入的密钥 -->
        <div v-else-if="agent?.running" class="kh-agent__empty">
          <p class="anx-t-sm anx-t-muted">agent 正在运行，但还没有加载任何密钥。</p>
          <div v-if="keys.length" class="kh-actions">
            <span class="anx-t-xs anx-t-muted">快速加入：</span>
            <button
              v-for="k in keys.filter((x) => !x.hasPassphrase).slice(0, 4)"
              :key="k.filename"
              class="anx-btn anx-btn--ghost anx-btn--sm"
              :disabled="agentBusy === k.filename"
              @click="agentAddKey(k)"
            >
              <span v-if="agentBusy === k.filename" class="anx-btn__spinner" />
              {{ k.filename }}
            </button>
            <span v-if="!keys.filter((x) => !x.hasPassphrase).length" class="anx-t-xs anx-t-muted">
              所有密钥都带口令，需在系统终端用 <code class="kh-code-inline">ssh-add</code> 手动加载
            </span>
          </div>
        </div>

        <div v-else class="kh-agent__empty">
          <p class="anx-t-sm anx-t-muted">
            agent 未运行。启动后，带口令的密钥在 git push 等场景才能自动认证。
          </p>
          <button class="anx-btn anx-btn--primary anx-btn--sm" :disabled="agentBusy === 'toggle'" @click="toggleAgent">
            启动 ssh-agent
          </button>
        </div>
      </div>
    </section>

    <!-- ══════ 安全策略 ══════ -->
    <section class="anx-panel">
      <div class="anx-panel__head">
        <div>
          <h2><Icon name="sliders" :size="15" /> 安全策略</h2>
          <p>与设置页同步</p>
        </div>
      </div>
      <div class="anx-panel__body">
        <ul class="kh-list">
          <li class="kh-list__item">
            <div class="kh-list__body">
              <div class="kh-list__title">自动修复新密钥权限</div>
              <p class="kh-list__text">生成密钥后立即设置为 600，避免 OpenSSH 拒绝加载</p>
            </div>
            <label class="anx-switch">
              <input
                type="checkbox"
                :checked="!!store.rawPrefs.autoFixPerms"
                :disabled="savingPref === 'autoFixPerms'"
                @change="setPref('autoFixPerms', !store.rawPrefs.autoFixPerms)"
              />
              <span class="anx-switch__track" />
            </label>
          </li>
          <li class="kh-list__item">
            <div class="kh-list__body">
              <div class="kh-list__title">保存 config 前自动备份</div>
              <p class="kh-list__text">每次修改 config 前留一份快照，误改可一键回滚</p>
            </div>
            <label class="anx-switch">
              <input
                type="checkbox"
                :checked="!!store.rawPrefs.autoBackupConfig"
                :disabled="savingPref === 'autoBackupConfig'"
                @change="setPref('autoBackupConfig', !store.rawPrefs.autoBackupConfig)"
              />
              <span class="anx-switch__track" />
            </label>
          </li>
          <li class="kh-list__item">
            <div class="kh-list__body">
              <div class="kh-list__title">记录审计日志</div>
              <p class="kh-list__text">对密钥读取、删除、回滚等敏感操作留痕，便于事后追溯</p>
            </div>
            <label class="anx-switch">
              <input
                type="checkbox"
                :checked="!!store.rawPrefs.auditEnabled"
                :disabled="savingPref === 'auditEnabled'"
                @change="setPref('auditEnabled', !store.rawPrefs.auditEnabled)"
              />
              <span class="anx-switch__track" />
            </label>
          </li>
          <li class="kh-list__item">
            <div class="kh-list__body">
              <div class="kh-list__title">发起连接前二次确认</div>
              <p class="kh-list__text">墨钥默认不联网；开启后每次测试都会先弹窗确认</p>
            </div>
            <label class="anx-switch">
              <input
                type="checkbox"
                :checked="!!store.rawPrefs.confirmBeforeConnect"
                :disabled="savingPref === 'confirmBeforeConnect'"
                @change="setPref('confirmBeforeConnect', !store.rawPrefs.confirmBeforeConnect)"
              />
              <span class="anx-switch__track" />
            </label>
          </li>
          <li class="kh-list__item">
            <div class="kh-list__body">
              <div class="kh-list__title">删除密钥需输入文件名确认</div>
              <p class="kh-list__text">防止误点造成的不可逆数据丢失</p>
            </div>
            <label class="anx-switch">
              <input
                type="checkbox"
                :checked="!!store.rawPrefs.confirmDeleteTyped"
                :disabled="savingPref === 'confirmDeleteTyped'"
                @change="setPref('confirmDeleteTyped', !store.rawPrefs.confirmDeleteTyped)"
              />
              <span class="anx-switch__track" />
            </label>
          </li>
          <li class="kh-list__item">
            <div class="kh-list__body">
              <div class="kh-list__title">
                强制使用系统密码学随机源
                <span class="anx-badge anx-badge--ok"><Icon name="lock" :size="12" /> 已强制</span>
              </div>
              <p class="kh-list__text">密钥生成始终由系统 CSPRNG 提供熵，该选项不可关闭</p>
            </div>
            <label class="anx-switch">
              <input type="checkbox" checked disabled />
              <span class="anx-switch__track" />
            </label>
          </li>
        </ul>
      </div>
    </section>

    <!-- ══════ 加密备份 ══════ -->
    <section class="anx-panel">
      <div class="anx-panel__head">
        <div>
          <h2><Icon name="clock" :size="15" /> 加密备份</h2>
          <p>AES-256-GCM · Argon2id 派生</p>
        </div>
        <div class="anx-panel__head-actions">
          <button class="anx-btn anx-btn--primary anx-btn--sm" @click="openBackup">
            <Icon name="plus" :size="14" /> 创建备份
          </button>
        </div>
      </div>
      <div class="anx-panel__body anx-panel__body--tight">
        <div v-if="!backups.length" class="anx-empty kh-empty-inline">
          <div class="anx-empty__icon"><Icon name="clock" :size="22" /></div>
          <p class="anx-empty__title">还没有备份</p>
          <p class="anx-empty__text">建议至少创建一份加密备份，防止误删或硬盘故障。</p>
          <div class="anx-empty__actions">
            <button class="anx-btn anx-btn--primary anx-btn--sm" @click="openBackup">立即创建</button>
          </div>
        </div>
        <div v-else>
          <div class="anx-table-wrap">
            <table class="anx-table">
              <thead>
                <tr>
                  <th>文件名</th>
                  <th>创建时间</th>
                  <th>大小</th>
                  <th>加密</th>
                  <th class="is-num">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="b in pagedBackups" :key="b.name">
                  <td>
                    <div class="anx-cell-main">
                      <Icon name="file" :size="15" class="anx-t-muted" />
                      <code class="kh-mono">{{ b.name }}</code>
                    </div>
                  </td>
                  <td><code class="kh-mono kh-t-xs">{{ fmtTime(b.createdAt) }}</code></td>
                  <td><code class="kh-mono kh-t-xs">{{ b.sizeLabel }}</code></td>
                  <td>
                    <span class="anx-badge" :class="b.encrypted ? 'anx-badge--ok' : 'anx-badge--danger'">
                      {{ b.encrypted ? '已加密' : '明文' }}
                    </span>
                  </td>
                  <td class="is-num">
                    <div class="anx-cell-actions">
                      <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="askRestore(b)">恢复</button>
                      <button
                        class="anx-iconbtn kh-danger-text"
                        title="删除备份"
                        aria-label="删除备份"
                        @click="deleteBackup(b)"
                      >
                        <Icon name="trash" :size="15" />
                      </button>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <TablePager
            :total="backupTotal"
            :page="backupPage"
            :page-size="backupPageSize"
            @update:page="backupPage = $event"
            @update:page-size="onBackupPageSize"
          />
        </div>
      </div>
    </section>

    <!-- ══════ 审计日志 ══════ -->
    <section class="anx-panel">
      <div class="anx-panel__head">
        <div>
          <h2><Icon name="file-text" :size="15" /> 审计日志</h2>
          <p v-if="auditStat">
            共 {{ auditStat.total }} 条 · 今日 {{ auditStat.today }} 条
            <span v-if="auditStat.failed" class="kh-danger-text">· {{ auditStat.failed }} 条失败</span>
          </p>
        </div>
        <div class="anx-panel__head-actions">
          <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="pruneAudit">清理过期</button>
          <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="exportCsv">
            <Icon name="download" :size="14" /> 导出 CSV
          </button>
          <button class="anx-btn anx-btn--danger-quiet anx-btn--sm" @click="clearAuditOpen = true">清空</button>
        </div>
      </div>

      <div class="anx-panel__body">
        <div class="kh-audit-filters">
          <div class="kh-searchfield kh-grow">
            <Icon name="search" :size="14" />
            <input
              v-model="auditQuery.keyword"
              class="anx-input"
              type="text"
              placeholder="搜索描述或对象…"
              aria-label="搜索审计日志"
              @input="scheduleAudit"
            />
          </div>
          <SelectMenu
            v-model="auditQuery.category"
            :options="categoryOptions"
            aria-label="按类别筛选"
            style="width: 150px"
            @update:model-value="reloadAudit"
          />
          <SelectMenu
            v-model="auditQuery.result"
            :options="resultOptions"
            aria-label="按结果筛选"
            style="width: 140px"
            @update:model-value="reloadAudit"
          />
        </div>

        <div v-if="auditLoading" class="kh-sk-rows">
          <span v-for="i in 4" :key="i" class="anx-sk kh-sk-row" />
        </div>
        <div v-else-if="!auditList.length" class="anx-empty kh-empty-inline">
          <div class="anx-empty__icon"><Icon name="file-text" :size="22" /></div>
          <p class="anx-empty__title">没有符合条件的审计记录</p>
        </div>
        <div v-else class="kh-audit-list">
          <div v-for="e in auditList" :key="e.seq" class="kh-audit-row">
            <span class="kh-audit-row__time">{{ fmtTime(e.at) }}</span>
            <span class="anx-tag">{{ catLabel(e.category) }}</span>
            <span class="kh-audit-row__msg">{{ e.message }}</span>
            <!-- target 可能为空，但必须始终占位：grid 固定 5 列，
                 缺一个子元素会把结果徽章整列左移 -->
            <code class="kh-audit-row__target" :title="e.target || ''">{{ e.target || '—' }}</code>
            <span
              class="anx-badge"
              :class="e.result === 'ok' ? 'anx-badge--ok' : e.result === 'fail' ? 'anx-badge--danger' : 'anx-badge--warn'"
            >
              {{ resultLabel(e.result) }}
            </span>
          </div>
        </div>
        <TablePager
          v-if="auditList.length"
          class="kh-audit-pager"
          :total="auditTotal"
          :page="auditPage"
          :page-size="auditPageSize"
          :sizes="[20, 50, 100]"
          @update:page="onAuditPage"
          @update:page-size="onAuditPageSize"
        />
      </div>
    </section>
    </template>

    <!-- ══════ 创建备份弹窗 ══════ -->
    <AppModal v-model="backupOpen" title="创建加密备份" wide>
      <div class="kh-form">
        <div v-if="backupPreview" class="kh-backup-preview">
          <div class="kh-actions">
            <span><b>{{ backupPreview.count }}</b> 个文件</span>
            <span><b>{{ backupPreview.sizeLabel }}</b> 未压缩</span>
            <span v-if="backupPreview.hasPrivate" class="kh-warn-text">
              <Icon name="alert-triangle" :size="13" /> 含私钥
            </span>
          </div>
          <div class="kh-backup-files">
            <code v-for="(f, i) in backupPreview.files.slice(0, 12)" :key="i">{{ f }}</code>
            <span v-if="backupPreview.files.length > 12" class="kh-fine">
              等 {{ backupPreview.files.length }} 个
            </span>
          </div>
        </div>

        <div class="anx-field kh-field">
          <label class="anx-field__label" for="backup-pass">备份口令 <span class="anx-req">*</span></label>
          <div class="kh-pass">
            <input
              id="backup-pass"
              v-model="backupForm.passphrase"
              :type="showBackupPass ? 'text' : 'password'"
              class="anx-input"
              placeholder="用于加密备份文件，请务必牢记"
              autocomplete="new-password"
            />
            <button
              type="button"
              class="anx-input-wrap__action"
              :aria-label="showBackupPass ? '隐藏口令' : '显示口令'"
              @click="showBackupPass = !showBackupPass"
            >
              <Icon :name="showBackupPass ? 'eye-off' : 'eye'" :size="16" />
            </button>
          </div>
          <p v-if="backupPassIssue" class="anx-field__error">{{ backupPassIssue }}</p>
        </div>

        <FieldInput
          v-model="backupForm.confirm"
          label="确认口令"
          required
          :type="showBackupPass ? 'text' : 'password'"
          autocomplete="new-password"
        />
        <FieldInput v-model="backupForm.label" label="备注（可选）" placeholder="例如：换机前" />

        <AlertBox
          tone="danger"
          title="口令一旦遗失，备份将无法解密"
          text="墨钥不保存备份口令，也无法帮你找回。请记录在密码管理器中。"
        />
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="backupOpen = false">取消</button>
        <button class="anx-btn anx-btn--primary" :disabled="!canBackup" @click="doBackup">
          <span v-if="creating" class="anx-btn__spinner" />
          创建备份
        </button>
      </template>
    </AppModal>

    <!-- ══════ 恢复弹窗 ══════ -->
    <AppModal v-model="restoreOpen" title="恢复备份" :dismissable="false">
      <div v-if="restoreTarget" class="kh-confirm">
        <span class="kh-cf-icon kh-cf-icon--warn"><Icon name="alert-triangle" :size="20" /></span>
        <p class="anx-t-sm">
          将从 <code class="kh-code-inline">{{ restoreTarget.name }}</code> 恢复文件到
          <code class="kh-code-inline">~/.ssh</code>。
        </p>
        <ul class="kh-check-list">
          <li>已存在的同名文件<strong>不会被覆盖</strong>（避免破坏当前可用配置）</li>
          <li>恢复前墨钥会先自动创建一份当前状态的备份</li>
          <li>恢复的密钥会自动重置为 600 权限</li>
        </ul>
        <FieldInput
          v-if="restoreTarget.encrypted"
          v-model="restorePass"
          label="备份口令"
          required
          type="password"
          autocomplete="off"
        />
        <p v-else class="anx-t-xs anx-t-muted">该备份未加密，可直接恢复。</p>
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="restoreOpen = false">取消</button>
        <button
          class="anx-btn anx-btn--primary"
          :disabled="restoring || (restoreTarget?.encrypted && !restorePass)"
          @click="doRestore"
        >
          <span v-if="restoring" class="anx-btn__spinner" />
          确认恢复
        </button>
      </template>
    </AppModal>

    <!-- 清空审计确认 -->
    <AppModal v-model="clearAuditOpen" title="清空审计日志" :dismissable="false">
      <div class="kh-confirm">
        <span class="kh-cf-icon kh-cf-icon--danger"><Icon name="trash" :size="20" /></span>
        <p class="anx-t-sm">将删除全部审计记录，此操作不可撤销。</p>
        <p class="anx-t-xs anx-t-muted">
          审计日志是排查安全事件的唯一线索。建议先导出 CSV 存档，再执行清空。
        </p>
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="clearAuditOpen = false">取消</button>
        <button class="anx-btn anx-btn--danger" @click="doClearAudit">确认清空</button>
      </template>
    </AppModal>
  </div>
</template>
