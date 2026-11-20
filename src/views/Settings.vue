<script setup>
/**
 * 设置 · 偏好中心
 *
 * 设计意图：所有开关都直连后端 prefs 文件（~/.keyhub/prefs.json），
 * 不做「先改 UI 再点保存」的两段式——改完即落盘，避免用户以为改了其实没改。
 * 唯一例外是文本类输入（默认注释、超时秒数），用失焦提交，避免每敲一个字写一次盘。
 *
 * 安全红线：strictCsprng 为强制项，UI 上锁定不可关闭，并注明原因。
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import AppIcon from '../components/AppIcon.vue'
import AppModal from '../components/AppModal.vue'
import { useEnvStore } from '../lib/store.js'
import * as api from '../lib/api.js'
import { useToast } from '../lib/toast.js'
import { useTheme } from '../lib/theme.js'

const env = useEnvStore()
/** 环境摘要的只读快照，模板里少写一层链 */
const sys = env.env
const toast = useToast()
const { theme, toggleTheme } = useTheme()

const saving = ref(false)
const paths = ref(null)
const backups = ref([])

/** 本地草稿：文本输入的缓冲，失焦时提交 */
const draft = reactive({
  defaultComment: '',
  connectTimeoutSecs: 10,
  auditRetentionDays: 90,
})

/** 危险操作确认弹窗 */
const confirmDialog = ref(null) // { title, body, confirmText, danger, run }
/** 供 AppModal 的 v-model 使用：有内容即显示 */
const confirmVisible = computed({
  get: () => !!confirmDialog.value,
  set: (v) => {
    if (!v) confirmDialog.value = null
  },
})

const p = computed(() => env.prefs)

/* ═══ 写入管线 ══════════════════════════════════════════════════
 * 所有修改都经过 patch()，保证：失败回滚 + 提示，不出现"界面改了盘没改"。
 * ═══════════════════════════════════════════════════════════════ */
async function patch(changes) {
  const before = {}
  for (const k of Object.keys(changes)) before[k] = env.rawPrefs[k]
  Object.assign(env.rawPrefs, changes)
  saving.value = true
  try {
    await env.savePrefs(changes)
  } catch (e) {
    Object.assign(env.rawPrefs, before)
    toast(`保存失败：${e.message || e}`, 'danger')
  } finally {
    saving.value = false
  }
}

function onToggle(key) {
  return (e) => patch({ [key]: e.target.checked })
}

function onSelect(key) {
  return (e) => patch({ [key]: e.target.value })
}

function onNumber(key, min, max) {
  return () => {
    let v = Number(draft[key])
    if (!Number.isFinite(v)) v = p.value[key]
    v = Math.min(max, Math.max(min, Math.round(v)))
    draft[key] = v
    if (v !== p.value[key]) patch({ [key]: v })
  }
}

/* ── 主题 ───────────────────────────────────────────────────── */
function setTheme(next) {
  if (theme.value !== next) toggleTheme()
  patch({ theme: next })
}

/* ── 算法默认值 ─────────────────────────────────────────────── */
const ALGOS = [
  { id: 'ed25519', name: 'ED25519', desc: '现代默认 · 短小快 · 推荐' },
  { id: 'rsa', name: 'RSA', desc: '兼容老系统 · 位数可调' },
]

/* ── 引导重跑 / 重置偏好 ────────────────────────────────────── */
function rerunOnboarding() {
  confirmDialog.value = {
    title: '重新运行环境自检向导',
    body: '将重新走一遍环境检测与密钥导入流程。已有密钥不会被删除，仅重新扫描。',
    confirmText: '开始自检',
    danger: false,
    async run() {
      env.rawPrefs.onboardedAt = null
      await env.savePrefs({ onboardedAt: null })
      location.hash = '#/onboarding'
    },
  }
}

function resetPrefs() {
  confirmDialog.value = {
    title: '恢复默认设置',
    body: '所有偏好将重置为出厂默认值。密钥、SSH Config、备份文件与审计日志不受影响。',
    confirmText: '恢复默认',
    danger: true,
    async run() {
      try {
        const fresh = await api.resetPrefs()
        Object.assign(env.rawPrefs, fresh)
        syncDraft()
        toast('已恢复默认设置')
        if (fresh.theme && fresh.theme !== theme.value) {
          document.documentElement.setAttribute('data-theme', fresh.theme)
        }
      } catch (e) {
        toast(`重置失败：${e.message || e}`, 'danger')
      }
    },
  }
}

function syncDraft() {
  draft.defaultComment = p.value.defaultComment || ''
  draft.connectTimeoutSecs = p.value.connectTimeoutSecs
  draft.auditRetentionDays = p.value.auditRetentionDays
}

/* ── 数据目录 / 维护 ─────────────────────────────────────────── */
/** 后端 get_paths 字段：sshDir/appData/config/backups/configBackups/audit/prefs/home */
const dataDir = computed(() => paths.value?.appData || '')
const auditFile = computed(() => paths.value?.audit || '')

async function loadPaths() {
  try {
    const [p, cb] = await Promise.all([api.getPaths(), api.configBackupDirPath()])
    paths.value = { ...p, configBackups: cb }
  } catch {
    paths.value = null
  }
}

async function loadBackups() {
  try {
    backups.value = await api.listConfigBackups()
  } catch {
    backups.value = []
  }
}

async function copyPath(text) {
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    toast('路径已复制')
  } catch {
    toast('复制失败：剪贴板不可用', 'danger')
  }
}

async function pruneNow() {
  const days = draft.auditRetentionDays
  confirmDialog.value = {
    title: `清理 ${days} 天前的审计日志`,
    body: `将永久删除 ${days} 天之前的审计记录，此操作不可撤销。建议先导出 CSV 备份。`,
    confirmText: '确认清理',
    danger: true,
    async run() {
      try {
        const n = await api.pruneAudit(days)
        toast(n ? `已清理 ${n} 条记录` : '没有需要清理的记录')
      } catch (e) {
        toast(`清理失败：${e.message || e}`, 'danger')
      }
    },
  }
}

async function exportCsv() {
  try {
    const out = await api.exportAuditCsv('')
    toast(`已导出：${out}`)
  } catch (e) {
    toast(`导出失败：${e.message || e}`, 'danger')
  }
}

async function runConfirm() {
  const d = confirmDialog.value
  confirmDialog.value = null
  if (d?.run) await d.run()
}

onMounted(async () => {
  if (!env.prefs.loaded) await env.loadPrefs()
  syncDraft()
  loadPaths()
  loadBackups()
})

watch(() => env.prefs.loaded, syncDraft)

const platformLabel = computed(() => {
  if (sys.error) return '未知'
  const m = { windows: 'Windows', macos: 'macOS', linux: 'Linux' }
  return m[sys.platform] || sys.platform || '—'
})
</script>

<template>
  <!-- 必须是单根元素：App.vue 用 <transition mode="out-in"> 包路由组件，
       多根节点会让离场永远无法结束，切走设置页后主区永久空白。 -->
  <div class="settings-page">
  <div class="page-head">
    <div>
      <h1>设置</h1>
      <p class="sub">
        应用偏好与本地数据位置。所有配置只保存在本机，程序不上传任何信息。
        <span class="meta" v-if="saving">· 保存中…</span>
      </p>
    </div>
    <div class="row">
      <button class="btn btn-secondary btn-sm" @click="rerunOnboarding">
        <AppIcon name="refresh" :width="1.8" />
        重新自检
      </button>
      <button class="btn btn-danger-line btn-sm" @click="resetPrefs">
        <AppIcon name="rotate" :width="1.8" />
        恢复默认
      </button>
    </div>
  </div>

  <div class="layout">
    <!-- ═══ 左栏：设置分组 ══════════════════════════════════════ -->
    <div class="col" style="gap: var(--space-5)">
      <!-- 外观 -->
      <section class="card-flat">
        <div class="card-head">
          <h2>外观</h2>
          <span class="meta">即时生效</span>
        </div>
        <div class="card-body col" style="gap: 16px">
          <div class="set-row">
            <div class="set-main">
              <div class="set-title">主题</div>
              <div class="set-desc">亮色 / 暗色，会同步到所有页面与图表配色</div>
            </div>
            <div class="seg">
              <button
                class="seg-item"
                :class="{ on: theme === 'light' }"
                @click="setTheme('light')"
              >
                <AppIcon name="sun" :size="14" :width="1.8" />
                亮色
              </button>
              <button
                class="seg-item"
                :class="{ on: theme === 'dark' }"
                @click="setTheme('dark')"
              >
                <AppIcon name="moon" :size="14" :width="1.8" />
                暗色
              </button>
            </div>
          </div>

          <div class="set-row">
            <div class="set-main">
              <div class="set-title">界面密度</div>
              <div class="set-desc">紧凑模式会压缩表格行高，适合小屏或密钥较多时</div>
            </div>
            <select
              class="select"
              style="width: 148px"
              :value="p.density"
              @change="onSelect('density')"
            >
              <option value="comfortable">舒适（默认）</option>
              <option value="compact">紧凑</option>
            </select>
          </div>
        </div>
      </section>

      <!-- 密钥生成默认值 -->
      <section class="card-flat">
        <div class="card-head">
          <h2>密钥生成默认值</h2>
          <span class="meta">新建密钥时的预填值</span>
        </div>
        <div class="card-body col" style="gap: 16px">
          <div class="field">
            <label>默认算法</label>
            <div class="algo-row">
              <button
                v-for="a in ALGOS"
                :key="a.id"
                class="algo-card"
                :class="{ on: p.defaultAlgorithm === a.id }"
                @click="patch({ defaultAlgorithm: a.id })"
              >
                <span class="algo-name">{{ a.name }}</span>
                <span class="algo-desc">{{ a.desc }}</span>
              </button>
            </div>
          </div>

          <div class="set-row" v-if="p.defaultAlgorithm === 'rsa'">
            <div class="set-main">
              <div class="set-title">RSA 位数</div>
              <div class="set-desc">
                <b>4096</b> 为推荐值；2048 可兼容极老的服务端，但已不建议
              </div>
            </div>
            <select
              class="select"
              style="width: 148px"
              :value="p.defaultRsaBits"
              @change="patch({ defaultRsaBits: Number($event.target.value) })"
            >
              <option :value="2048">2048 位</option>
              <option :value="3072">3072 位</option>
              <option :value="4096">4096 位</option>
            </select>
          </div>

          <div class="field">
            <label>
              默认注释（comment）
              <span class="mono-hint">会写进公钥末尾</span>
            </label>
            <input
              v-model="draft.defaultComment"
              class="input"
              placeholder="例如 leeli@workstation"
              spellcheck="false"
              @blur="patch({ defaultComment: draft.defaultComment.trim() })"
            />
            <span class="field-hint">
              留空则使用「用户名@主机名」。注释能帮你在 GitHub / GitLab 上认出这把钥匙。
            </span>
          </div>

          <div class="set-row">
            <div class="set-main">
              <div class="set-title">生成后自动加入 ssh-agent</div>
              <div class="set-desc">仅对无口令密钥有效；带口令的密钥需要你手动输入口令，不会静默加入</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="p.autoAddToAgent"
                @change="onToggle('autoAddToAgent')"
              />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>
        </div>
      </section>

      <!-- 安全策略 -->
      <section class="card-flat">
        <div class="card-head">
          <h2>安全策略</h2>
          <span class="meta">影响危险操作的门槛</span>
        </div>
        <div class="card-body col" style="gap: 0">
          <div class="set-row">
            <div class="set-main">
              <div class="set-title">删除前输入文件名确认</div>
              <div class="set-desc">删除密钥时要求手动键入文件名，防止误点</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="p.confirmDeleteTyped"
                @change="onToggle('confirmDeleteTyped')"
              />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>

          <div class="set-row">
            <div class="set-main">
              <div class="set-title">连通测试前二次确认</div>
              <div class="set-desc">测试会发起真实网络连接，确认后才会执行</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="p.confirmBeforeConnect"
                @change="onToggle('confirmBeforeConnect')"
              />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>

          <div class="set-row">
            <div class="set-main">
              <div class="set-title">启动时自动体检环境</div>
              <div class="set-desc">打开程序时扫描 OpenSSH、权限与 agent 状态</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="p.autocheckOnStart"
                @change="onToggle('autocheckOnStart')"
              />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>

          <div class="set-row">
            <div class="set-main">
              <div class="set-title">自动修复权限</div>
              <div class="set-desc">读取到过宽权限时提示并一键修复（不会静默改盘）</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="p.autoFixPerms"
                @change="onToggle('autoFixPerms')"
              />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>

          <div class="set-row">
            <div class="set-main">
              <div class="set-title">改 Config 前自动备份</div>
              <div class="set-desc">每次写入前留存一份可回滚的副本</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="p.autoBackupConfig"
                @change="onToggle('autoBackupConfig')"
              />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>

          <div class="set-row locked">
            <div class="set-main">
              <div class="set-title">
                强随机数校验（安全红线）
                <span class="pill pill-ok pill-sm"><i />已强制开启</span>
              </div>
              <div class="set-desc">
                检测到弱熵环境时拒绝生成密钥，避免产出可预测的私钥。此项不可关闭。
              </div>
            </div>
            <label class="switch">
              <input type="checkbox" checked disabled />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>
        </div>
      </section>

      <!-- 审计与日志 -->
      <section class="card-flat">
        <div class="card-head">
          <h2>审计与日志</h2>
          <div class="row">
            <button class="btn btn-ghost btn-sm" @click="exportCsv">
              <AppIcon name="download" :width="1.8" />
              导出 CSV
            </button>
            <button class="btn btn-ghost btn-sm" @click="pruneNow">
              <AppIcon name="trash" :width="1.8" />
              立即清理
            </button>
          </div>
        </div>
        <div class="card-body col" style="gap: 16px">
          <div class="set-row">
            <div class="set-main">
              <div class="set-title">启用审计日志</div>
              <div class="set-desc">记录密钥生成、Config 修改、权限修复、连通测试等敏感操作</div>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                :checked="p.auditEnabled"
                @change="onToggle('auditEnabled')"
              />
              <span class="track"><span class="knob" /></span>
            </label>
          </div>

          <div class="grid-2">
            <div class="field">
              <label>
                保留天数
                <span class="mono-hint">超期自动清理</span>
              </label>
              <input
                v-model.number="draft.auditRetentionDays"
                class="input"
                type="number"
                min="7"
                max="3650"
                @blur="onNumber('auditRetentionDays', 7, 3650)"
              />
            </div>
            <div class="field">
              <label>
                连通测试超时
                <span class="mono-hint">秒</span>
              </label>
              <input
                v-model.number="draft.connectTimeoutSecs"
                class="input"
                type="number"
                min="3"
                max="120"
                @blur="onNumber('connectTimeoutSecs', 3, 120)"
              />
            </div>
          </div>
          <div class="hint-row">
            <AppIcon name="info" :width="1.8" />
            <span>
              审计日志只记录操作的<strong>元信息</strong>（时间、类别、目标文件名、结果），
              绝不记录私钥内容或口令。
            </span>
          </div>
        </div>
      </section>
    </div>

    <!-- ═══ 右栏：本机数据位置 ══════════════════════════════════ -->
    <div class="col" style="gap: var(--space-5)">
      <section class="card-flat">
        <div class="card-head">
          <h2>本机数据位置</h2>
        </div>
        <div class="card-body col" style="gap: 14px">
          <div class="path-item">
            <div class="block-title" style="margin: 0">SSH 目录</div>
            <div class="path-line">
              <code class="truncate" :title="paths?.sshDir">{{ paths?.sshDir || '—' }}</code>
              <button class="icon-btn" title="复制" :disabled="!paths?.sshDir" @click="copyPath(paths?.sshDir)">
                <AppIcon name="copy" :width="1.8" />
              </button>
            </div>
            <div class="meta" style="margin-top: 4px">密钥与 config 的唯一真实存放位置</div>
          </div>

          <div class="path-item">
            <div class="block-title" style="margin: 0">程序数据目录</div>
            <div class="path-line">
              <code class="truncate" :title="dataDir">{{ dataDir || '—' }}</code>
              <button class="icon-btn" title="复制" :disabled="!dataDir" @click="copyPath(dataDir)">
                <AppIcon name="copy" :width="1.8" />
              </button>
            </div>
            <div class="meta" style="margin-top: 4px">
              存放偏好、审计日志、备份 —— <b>不含私钥副本</b>
            </div>
          </div>

          <div class="path-item">
            <div class="block-title" style="margin: 0">审计日志文件</div>
            <div class="path-line">
              <code class="truncate" :title="auditFile">{{ auditFile || '—' }}</code>
              <button class="icon-btn" title="复制" :disabled="!auditFile" @click="copyPath(auditFile)">
                <AppIcon name="copy" :width="1.8" />
              </button>
            </div>
            <div class="meta" style="margin-top: 4px">JSONL 格式，一行一条操作记录</div>
          </div>

          <div class="path-item">
            <div class="block-title" style="margin: 0">Config 备份</div>
            <div class="path-line">
              <code class="truncate" :title="paths?.configBackups">
                {{ paths?.configBackups || '—' }}
              </code>
              <button class="icon-btn" title="复制" :disabled="!paths?.configBackups" @click="copyPath(paths?.configBackups)">
                <AppIcon name="copy" :width="1.8" />
              </button>
            </div>
            <div class="meta" style="margin-top: 4px">
              已有 <b>{{ backups.length }}</b> 份历史备份
            </div>
          </div>

          <button class="btn btn-secondary btn-sm" style="width: 100%" @click="loadPaths">
            <AppIcon name="refresh" :width="1.8" />
            刷新路径信息
          </button>
        </div>
      </section>

      <!-- 环境摘要 -->
      <section class="card-flat">
        <div class="card-head">
          <h2>当前环境</h2>
        </div>
        <div class="card-body">
          <dl class="prop-grid">
            <dt>系统平台</dt>
            <dd>{{ platformLabel }}</dd>
            <dt>OpenSSH</dt>
            <dd :class="sys.error ? '' : sys.sshAvailable ? 'okc' : 'badc'">
              {{ sys.error ? '未知' : sys.sshLabel }}
            </dd>
            <dt>Git</dt>
            <dd>{{ sys.error ? '未知' : sys.gitVersion || '未安装' }}</dd>
            <dt>密钥数量</dt>
            <dd>{{ sys.error ? '—' : sys.keyCount }}</dd>
            <dt>config</dt>
            <dd>{{ sys.error ? '未知' : sys.hasConfig ? '已存在' : '未创建' }}</dd>
          </dl>
        </div>
      </section>

      <!-- 关于 -->
      <section class="card-flat">
        <div class="card-head">
          <h2>关于墨钥</h2>
        </div>
        <div class="card-body col" style="gap: 12px">
          <div class="about-row">
            <span class="logo-seal" style="position: static">墨</span>
            <div>
              <div style="font-weight: 700">墨钥 KeyHub</div>
              <div class="meta">v0.1.0-mvp · 本地优先</div>
            </div>
          </div>
          <div class="hint-row">
            <AppIcon name="shield" :width="1.8" />
            <span>
              墨钥默认<strong>不发起任何网络请求</strong>，不上传密钥、指纹或环境信息。
              唯一会联网的动作是你在「连接测试」里主动触发的 SSH 握手。
            </span>
          </div>
          <div class="about-list">
            <div>· 私钥操作时读取，用后清零，不留副本</div>
            <div>· 所有写盘操作先备份，可一键回滚</div>
            <div>· 底层调用系统原生 ssh / ssh-keygen，不做第三方实现</div>
          </div>
        </div>
      </section>
    </div>
  </div>

  <!-- 危险操作确认 -->
  <!-- 注意：confirmDialog 可能为 null，所有取值必须可选链，否则整页渲染崩溃 -->
  <AppModal
    v-model="confirmVisible"
    :title="confirmDialog?.title || ''"
    :dismissable="false"
    @close="confirmDialog = null"
  >
    <div v-if="confirmDialog" class="row" style="align-items: flex-start; gap: 12px">
      <AppIcon
        :name="confirmDialog.danger ? 'warning' : 'info'"
        :size="20"
        :width="1.8"
        :style="{ color: confirmDialog.danger ? 'var(--danger)' : 'var(--accent)', flex: '0 0 20px' }"
      />
      <p style="line-height: 1.65">{{ confirmDialog.body }}</p>
    </div>
    <template #footer>
      <button class="btn btn-secondary" @click="confirmDialog = null">取消</button>
      <button
        v-if="confirmDialog"
        class="btn"
        :class="confirmDialog.danger ? 'btn-danger-solid' : 'btn-primary'"
        @click="runConfirm"
      >
        {{ confirmDialog.confirmText }}
      </button>
    </template>
  </AppModal>
  </div>
</template>

<style scoped>
/* 设置行：左说明右控件的通用布局 */
.set-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 13px 0;
  border-top: 1px solid var(--border-soft);
}

.set-row:first-child {
  border-top: 0;
  padding-top: 0;
}

.set-row:last-child {
  padding-bottom: 0;
}

.set-row.locked {
  opacity: 0.92;
}

.set-main {
  min-width: 0;
  flex: 1;
}

.set-title {
  font-weight: 620;
  font-size: 13.5px;
  display: flex;
  align-items: center;
  gap: 9px;
  flex-wrap: wrap;
}

.set-desc {
  color: var(--muted);
  font-size: 12.5px;
  margin-top: 3px;
  line-height: 1.55;
}

.set-desc b {
  color: var(--fg);
}

/* 主题二选一分段控件 */
.seg {
  display: inline-flex;
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 2px;
  background: var(--bg);
  gap: 2px;
  flex: 0 0 auto;
}

.seg-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 13px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--muted);
  font-size: 12.5px;
  font-weight: 600;
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}

.seg-item.on {
  background: var(--surface);
  color: var(--accent);
  box-shadow: var(--elev-ring);
}

/* 算法选择卡片 */
.algo-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.algo-card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: flex-start;
  padding: 13px 15px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--surface);
  text-align: left;
  transition: border-color var(--motion-fast) var(--ease-standard),
    background var(--motion-fast) var(--ease-standard);
}

.algo-card:hover {
  border-color: var(--muted);
}

.algo-card.on {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.algo-name {
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 13px;
  color: var(--fg);
}

.algo-card.on .algo-name {
  color: var(--accent);
}

.algo-desc {
  font-size: 11.5px;
  color: var(--muted);
  line-height: 1.5;
}

/* 路径条目 */
.path-item {
  display: flex;
  flex-direction: column;
}

.path-line {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
}

.path-line code {
  flex: 1;
  min-width: 0;
  font-family: var(--font-mono);
  font-size: 11.5px;
  background: var(--bg);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  padding: 7px 10px;
  color: var(--fg-2);
}

.path-line .icon-btn {
  flex: 0 0 30px;
}

/* 环境摘要着色 */
.prop-grid dd.okc {
  color: var(--success-ink);
}

.prop-grid dd.badc {
  color: var(--danger-ink);
}

/* 关于块 */
.about-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.about-row .logo-seal {
  flex: 0 0 34px;
  width: 34px;
  height: 34px;
  border-radius: 10px;
  font-size: 15px;
}

.about-list {
  display: grid;
  gap: 6px;
  font-size: 12.5px;
  color: var(--muted);
  line-height: 1.6;
}

@media (max-width: 920px) {
  .algo-row {
    grid-template-columns: 1fr;
  }
}
</style>
