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
import AppIcon from '../components/AppIcon.vue'
import AppModal from '../components/AppModal.vue'

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
const auditLimit = ref(200)

async function loadAudit() {
  auditLoading.value = true
  try {
    auditList.value =
      (await api.readAuditLog({
        ...auditQuery.value,
        limit: auditLimit.value,
      })) || []
  } catch {
    auditList.value = []
  } finally {
    auditLoading.value = false
  }
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
  auditTimer = setTimeout(loadAudit, 300)
}

onMounted(async () => {
  await Promise.allSettled([loadAll(), loadAudit(), loadAuditStats()])
})
</script>

<template>
  <div class="sec">
    <div class="page-head">
      <div>
        <h1>安全中心</h1>
        <p class="sub">权限、口令、ssh-agent 与备份的整体状态。评分只反映当前可读取到的数据。</p>
      </div>
    </div>

    <!-- ══════ 顶部：评分 + 体检 ══════ -->
    <div class="hero-grid">
      <!-- 评分卡 -->
      <div class="card score-card" :class="scoreLevel.tone">
        <div class="sc-label">安全评分</div>
        <div class="sc-score">
          <span class="num">{{ score ?? '—' }}</span>
          <span v-if="score !== null" class="max">/100</span>
        </div>
        <div class="sc-level">
          <span class="pill" :class="scoreLevel.tone">{{ scoreLevel.label }}</span>
          <span class="desc">{{ scoreLevel.desc }}</span>
        </div>
        <div class="sc-bars">
          <div v-for="b in bars" :key="b.label" class="bar-row">
            <span class="bl">{{ b.label }}</span>
            <div class="bar">
              <span
                class="fill"
                :class="b.value >= 90 ? 'ok' : b.value >= 60 ? 'warn' : 'danger'"
                :style="{ width: b.value + '%' }"
              />
            </div>
            <span class="bv">{{ b.value }}</span>
          </div>
        </div>
      </div>

      <!-- 体检清单 -->
      <div class="card check-card">
        <div class="card-head">
          <span class="ch-title"><AppIcon name="shield" :size="14" /> 安全体检</span>
          <button class="btn ghost xs" :disabled="loading" @click="loadAll">
            <AppIcon name="refresh" :size="13" /> 重新体检
          </button>
        </div>
        <div v-if="loading" class="mini-empty"><span class="spin" /> 正在体检…</div>
        <ul v-else class="check-list">
          <li v-for="c in checks" :key="c.key" class="check-row" :class="c.level">
            <span class="ck-icon"><AppIcon :name="c.icon" :size="15" /></span>
            <div class="ck-body">
              <div class="ck-top">
                <strong>{{ c.title }}</strong>
                <span class="pill" :class="c.ok === null ? 'idle' : c.ok ? 'ok' : c.level === 'error' ? 'danger' : 'warn'">
                  {{ c.value }}
                </span>
              </div>
              <p class="ck-detail">{{ c.detail }}</p>
              <p class="ck-expect">标准：{{ c.expect }}</p>
            </div>
            <button
              v-if="c.action"
              class="btn xs"
              :disabled="busy"
              @click="runCheckAction(c.action)"
            >
              <span v-if="busy && busy === c.action" class="spin" />
              {{ c.actionLabel }}
            </button>
          </li>
        </ul>
      </div>
    </div>

    <!-- ══════ ssh-agent ══════ -->
    <div class="card agent-card">
      <div class="card-head">
        <span class="ch-title"><AppIcon name="shield" :size="14" /> ssh-agent</span>
        <div class="head-right">
          <label class="switch-row">
            <span>开机自启</span>
            <button
              class="switch"
              :class="{ on: agent?.autostart }"
              :disabled="agentBusy === 'autostart'"
              @click="toggleAutostart"
            >
              <span class="knob" />
            </button>
          </label>
          <button class="btn sm" :disabled="agentBusy === 'toggle'" @click="toggleAgent">
            <span v-if="agentBusy === 'toggle'" class="spin" />
            {{ agent?.running ? '停止' : '启动' }}
          </button>
        </div>
      </div>

      <div class="agent-body">
        <div class="agent-status">
          <span class="as-dot" :class="agent?.running ? (agent.count ? 'ok' : 'warn') : 'idle'" />
          <div class="as-txt">
            <strong>{{ agent?.label || (loading ? '检测中…' : loadError ? '无法获取 agent 状态' : '未知') }}</strong>
            <span v-if="agent?.sock" class="as-sock">{{ agent.sock }}</span>
          </div>
        </div>

        <!-- 已加载密钥 -->
        <div v-if="agent?.keys?.length" class="agent-keys">
          <div class="ak-head">
            <span>已加载的密钥</span>
            <button class="btn ghost xs" :disabled="agentBusy === 'clear'" @click="agentClear">
              全部清空
            </button>
          </div>
          <ul>
            <li v-for="(k, i) in agent.keys" :key="i">
              <AppIcon name="key" :size="13" />
              <code>{{ k.fingerprint }}</code>
              <span class="ak-cmt">{{ k.comment }}</span>
              <span class="tag sm">{{ k.algorithm || 'SSH' }}</span>
            </li>
          </ul>
        </div>

        <!-- 未加载：给出可加入的密钥 -->
        <div v-else-if="agent?.running" class="agent-empty">
          <p>agent 正在运行，但还没有加载任何密钥。</p>
          <div v-if="keys.length" class="agent-add">
            <span>快速加入：</span>
            <button
              v-for="k in keys.filter((x) => !x.hasPassphrase).slice(0, 4)"
              :key="k.filename"
              class="btn xs"
              :disabled="agentBusy === k.filename"
              @click="agentAddKey(k)"
            >
              <span v-if="agentBusy === k.filename" class="spin" />
              {{ k.filename }}
            </button>
            <span v-if="!keys.filter((x) => !x.hasPassphrase).length" class="all-pass">
              所有密钥都带口令，需在系统终端用 <code>ssh-add</code> 手动加载
            </span>
          </div>
        </div>

        <div v-else class="agent-empty">
          <p>agent 未运行。启动后，带口令的密钥在 git push 等场景才能自动认证。</p>
          <button class="btn primary sm" :disabled="agentBusy === 'toggle'" @click="toggleAgent">
            启动 ssh-agent
          </button>
        </div>
      </div>
    </div>

    <!-- ══════ 安全策略 ══════ -->
    <div class="card policy-card">
      <div class="card-head">
        <span class="ch-title"><AppIcon name="settings" :size="14" /> 安全策略</span>
        <span class="meta">与设置页同步</span>
      </div>
      <ul class="policy-list">
        <li>
          <div class="pl-txt">
            <strong>自动修复新密钥权限</strong>
            <p>生成密钥后立即设置为 600，避免 OpenSSH 拒绝加载</p>
          </div>
          <button
            class="switch"
            :class="{ on: store.rawPrefs.autoFixPerms }"
            :disabled="savingPref === 'autoFixPerms'"
            @click="setPref('autoFixPerms', !store.rawPrefs.autoFixPerms)"
          >
            <span class="knob" />
          </button>
        </li>
        <li>
          <div class="pl-txt">
            <strong>保存 config 前自动备份</strong>
            <p>每次修改 config 前留一份快照，误改可一键回滚</p>
          </div>
          <button
            class="switch"
            :class="{ on: store.rawPrefs.autoBackupConfig }"
            :disabled="savingPref === 'autoBackupConfig'"
            @click="setPref('autoBackupConfig', !store.rawPrefs.autoBackupConfig)"
          >
            <span class="knob" />
          </button>
        </li>
        <li>
          <div class="pl-txt">
            <strong>记录审计日志</strong>
            <p>对密钥读取、删除、回滚等敏感操作留痕，便于事后追溯</p>
          </div>
          <button
            class="switch"
            :class="{ on: store.rawPrefs.auditEnabled }"
            :disabled="savingPref === 'auditEnabled'"
            @click="setPref('auditEnabled', !store.rawPrefs.auditEnabled)"
          >
            <span class="knob" />
          </button>
        </li>
        <li>
          <div class="pl-txt">
            <strong>发起连接前二次确认</strong>
            <p>墨钥默认不联网；开启后每次测试都会先弹窗确认</p>
          </div>
          <button
            class="switch"
            :class="{ on: store.rawPrefs.confirmBeforeConnect }"
            :disabled="savingPref === 'confirmBeforeConnect'"
            @click="setPref('confirmBeforeConnect', !store.rawPrefs.confirmBeforeConnect)"
          >
            <span class="knob" />
          </button>
        </li>
        <li>
          <div class="pl-txt">
            <strong>删除密钥需输入文件名确认</strong>
            <p>防止误点造成的不可逆数据丢失</p>
          </div>
          <button
            class="switch"
            :class="{ on: store.rawPrefs.confirmDeleteTyped }"
            :disabled="savingPref === 'confirmDeleteTyped'"
            @click="setPref('confirmDeleteTyped', !store.rawPrefs.confirmDeleteTyped)"
          >
            <span class="knob" />
          </button>
        </li>
        <li class="locked">
          <div class="pl-txt">
            <strong>
              强制使用系统密码学随机源
              <AppIcon name="lock" :size="12" />
            </strong>
            <p>密钥生成始终由系统 CSPRNG 提供熵，该选项不可关闭</p>
          </div>
          <button class="switch on" disabled>
            <span class="knob" />
          </button>
        </li>
      </ul>
    </div>

    <!-- ══════ 加密备份 ══════ -->
    <div class="card backup-card">
      <div class="card-head">
        <span class="ch-title"><AppIcon name="clock" :size="14" /> 加密备份</span>
        <div class="head-right">
          <span class="bk-hint">AES-256-GCM · Argon2id 派生</span>
          <button class="btn primary sm" @click="openBackup">
            <AppIcon name="plus" :size="13" /> 创建备份
          </button>
        </div>
      </div>

      <div v-if="!backups.length" class="mini-empty big">
        <AppIcon name="clock" :size="22" />
        <span>还没有备份。建议至少创建一份加密备份，防止误删或硬盘故障。</span>
        <button class="btn primary sm" @click="openBackup">立即创建</button>
      </div>

      <table v-else class="ds-table">
        <thead>
          <tr>
            <th>文件名</th>
            <th>创建时间</th>
            <th>大小</th>
            <th>加密</th>
            <th class="ta-r">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="b in backups" :key="b.name">
            <td>
              <div class="bk-name">
                <AppIcon name="file" :size="13" />
                <code>{{ b.name }}</code>
              </div>
            </td>
            <td><code class="mono-sm">{{ fmtTime(b.createdAt) }}</code></td>
            <td><code class="mono-sm">{{ b.sizeLabel }}</code></td>
            <td>
              <span class="pill" :class="b.encrypted ? 'ok' : 'danger'">
                {{ b.encrypted ? '已加密' : '明文' }}
              </span>
            </td>
            <td class="ta-r">
              <div class="row-actions">
                <button class="btn xs" @click="askRestore(b)">恢复</button>
                <button class="btn xs danger-text" @click="deleteBackup(b)">
                  <AppIcon name="trash" :size="12" />
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- ══════ 审计日志 ══════ -->
    <div class="card audit-card">
      <div class="card-head">
        <span class="ch-title"><AppIcon name="file" :size="14" /> 审计日志</span>
        <div class="head-right">
          <span v-if="auditStat" class="audit-stat">
            共 {{ auditStat.total }} 条 · 今日 {{ auditStat.today }} 条
            <template v-if="auditStat.failed">
              · <span class="fail-count">{{ auditStat.failed }} 条失败</span>
            </template>
          </span>
          <button class="btn ghost xs" @click="pruneAudit">清理过期</button>
          <button class="btn ghost xs" @click="exportCsv">
            <AppIcon name="download" :size="13" /> 导出 CSV
          </button>
          <button class="btn ghost xs danger-text" @click="clearAuditOpen = true">清空</button>
        </div>
      </div>

      <!-- 筛选 -->
      <div class="audit-filters">
        <div class="search sm">
          <AppIcon name="search" :size="14" />
          <input
            v-model="auditQuery.keyword"
            type="text"
            placeholder="搜索描述或对象…"
            @input="scheduleAudit"
          />
        </div>
        <select v-model="auditQuery.category" class="select sm" @change="loadAudit">
          <option v-for="c in categoryOptions" :key="c.value" :value="c.value">{{ c.label }}</option>
        </select>
        <select v-model="auditQuery.result" class="select sm" @change="loadAudit">
          <option value="">全部结果</option>
          <option value="ok">成功</option>
          <option value="fail">失败</option>
          <option value="warn">警告</option>
        </select>
      </div>

      <div v-if="auditLoading" class="mini-empty"><span class="spin" /> 读取中…</div>
      <div v-else-if="!auditList.length" class="mini-empty big">
        <AppIcon name="file" :size="20" />
        <span>没有符合条件的审计记录</span>
      </div>
      <div v-else class="audit-list">
        <div v-for="e in auditList" :key="e.seq" class="audit-row" :class="e.result">
          <span class="ar-time">{{ fmtTime(e.at) }}</span>
          <span class="tag sm cat">{{ catLabel(e.category) }}</span>
          <span class="ar-msg">{{ e.message }}</span>
          <code v-if="e.target" class="ar-target">{{ e.target }}</code>
          <span class="pill sm" :class="e.result === 'ok' ? 'ok' : e.result === 'fail' ? 'danger' : 'warn'">
            {{ resultLabel(e.result) }}
          </span>
        </div>
      </div>
    </div>

    <!-- ══════ 创建备份弹窗 ══════ -->
    <AppModal v-model="backupOpen" title="创建加密备份" wide>
      <div class="form">
        <div v-if="backupPreview" class="bk-preview">
          <div class="bp-stat">
            <span><b>{{ backupPreview.count }}</b> 个文件</span>
            <span><b>{{ backupPreview.sizeLabel }}</b> 未压缩</span>
            <span v-if="backupPreview.hasPrivate" class="bp-warn">
              <AppIcon name="warning" :size="12" /> 含私钥
            </span>
          </div>
          <div class="bp-files">
            <code v-for="(f, i) in backupPreview.files.slice(0, 12)" :key="i">{{ f }}</code>
            <span v-if="backupPreview.files.length > 12" class="bp-more">
              等 {{ backupPreview.files.length }} 个
            </span>
          </div>
        </div>

        <div class="field">
          <label>备份口令 <span class="req">*</span></label>
          <div class="pass-wrap">
            <input
              v-model="backupForm.passphrase"
              :type="showBackupPass ? 'text' : 'password'"
              class="input"
              placeholder="用于加密备份文件，请务必牢记"
              autocomplete="new-password"
            />
            <button class="eye" @click="showBackupPass = !showBackupPass">
              <AppIcon :name="showBackupPass ? 'eyeOff' : 'eye'" :size="14" />
            </button>
          </div>
          <p v-if="backupPassIssue" class="hint-row danger-text">{{ backupPassIssue }}</p>
        </div>

        <div class="field">
          <label>确认口令 <span class="req">*</span></label>
          <input
            v-model="backupForm.confirm"
            :type="showBackupPass ? 'text' : 'password'"
            class="input"
            autocomplete="new-password"
          />
        </div>

        <div class="field">
          <label>备注（可选）</label>
          <input v-model="backupForm.label" type="text" class="input" placeholder="例如：换机前" />
        </div>

        <div class="banner danger sm">
          <AppIcon name="warning" :size="14" />
          <div>
            <strong>口令一旦遗失，备份将无法解密</strong>
            <p>墨钥不保存备份口令，也无法帮你找回。请记录在密码管理器中。</p>
          </div>
        </div>
      </div>
      <template #footer>
        <button class="btn ghost" @click="backupOpen = false">取消</button>
        <button class="btn primary" :disabled="!canBackup" @click="doBackup">
          <span v-if="creating" class="spin" />
          创建备份
        </button>
      </template>
    </AppModal>

    <!-- ══════ 恢复弹窗 ══════ -->
    <AppModal v-model="restoreOpen" title="恢复备份" :dismissable="false">
      <div v-if="restoreTarget" class="confirm">
        <div class="cf-icon warn"><AppIcon name="warning" :size="20" /></div>
        <p class="cf-lead">
          将从 <code>{{ restoreTarget.name }}</code> 恢复文件到 <code>~/.ssh</code>。
        </p>
        <ul class="cf-list">
          <li>已存在的同名文件<strong>不会被覆盖</strong>（避免破坏当前可用配置）</li>
          <li>恢复前墨钥会先自动创建一份当前状态的备份</li>
          <li>恢复的密钥会自动重置为 600 权限</li>
        </ul>
        <div v-if="restoreTarget.encrypted" class="field">
          <label>备份口令 <span class="req">*</span></label>
          <input v-model="restorePass" type="password" class="input" autocomplete="off" />
        </div>
        <p v-else class="cf-note">该备份未加密，可直接恢复。</p>
      </div>
      <template #footer>
        <button class="btn ghost" @click="restoreOpen = false">取消</button>
        <button
          class="btn primary"
          :disabled="restoring || (restoreTarget?.encrypted && !restorePass)"
          @click="doRestore"
        >
          <span v-if="restoring" class="spin" />
          确认恢复
        </button>
      </template>
    </AppModal>

    <!-- 清空审计确认 -->
    <AppModal v-model="clearAuditOpen" title="清空审计日志" :dismissable="false">
      <div class="confirm">
        <div class="cf-icon danger"><AppIcon name="trash" :size="20" /></div>
        <p class="cf-lead">将删除全部审计记录，此操作不可撤销。</p>
        <p class="cf-note">
          审计日志是排查安全事件的唯一线索。建议先导出 CSV 存档，再执行清空。
        </p>
      </div>
      <template #footer>
        <button class="btn ghost" @click="clearAuditOpen = false">取消</button>
        <button class="btn danger" @click="doClearAudit">确认清空</button>
      </template>
    </AppModal>
  </div>
</template>

<style scoped>
.sec {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* 顶部两列 */
.hero-grid {
  display: grid;
  grid-template-columns: 320px minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}
@media (max-width: 1080px) {
  .hero-grid {
    grid-template-columns: 1fr;
  }
}

/* 评分卡 */
.score-card {
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.sc-label {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.sc-score {
  display: flex;
  align-items: baseline;
  gap: 4px;
  margin: 6px 0 2px;
}
.sc-score .num {
  font-family: var(--font-mono);
  font-size: 42px;
  font-weight: 700;
  line-height: 1;
  color: var(--fg);
  letter-spacing: -0.02em;
}
.score-card.ok .sc-score .num {
  color: var(--success-ink);
}
.score-card.warn .sc-score .num {
  color: var(--warn);
}
.score-card.danger .sc-score .num {
  color: var(--danger);
}
.sc-score .max {
  font-family: var(--font-mono);
  font-size: 15px;
  color: var(--fg-faint);
}
.sc-level {
  display: flex;
  align-items: center;
  gap: 9px;
  margin-bottom: 16px;
}
.sc-level .desc {
  font-size: 11.5px;
  color: var(--fg-muted);
}
.sc-bars {
  display: flex;
  flex-direction: column;
  gap: 9px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
}
.bar-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.bl {
  font-size: 11.5px;
  color: var(--fg-muted);
  width: 44px;
  flex: 0 0 auto;
}
.bar {
  flex: 1;
  height: 5px;
  background: var(--surface-warm);
  border-radius: 3px;
  overflow: hidden;
}
.fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  transition: width 0.4s ease;
}
.fill.ok {
  background: var(--success);
}
.fill.warn {
  background: var(--warn);
}
.fill.danger {
  background: var(--danger);
}
.bv {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg-faint);
  width: 26px;
  text-align: right;
  flex: 0 0 auto;
}

/* 体检 */
.check-card {
  padding: 0;
  overflow: hidden;
}
.card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 13px 16px;
  border-bottom: 1px solid var(--border);
  gap: 12px;
  flex-wrap: wrap;
}
.ch-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--fg);
  flex: 0 0 auto;
}
.head-right {
  display: flex;
  align-items: center;
  gap: 9px;
  flex-wrap: wrap;
}
.check-list {
  list-style: none;
  padding: 12px;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.check-row {
  display: flex;
  align-items: flex-start;
  gap: 11px;
  padding: 11px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--surface-warm);
}
.check-row.error {
  border-color: color-mix(in srgb, var(--danger) 28%, var(--border));
  background: color-mix(in srgb, var(--danger) 3%, var(--surface));
}
.check-row.warn {
  border-color: color-mix(in srgb, var(--warn) 28%, var(--border));
  background: color-mix(in srgb, var(--warn) 3%, var(--surface));
}
.check-row.ok {
  border-color: color-mix(in srgb, var(--success) 22%, var(--border));
}
.check-row.unknown {
  border-color: var(--border-soft);
  background: var(--surface);
}
.ck-icon {
  width: 30px;
  height: 30px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--surface);
  color: var(--fg-muted);
  flex: 0 0 auto;
}
.check-row.ok .ck-icon {
  background: var(--success-soft);
  color: var(--success-ink);
}
.check-row.error .ck-icon {
  background: var(--danger-soft);
  color: var(--danger);
}
.check-row.warn .ck-icon {
  background: var(--warn-soft);
  color: var(--warn);
}
.ck-body {
  flex: 1;
  min-width: 0;
}
.ck-top {
  display: flex;
  align-items: center;
  gap: 9px;
  margin-bottom: 3px;
  flex-wrap: wrap;
}
.ck-top strong {
  font-size: 12.5px;
  color: var(--fg);
}
.ck-detail {
  font-size: 11.5px;
  color: var(--fg-muted);
  margin: 0 0 2px;
  line-height: 1.6;
}
.ck-expect {
  font-size: 10.5px;
  color: var(--fg-faint);
  margin: 0;
  font-family: var(--font-mono);
}
.check-row > .btn {
  flex: 0 0 auto;
  margin-top: 2px;
}

/* agent */
.agent-card {
  padding: 0;
  overflow: hidden;
}
.switch-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--fg-muted);
}
.agent-body {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.agent-status {
  display: flex;
  align-items: center;
  gap: 11px;
}
.as-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.as-dot.ok {
  background: var(--success);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--success) 18%, transparent);
}
.as-dot.warn {
  background: var(--warn);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--warn) 18%, transparent);
}
.as-dot.idle {
  background: var(--fg-faint);
}
.as-txt strong {
  display: block;
  font-size: 13px;
  color: var(--fg);
  margin-bottom: 2px;
}
.as-sock {
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: var(--fg-faint);
  word-break: break-all;
}

.agent-keys,
.agent-empty {
  padding: 12px 13px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
}
.ak-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 9px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.agent-keys ul {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.agent-keys li {
  display: flex;
  align-items: center;
  gap: 9px;
  font-size: 12px;
  color: var(--fg-muted);
}
.agent-keys li :deep(svg) {
  color: var(--success);
  flex: 0 0 auto;
}
.agent-keys code {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg);
}
.ak-cmt {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  color: var(--fg-faint);
}
.agent-empty p {
  font-size: 12.5px;
  color: var(--fg-muted);
  margin: 0 0 10px;
  line-height: 1.6;
}
.agent-add {
  display: flex;
  align-items: center;
  gap: 7px;
  flex-wrap: wrap;
  font-size: 12px;
  color: var(--fg-faint);
}
.all-pass {
  font-size: 11.5px;
  color: var(--fg-faint);
}
.all-pass code {
  font-family: var(--font-mono);
  background: var(--surface);
  padding: 1px 5px;
  border-radius: 5px;
  color: var(--accent);
}

/* 策略 */
.policy-card {
  padding: 0;
  overflow: hidden;
}
.policy-list {
  list-style: none;
  padding: 4px 0;
  margin: 0;
}
.policy-list li {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 13px 16px;
  border-bottom: 1px solid var(--border);
}
.policy-list li:last-child {
  border-bottom: none;
}
.pl-txt {
  flex: 1;
  min-width: 0;
}
.pl-txt strong {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  color: var(--fg);
  margin-bottom: 3px;
}
.pl-txt p {
  font-size: 11.5px;
  color: var(--fg-muted);
  margin: 0;
  line-height: 1.6;
}
.policy-list li.locked {
  background: var(--surface-warm);
}
.policy-list li.locked :deep(svg) {
  color: var(--success);
}

/* 备份卡 */
.backup-card {
  padding: 0;
  overflow: hidden;
}
.bk-hint {
  font-size: 10.5px;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.bk-name {
  display: flex;
  align-items: center;
  gap: 8px;
}
.bk-name code {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg);
}
.mono-sm {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg-muted);
}
.row-actions {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}
.ta-r {
  text-align: right;
}

/* 审计卡 */
.audit-card {
  padding: 0;
  overflow: hidden;
}
.audit-stat {
  font-size: 11.5px;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.fail-count {
  color: var(--danger);
}
.audit-filters {
  display: flex;
  gap: 9px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
  flex-wrap: wrap;
}
.audit-filters .search {
  flex: 1;
  min-width: 200px;
  height: 34px;
  padding: 0 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--fg-faint);
}
.audit-filters .search input {
  flex: 1;
  border: none;
  background: transparent;
  outline: none;
  font-size: 12.5px;
  color: var(--fg);
  font-family: inherit;
}
.select.sm {
  height: 34px;
  width: auto;
  min-width: 110px;
}
.audit-list {
  max-height: 420px;
  overflow-y: auto;
}
.audit-row {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 9px 16px;
  border-bottom: 1px solid var(--border);
  font-size: 12px;
}
.audit-row:last-child {
  border-bottom: none;
}
.audit-row:hover {
  background: var(--surface-warm);
}
.audit-row.fail {
  background: color-mix(in srgb, var(--danger) 3%, transparent);
}
.ar-time {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg-faint);
  width: 138px;
  flex: 0 0 auto;
}
.tag.sm {
  font-size: 10px;
  padding: 1px 6px;
  flex: 0 0 auto;
}
.ar-msg {
  flex: 1;
  min-width: 0;
  color: var(--fg-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ar-target {
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: var(--fg-faint);
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 0 0 auto;
}
.pill.sm {
  font-size: 10px;
  padding: 2px 7px;
  flex: 0 0 auto;
}

/* 空态 */
.mini-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  padding: 28px 16px;
  font-size: 12.5px;
  color: var(--fg-faint);
  text-align: center;
}
.mini-empty.big {
  flex-direction: column;
  padding: 42px 20px;
}

/* 表单 */
.form {
  display: flex;
  flex-direction: column;
  gap: 17px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field label {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--fg);
}
.req {
  color: var(--danger);
}
.hint-row {
  font-size: 11.5px;
  color: var(--fg-faint);
  margin: 0;
  line-height: 1.6;
}
.danger-text {
  color: var(--danger);
}
.pass-wrap {
  position: relative;
}
.pass-wrap .input {
  padding-right: 40px;
}
.eye {
  position: absolute;
  right: 10px;
  top: 50%;
  transform: translateY(-50%);
  border: none;
  background: transparent;
  color: var(--fg-faint);
  cursor: pointer;
  display: grid;
  place-items: center;
}

.bk-preview {
  padding: 13px 14px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
}
.bp-stat {
  display: flex;
  gap: 18px;
  font-size: 12px;
  color: var(--fg-muted);
  margin-bottom: 10px;
  flex-wrap: wrap;
}
.bp-stat b {
  color: var(--fg);
  font-family: var(--font-mono);
}
.bp-warn {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--warn);
}
.bp-files {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.bp-files code {
  font-family: var(--font-mono);
  font-size: 10.5px;
  padding: 2px 7px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--fg-muted);
}
.bp-more {
  font-size: 10.5px;
  color: var(--fg-faint);
  align-self: center;
}

/* 确认弹窗 */
.confirm {
  display: flex;
  flex-direction: column;
  gap: 13px;
}
.cf-icon {
  width: 46px;
  height: 46px;
  border-radius: 15px;
  display: grid;
  place-items: center;
  align-self: flex-start;
}
.cf-icon.warn {
  background: var(--warn-soft);
  color: var(--warn);
}
.cf-icon.danger {
  background: var(--danger-soft);
  color: var(--danger);
}
.cf-lead {
  font-size: 13.5px;
  color: var(--fg);
  margin: 0;
  line-height: 1.7;
}
.cf-lead code {
  font-family: var(--font-mono);
  font-size: 12px;
  background: var(--surface-warm);
  padding: 1px 5px;
  border-radius: 5px;
  color: var(--accent);
}
.cf-list {
  margin: 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.cf-list li {
  font-size: 12.5px;
  color: var(--fg-muted);
  line-height: 1.6;
}
.cf-note {
  font-size: 12px;
  color: var(--fg-muted);
  line-height: 1.7;
  margin: 0;
  padding: 10px 12px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
}
</style>
