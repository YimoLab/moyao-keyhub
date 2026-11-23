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
import Icon from '../design/vue/ui/Icon.vue'
import PageHead from '../design/vue/ui/PageHead.vue'
import FieldInput from '../design/vue/ui/FieldInput.vue'
import SelectMenu from '../design/vue/ui/SelectMenu.vue'
import SegmentedControl from '../design/vue/ui/SegmentedControl.vue'
import KvList from '../design/vue/ui/KvList.vue'
import AppModal from '../components/AppModal.vue'
import PageSkeleton from '../components/PageSkeleton.vue'
import BrandMark from '../components/BrandMark.vue'
import { getVersion } from '@tauri-apps/api/app'
import { checkForUpdates, updateState } from '../lib/updater.js'
import { useEnvStore } from '../lib/store.js'
import * as api from '../lib/api.js'
import { useToast } from '../lib/toast.js'
import { useTheme } from '../lib/theme.js'

const env = useEnvStore()
/** 环境摘要的只读快照，模板里少写一层链 */
const sys = env.env
const toast = useToast()
const { theme, toggleTheme } = useTheme()

const themeOptions = [
  { value: 'light', label: '亮色' },
  { value: 'dark', label: '暗色' },
]
const densityOptions = [
  { value: 'comfortable', label: '舒适（默认）' },
  { value: 'compact', label: '紧凑' },
]
const titlebarOptions = [
  { value: 'auto', label: '跟随系统' },
  { value: 'windows', label: 'Windows' },
  { value: 'mac', label: 'macOS' },
]
const rsaBitsOptions = [
  { value: '2048', label: '2048 位' },
  { value: '3072', label: '3072 位' },
  { value: '4096', label: '4096 位' },
]

const saving = ref(false)
const paths = ref(null)
const backups = ref([])
const appVersion = ref('—')

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

/*
 * 事件处理器直接接收事件对象。
 * 注意：模板里必须写成 onToggle('key', $event) —— 写成 onToggle('key') 时
 * Vue 只会调用它一次并丢弃返回值，开关看起来变了，实际不会落盘。
 */
function onToggle(key, e) {
  patch({ [key]: e.target.checked })
}

function onSelect(key, e) {
  patch({ [key]: e.target.value })
}

function onNumber(key, min, max) {
  let v = Number(draft[key])
  if (!Number.isFinite(v)) v = p.value[key]
  v = Math.min(max, Math.max(min, Math.round(v)))
  draft[key] = v
  if (v !== p.value[key]) patch({ [key]: v })
}

/* ── 主题 ───────────────────────────────────────────────────── */
function setTheme(next) {
  if (theme.value !== next) toggleTheme()
  patch({ theme: next })
}

/* ── 标题栏风格（窗口装饰在启动时应用，切换后需重启） ───────── */
function setTitlebar(next) {
  if (next === p.value.titlebarStyle) return
  patch({ titlebarStyle: next })
  toast.push('标题栏风格将在重启应用后生效', 'info')
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
  try {
    appVersion.value = await getVersion()
  } catch {
    appVersion.value = '0.2.5'
  }
})

watch(() => env.prefs.loaded, syncDraft)

const platformLabel = computed(() => {
  if (sys.error) return '未知'
  const m = { windows: 'Windows', macos: 'macOS', linux: 'Linux' }
  return m[sys.platform] || sys.platform || '—'
})
</script>

<template>
  <!-- 必须是单根元素：App.vue 用 <RouteFade> 包路由组件 -->
  <div class="kh-page">
    <PageHead title="设置">
      <template #desc>
        应用偏好与本地数据位置。所有配置只保存在本机，程序不上传任何信息。
        <span v-if="saving" class="kh-fine">· 保存中…</span>
      </template>
      <template #actions>
        <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="rerunOnboarding">
          <Icon name="refresh" :size="15" />
          重新自检
        </button>
        <button class="anx-btn anx-btn--danger-quiet anx-btn--sm" @click="resetPrefs">
          <Icon name="rotate" :size="15" />
          恢复默认
        </button>
      </template>
    </PageHead>

    <PageSkeleton v-if="!env.prefs.loaded" variant="settings" />

    <div v-else class="kh-settings kh-collapse">
      <!-- ═══ 左栏：设置分组 ══════════════════════════════════════ -->
      <div class="kh-settings__col">
        <!-- 外观 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2>外观</h2><p>即时生效</p></div>
          </div>
          <div class="anx-panel__body kh-settings-body">
            <div class="kh-set-row">
              <div class="kh-set-main">
                <div class="kh-set-title">主题</div>
                <div class="kh-set-desc">亮色 / 暗色，会同步到所有页面与图表配色</div>
              </div>
              <SegmentedControl
                :model-value="theme"
                :options="themeOptions"
                @update:model-value="setTheme"
              />
            </div>

            <div class="kh-set-row">
              <div class="kh-set-main">
                <div class="kh-set-title">界面密度</div>
                <div class="kh-set-desc">紧凑模式会压缩表格行高，适合小屏或密钥较多时</div>
              </div>
              <SelectMenu
                :model-value="p.density"
                :options="densityOptions"
                aria-label="界面密度"
                style="width: 148px"
                @update:model-value="patch({ density: $event })"
              />
            </div>

            <div class="kh-set-row">
              <div class="kh-set-main">
                <div class="kh-set-title">标题栏风格</div>
                <div class="kh-set-desc">
                  Windows 与 macOS 两种风格均由墨钥自绘，切换后重启应用生效。
                </div>
              </div>
              <SelectMenu
                :model-value="p.titlebarStyle || 'auto'"
                :options="titlebarOptions"
                aria-label="标题栏风格"
                style="width: 148px"
                @update:model-value="setTitlebar"
              />
            </div>
          </div>
        </section>

        <!-- 密钥生成默认值 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2>密钥生成默认值</h2><p>新建密钥时的预填值</p></div>
          </div>
          <div class="anx-panel__body kh-settings-body">
            <div class="anx-field kh-field">
              <span class="anx-field__label">默认算法</span>
              <div class="kh-choice-row">
                <button
                  v-for="a in ALGOS"
                  :key="a.id"
                  type="button"
                  class="kh-choice"
                  :aria-pressed="p.defaultAlgorithm === a.id"
                  @click="patch({ defaultAlgorithm: a.id })"
                >
                  <strong>{{ a.name }}</strong>
                  <span>{{ a.desc }}</span>
                </button>
              </div>
            </div>

            <div v-if="p.defaultAlgorithm === 'rsa'" class="kh-set-row">
              <div class="kh-set-main">
                <div class="kh-set-title">RSA 位数</div>
                <div class="kh-set-desc"><b>4096</b> 为推荐值；2048 可兼容极老的服务端，但已不建议</div>
              </div>
              <SelectMenu
                :model-value="String(p.defaultRsaBits)"
                :options="rsaBitsOptions"
                aria-label="RSA 位数"
                style="width: 148px"
                @update:model-value="patch({ defaultRsaBits: Number($event) })"
              />
            </div>

            <FieldInput
              v-model="draft.defaultComment"
              label="默认注释（comment）"
              placeholder="例如 leeli@workstation"
              :spellcheck="false"
              hint="留空则使用「用户名@主机名」。注释能帮你在 GitHub / GitLab 上认出这把钥匙。"
              @focusout="patch({ defaultComment: draft.defaultComment.trim() })"
            />

            <div class="kh-set-row">
              <div class="kh-set-main">
                <div class="kh-set-title">生成后自动加入 ssh-agent</div>
                <div class="kh-set-desc">仅对无口令密钥有效；带口令的密钥需要你手动输入口令，不会静默加入</div>
              </div>
              <label class="anx-switch">
                <input type="checkbox" :checked="p.autoAddToAgent" @change="onToggle('autoAddToAgent', $event)" />
                <span class="anx-switch__track" />
              </label>
            </div>
          </div>
        </section>

        <!-- 安全策略 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2>安全策略</h2><p>影响危险操作的门槛</p></div>
          </div>
          <div class="anx-panel__body">
            <ul class="kh-list">
              <li class="kh-list__item">
                <div class="kh-list__body">
                  <div class="kh-list__title">删除前输入文件名确认</div>
                  <p class="kh-list__text">删除密钥时要求手动键入文件名，防止误点</p>
                </div>
                <label class="anx-switch">
                  <input type="checkbox" :checked="p.confirmDeleteTyped" @change="onToggle('confirmDeleteTyped', $event)" />
                  <span class="anx-switch__track" />
                </label>
              </li>
              <li class="kh-list__item">
                <div class="kh-list__body">
                  <div class="kh-list__title">连通测试前二次确认</div>
                  <p class="kh-list__text">测试会发起真实网络连接，确认后才会执行</p>
                </div>
                <label class="anx-switch">
                  <input type="checkbox" :checked="p.confirmBeforeConnect" @change="onToggle('confirmBeforeConnect', $event)" />
                  <span class="anx-switch__track" />
                </label>
              </li>
              <li class="kh-list__item">
                <div class="kh-list__body">
                  <div class="kh-list__title">启动时自动体检环境</div>
                  <p class="kh-list__text">打开程序时扫描 OpenSSH、权限与 agent 状态</p>
                </div>
                <label class="anx-switch">
                  <input type="checkbox" :checked="p.autocheckOnStart" @change="onToggle('autocheckOnStart', $event)" />
                  <span class="anx-switch__track" />
                </label>
              </li>
              <li class="kh-list__item">
                <div class="kh-list__body">
                  <div class="kh-list__title">自动修复权限</div>
                  <p class="kh-list__text">读取到过宽权限时提示并一键修复（不会静默改盘）</p>
                </div>
                <label class="anx-switch">
                  <input type="checkbox" :checked="p.autoFixPerms" @change="onToggle('autoFixPerms', $event)" />
                  <span class="anx-switch__track" />
                </label>
              </li>
              <li class="kh-list__item">
                <div class="kh-list__body">
                  <div class="kh-list__title">改 Config 前自动备份</div>
                  <p class="kh-list__text">每次写入前留存一份可回滚的副本</p>
                </div>
                <label class="anx-switch">
                  <input type="checkbox" :checked="p.autoBackupConfig" @change="onToggle('autoBackupConfig', $event)" />
                  <span class="anx-switch__track" />
                </label>
              </li>
              <li class="kh-list__item">
                <div class="kh-list__body">
                  <div class="kh-list__title">
                    强随机数校验（安全红线）
                    <span class="anx-badge anx-badge--ok"><Icon name="lock" :size="12" /> 已强制开启</span>
                  </div>
                  <p class="kh-list__text">检测到弱熵环境时拒绝生成密钥，避免产出可预测的私钥。此项不可关闭。</p>
                </div>
                <label class="anx-switch">
                  <input type="checkbox" checked disabled />
                  <span class="anx-switch__track" />
                </label>
              </li>
            </ul>
          </div>
        </section>

        <!-- 审计与日志 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2>审计与日志</h2></div>
            <div class="anx-panel__head-actions">
              <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="exportCsv">
                <Icon name="download" :size="14" /> 导出 CSV
              </button>
              <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="pruneNow">
                <Icon name="trash" :size="14" /> 立即清理
              </button>
            </div>
          </div>
          <div class="anx-panel__body kh-settings-body">
            <div class="kh-set-row">
              <div class="kh-set-main">
                <div class="kh-set-title">启用审计日志</div>
                <div class="kh-set-desc">记录密钥生成、Config 修改、权限修复、连通测试等敏感操作</div>
              </div>
              <label class="anx-switch">
                <input type="checkbox" :checked="p.auditEnabled" @change="onToggle('auditEnabled', $event)" />
                <span class="anx-switch__track" />
              </label>
            </div>

            <div class="kh-form-grid">
              <FieldInput
                :model-value="String(draft.auditRetentionDays)"
                label="保留天数"
                type="number"
                hint="超期自动清理"
                @update:model-value="draft.auditRetentionDays = $event"
                @focusout="onNumber('auditRetentionDays', 7, 3650)"
              />
              <FieldInput
                :model-value="String(draft.connectTimeoutSecs)"
                label="连通测试超时"
                type="number"
                hint="秒"
                @update:model-value="draft.connectTimeoutSecs = $event"
                @focusout="onNumber('connectTimeoutSecs', 3, 120)"
              />
            </div>

            <div class="kh-hint">
              <Icon name="info" :size="14" />
              <span>
                审计日志只记录操作的<strong>元信息</strong>（时间、类别、目标文件名、结果），
                绝不记录私钥内容或口令。
              </span>
            </div>
          </div>
        </section>
      </div>

      <!-- ═══ 右栏：本机数据位置 ══════════════════════════════════ -->
      <div class="kh-settings__col">
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2>本机数据位置</h2></div>
          </div>
          <div class="anx-panel__body kh-settings-body">
            <div class="kh-path-block">
              <div class="anx-t-xs anx-t-strong">SSH 目录</div>
              <div class="kh-path-line">
                <code class="kh-truncate" :title="paths?.sshDir">{{ paths?.sshDir || '—' }}</code>
                <button class="anx-iconbtn" title="复制" :disabled="!paths?.sshDir" @click="copyPath(paths?.sshDir)">
                  <Icon name="copy" :size="15" />
                </button>
              </div>
              <div class="anx-t-xs anx-t-muted">密钥与 config 的唯一真实存放位置</div>
            </div>

            <div class="kh-path-block">
              <div class="anx-t-xs anx-t-strong">程序数据目录</div>
              <div class="kh-path-line">
                <code class="kh-truncate" :title="dataDir">{{ dataDir || '—' }}</code>
                <button class="anx-iconbtn" title="复制" :disabled="!dataDir" @click="copyPath(dataDir)">
                  <Icon name="copy" :size="15" />
                </button>
              </div>
              <div class="anx-t-xs anx-t-muted">
                存放偏好、审计日志、备份 —— <b>不含私钥副本</b>
              </div>
            </div>

            <div class="kh-path-block">
              <div class="anx-t-xs anx-t-strong">审计日志文件</div>
              <div class="kh-path-line">
                <code class="kh-truncate" :title="auditFile">{{ auditFile || '—' }}</code>
                <button class="anx-iconbtn" title="复制" :disabled="!auditFile" @click="copyPath(auditFile)">
                  <Icon name="copy" :size="15" />
                </button>
              </div>
              <div class="anx-t-xs anx-t-muted">JSONL 格式，一行一条操作记录</div>
            </div>

            <div class="kh-path-block">
              <div class="anx-t-xs anx-t-strong">Config 备份</div>
              <div class="kh-path-line">
                <code class="kh-truncate" :title="paths?.configBackups">{{ paths?.configBackups || '—' }}</code>
                <button
                  class="anx-iconbtn"
                  title="复制"
                  :disabled="!paths?.configBackups"
                  @click="copyPath(paths?.configBackups)"
                >
                  <Icon name="copy" :size="15" />
                </button>
              </div>
              <div class="anx-t-xs anx-t-muted">已有 <b>{{ backups.length }}</b> 份历史备份</div>
            </div>

            <button class="anx-btn anx-btn--ghost anx-btn--sm anx-btn--block" @click="loadPaths">
              <Icon name="refresh" :size="14" />
              刷新路径信息
            </button>
          </div>
        </section>

        <!-- 环境摘要 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2>当前环境</h2></div>
          </div>
          <div class="anx-panel__body">
            <KvList
              :rows="[
                ['系统平台', platformLabel],
                ['OpenSSH', sys.error ? '未知' : sys.sshLabel],
                ['Git', sys.error ? '未知' : sys.gitVersion || '未安装'],
                ['密钥数量', sys.error ? '—' : String(sys.keyCount)],
                ['config', sys.error ? '未知' : sys.hasConfig ? '已存在' : '未创建'],
              ]"
              tight
            />
          </div>
        </section>

        <!-- 应用更新 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div>
              <h2>应用更新</h2>
              <p>v{{ appVersion }}</p>
            </div>
          </div>
          <div class="anx-panel__body kh-settings-body">
            <div class="kh-set-row">
              <div class="kh-set-main">
                <div class="kh-set-title">启动时自动检查更新</div>
                <div class="kh-set-desc">
                  默认关闭。开启后启动 5 秒检查一次，6 小时内不重复；
                  只访问 GitHub 的版本文件，不上传任何信息。
                </div>
              </div>
              <label class="anx-switch">
                <input
                  type="checkbox"
                  :checked="p.autoCheckUpdates"
                  @change="onToggle('autoCheckUpdates', $event)"
                />
                <span class="anx-switch__track" />
              </label>
            </div>
            <button
              class="anx-btn anx-btn--ghost anx-btn--sm anx-btn--block"
              :disabled="updateState.checking"
              @click="checkForUpdates({ silent: false })"
            >
              <span v-if="updateState.checking" class="anx-btn__spinner" />
              <Icon v-else name="refresh" :size="14" />
              {{ updateState.checking ? '检查中…' : '立即检查更新' }}
            </button>
            <p class="kh-fine">
              更新包会先用内置公钥校验签名，校验失败不会安装；安装完成后应用自动重启。
            </p>
          </div>
        </section>

        <!-- 关于 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2>关于墨钥</h2></div>
          </div>
          <div class="anx-panel__body kh-settings-body">
            <div class="kh-about-row">
              <span class="kh-brand-mark kh-brand-mark--static"><BrandMark /></span>
              <div>
                <div class="anx-t-strong">墨钥 KeyHub</div>
                <div class="anx-t-xs anx-t-muted">v{{ appVersion }} · 本地优先</div>
              </div>
            </div>
            <div class="kh-hint">
              <Icon name="shield" :size="14" />
              <span>
                墨钥不上传密钥、指纹或环境信息。可选的联网行为只有两个：
                你主动触发的 SSH 连接测试，以及你开启的更新检查（只读取 GitHub 的版本文件）。
              </span>
            </div>
            <ul class="kh-check-list">
              <li>私钥操作时读取，用后清零，不留副本</li>
              <li>所有写盘操作先备份，可一键回滚</li>
              <li>底层调用系统原生 ssh / ssh-keygen，不做第三方实现</li>
            </ul>
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
      <div v-if="confirmDialog" class="kh-row-top">
        <Icon
          :name="confirmDialog.danger ? 'alert-triangle' : 'info'"
          :size="20"
          :class="confirmDialog.danger ? 'kh-danger-text' : ''"
        />
        <p class="anx-t-sm">{{ confirmDialog.body }}</p>
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="confirmDialog = null">取消</button>
        <button
          v-if="confirmDialog"
          class="anx-btn"
          :class="confirmDialog.danger ? 'anx-btn--danger' : 'anx-btn--primary'"
          @click="runConfirm"
        >
          {{ confirmDialog.confirmText }}
        </button>
      </template>
    </AppModal>
  </div>
</template>
