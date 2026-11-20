<script setup>
/**
 * SSH Config 可视化编辑器（PRD 3.1.3）
 *
 * 布局：左侧 308px Host 窗格 + 右侧编辑器
 * 模式：
 *   - Form：结构化编辑墨钥托管块（其它内容原样保留）
 *   - Raw ：直接编辑 config 全文
 * 安全：保存前自动备份，保存后设 600；底部常驻备份历史，可一键回滚
 */
import { ref, computed, onMounted, watch } from 'vue'
import * as api from '../lib/api'
import { useToast } from '../lib/toast'
import { useEnvStore } from '../lib/store'
import Icon from '../design/vue/ui/Icon.vue'
import PageHead from '../design/vue/ui/PageHead.vue'
import EmptyState from '../design/vue/ui/EmptyState.vue'
import FieldInput from '../design/vue/ui/FieldInput.vue'
import SelectMenu from '../design/vue/ui/SelectMenu.vue'
import SegmentedControl from '../design/vue/ui/SegmentedControl.vue'
import AppModal from '../components/AppModal.vue'
import AppDrawer from '../components/AppDrawer.vue'
import AppErrorState from '../components/AppErrorState.vue'

const toast = useToast()
const store = useEnvStore()

/* ══════ 文档状态 ══════ */
const loading = ref(true)
const loadError = ref(null)
const doc = ref(null)
const mode = ref('form') // form | raw
const modeOptions = [
  { value: 'form', label: '表单' },
  { value: 'raw', label: '原文' },
]
const dirty = ref(false)
const saving = ref(false)

/** Form 模式下的 Host 列表（深拷贝出来编辑，避免直接改 doc） */
const hosts = ref([])
/** Raw 模式下的原文 */
const rawText = ref('')

/** 原始快照，用于脏检测与「放弃更改」 */
const origin = ref({ hosts: [], raw: '' })

async function load() {
  loading.value = true
  loadError.value = null
  try {
    const d = await api.readConfig()
    doc.value = d
    hosts.value = (d.hosts || []).map((h) => ({ ...h, extra: h.extra || [] }))
    rawText.value = d.raw || ''
    origin.value = {
      hosts: JSON.parse(JSON.stringify(hosts.value)),
      raw: d.raw || '',
    }
    dirty.value = false
    await loadBackups()
  } catch (e) {
    loadError.value = e
  } finally {
    loading.value = false
  }
}

watch([hosts, rawText], () => {
  if (loading.value) return
  if (mode.value === 'form') {
    dirty.value = JSON.stringify(hosts.value) !== JSON.stringify(origin.value.hosts)
  } else {
    dirty.value = rawText.value !== origin.value.raw
  }
}, { deep: true })

/* ══════ Host 选择 ══════ */
const selectedId = ref(null)

const selected = computed(() =>
  hosts.value.find((h) => h.id === selectedId.value) || null
)

watch(
  hosts,
  () => {
    if (hosts.value.length && !hosts.value.some((h) => h.id === selectedId.value)) {
      selectedId.value = hosts.value[0].id
    }
    if (!hosts.value.length) selectedId.value = null
  },
  { immediate: true, deep: true }
)

/* Host 搜索 */
const hostQuery = ref('')
const filteredHosts = computed(() => {
  const kw = hostQuery.value.trim().toLowerCase()
  if (!kw) return hosts.value
  return hosts.value.filter(
    (h) =>
      h.alias.toLowerCase().includes(kw) ||
      (h.hostname || '').toLowerCase().includes(kw) ||
      (h.note || '').toLowerCase().includes(kw)
  )
})

/* ══════ 编辑操作 ══════ */
function addHost() {
  const existing = new Set(hosts.value.map((h) => h.alias.toLowerCase()))
  let alias = 'my-server'
  for (let i = 2; existing.has(alias); i++) alias = `my-server-${i}`
  const h = {
    id: `new_${Date.now()}`,
    alias,
    hostname: '',
    port: 22,
    user: '',
    identity: '',
    note: '',
    extra: [],
    managed: true,
    line: 0,
  }
  hosts.value = [...hosts.value, h]
  selectedId.value = h.id
  mode.value = 'form'
}

function removeHost(id) {
  const h = hosts.value.find((x) => x.id === id)
  if (!h) return
  hosts.value = hosts.value.filter((x) => x.id !== id)
  toast.push(`已移除 ${h.alias}（保存后生效）`, 'info')
}

function duplicateHost(id) {
  const h = hosts.value.find((x) => x.id === id)
  if (!h) return
  const existing = new Set(hosts.value.map((x) => x.alias.toLowerCase()))
  let alias = `${h.alias}-copy`
  for (let i = 2; existing.has(alias); i++) alias = `${h.alias}-copy-${i}`
  const copy = { ...JSON.parse(JSON.stringify(h)), id: `copy_${Date.now()}`, alias }
  hosts.value = [...hosts.value, copy]
  selectedId.value = copy.id
}

function patchHost(field, value) {
  const h = hosts.value.find((x) => x.id === selectedId.value)
  if (!h) return
  h[field] = value
  // 触发响应式（数组内对象替换）
  hosts.value = [...hosts.value]
}

/* ══════ 校验 ══════ */
const issues = ref([])
let validateTimer = null

function scheduleValidate() {
  clearTimeout(validateTimer)
  validateTimer = setTimeout(runValidate, 260)
}

async function runValidate() {
  try {
    if (mode.value === 'form') {
      issues.value = (await api.validateHosts(JSON.parse(JSON.stringify(hosts.value)))) || []
    } else {
      issues.value = (await api.validateConfig(rawText.value)) || []
    }
  } catch {
    issues.value = []
  }
}

watch([hosts, rawText, mode], scheduleValidate, { deep: true })

const errors = computed(() => issues.value.filter((i) => i.level === 'error'))
const warnings = computed(() => issues.value.filter((i) => i.level === 'warn'))
const infos = computed(() => issues.value.filter((i) => i.level === 'info'))
const hasBlocking = computed(() => errors.value.length > 0)

/** 每个 Host 的问题数（窗格上打点） */
const issueByAlias = computed(() => {
  const map = {}
  for (const i of issues.value) {
    if (!i.alias) continue
    const k = i.alias.toLowerCase()
    if (!map[k]) map[k] = { error: 0, warn: 0 }
    if (i.level === 'error') map[k].error++
    else if (i.level === 'warn') map[k].warn++
  }
  return map
})

/* ══════ 保存 / 回滚 ══════ */
const saveOpen = ref(false)
const saveNote = ref('')

function askSave() {
  if (!dirty.value) return
  if (hasBlocking.value) {
    toast.push('存在必须修正的问题，请先处理校验清单中的错误', 'danger')
    return
  }
  saveNote.value = ''
  saveOpen.value = true
}

async function doSave() {
  saving.value = true
  try {
    let res
    if (mode.value === 'form') {
      res = await api.writeHosts(JSON.parse(JSON.stringify(hosts.value)))
    } else {
      res = await api.writeConfig(rawText.value)
    }
    toast.push(
      `已保存（${res.lines} 行${res.backup ? `，备份 ${res.backup}` : ''}）`,
      'success'
    )
    saveOpen.value = false
    await load()
    await store.refreshEnv()
  } catch (e) {
    toast.push(e.message || '保存失败', 'danger')
  } finally {
    saving.value = false
  }
}

function discard() {
  hosts.value = JSON.parse(JSON.stringify(origin.value.hosts))
  rawText.value = origin.value.raw
  dirty.value = false
  toast.push('已放弃未保存的更改', 'info')
}

/* 备份历史 */
const backups = ref([])
const backupsOpen = ref(false)
const rollbackTarget = ref(null)
const rollbackOpen = ref(false)
const rollingBack = ref(false)

async function loadBackups() {
  try {
    backups.value = (await api.listConfigBackups()) || []
  } catch {
    backups.value = []
  }
}

const previewOpen = ref(false)
const previewText = ref('')
const previewName = ref('')

async function previewBackup(b) {
  try {
    previewText.value = await api.readConfigBackup(b.name)
    previewName.value = b.name
    previewOpen.value = true
  } catch (e) {
    toast.push(e.message || '读取备份失败', 'danger')
  }
}

function askRollback(b) {
  rollbackTarget.value = b
  rollbackOpen.value = true
}

async function doRollback() {
  if (!rollbackTarget.value) return
  rollingBack.value = true
  try {
    await api.rollbackConfig(rollbackTarget.value.name)
    toast.push('已回滚到所选备份', 'success')
    rollbackOpen.value = false
    backupsOpen.value = false
    await load()
  } catch (e) {
    toast.push(e.message || '回滚失败', 'danger')
  } finally {
    rollingBack.value = false
  }
}

async function deleteBackup(b) {
  try {
    await api.deleteConfigBackup(b.name)
    toast.push('备份已删除', 'info')
    await loadBackups()
  } catch (e) {
    toast.push(e.message || '删除失败', 'danger')
  }
}

/* 生成初始 config */
async function createSample() {
  try {
    const s = await api.sampleConfig()
    rawText.value = s
    mode.value = 'raw'
    toast.push('已生成示例配置，请按需修改后保存', 'info')
  } catch (e) {
    toast.push(e.message || '生成失败', 'danger')
  }
}

function fmtTime(s) {
  if (!s) return '—'
  return s.slice(0, 19).replace('T', ' ')
}

/** 从 IdentityFile 路径提取文件名，用于匹配下拉选中项 */
function matchKeyName(identity) {
  if (!identity) return ''
  const name = String(identity).split(/[/\\]/).pop() || ''
  return keyOptions.value.some((k) => k.filename === name) ? name : ''
}

/** 字节可读化 */
function fmtSize(n) {
  if (!n) return '0 B'
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(2)} MB`
}

/** 备份原因中文标签 */
function reasonLabel(r) {
  return { save: '保存前', rollback: '回滚前', edit: '编辑' }[r] || r || '备份'
}

/** 渲染单个 Host 将写入的文本（与后端 render_managed_block 保持一致） */
function renderPreview(h) {
  const lines = []
  const note = (h.note || '').trim() || '由墨钥管理'
  lines.push(`# ─── ${h.alias} · ${note} · 由墨钥管理`)
  lines.push(`Host ${h.alias}`)
  if ((h.hostname || '').trim()) lines.push(`    HostName ${h.hostname.trim()}`)
  if (h.port > 0) lines.push(`    Port ${h.port}`)
  if ((h.user || '').trim()) lines.push(`    User ${h.user.trim()}`)
  if ((h.identity || '').trim()) {
    lines.push(`    IdentityFile ${h.identity.trim()}`)
    lines.push('    IdentitiesOnly yes')
  }
  for (const pair of h.extra || []) {
    if (pair[0] === 'HostAlias' || !pair[0]) continue
    lines.push(`    ${pair[0]} ${pair[1]}`)
  }
  return lines.join('\n')
}

/* 密钥下拉（IdentityFile 选择） */
const keyOptions = ref([])

const keySelectOptions = computed(() =>
  keyOptions.value.map((k) => ({ value: k.filename, label: `${k.filename} · ${k.algorithm}` }))
)

async function loadKeys() {
  try {
    keyOptions.value = (await api.scanKeys()) || []
  } catch {
    keyOptions.value = []
  }
}

onMounted(async () => {
  await Promise.allSettled([load(), loadKeys()])
  runValidate()
})
</script>

<template>
  <div class="kh-page">
    <PageHead
      title="SSH Config"
      desc="可视化编辑 ~/.ssh/config。墨钥只重写自己托管的块，块外的配置一字不动。"
    />

    <!-- 顶部状态条 -->
    <div class="kh-toolbar">
      <div class="kh-toolbar__left">
        <span class="kh-path">
          <Icon name="file" :size="15" />
          <code>{{ doc?.prettyPath || '~/.ssh/config' }}</code>
        </span>
        <span
          v-if="doc?.exists"
          class="anx-badge"
          :class="doc.permCompliant ? 'anx-badge--ok' : 'anx-badge--warn'"
        >
          {{ doc.permDisplay || '—' }}
        </span>
        <span v-else class="anx-badge anx-badge--neutral">文件不存在</span>
        <span v-if="doc?.hasManagedBlock" class="anx-badge anx-badge--ok">
          <Icon name="check" :size="12" />
          含墨钥托管块
        </span>
      </div>
      <div class="kh-toolbar__right">
        <SegmentedControl v-model="mode" :options="modeOptions" />
        <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="backupsOpen = true">
          <Icon name="clock" :size="15" />
          备份历史
          <span v-if="backups.length" class="anx-tab__count">{{ backups.length }}</span>
        </button>
      </div>
    </div>

    <!-- 加载 / 错误 -->
    <div v-if="loading" class="anx-empty">
      <span class="kh-spin kh-spin--lg" />
      <p class="anx-empty__title">正在解析 config …</p>
    </div>

    <AppErrorState
      v-else-if="loadError"
      title="无法读取 ~/.ssh/config"
      :detail="loadError.message"
      @retry="load"
    />

    <!-- 不存在时引导 -->
    <EmptyState
      v-else-if="!doc?.exists"
      icon="file-text"
      title="还没有 SSH config 文件"
      text="config 能让你用简短的别名代替冗长的连接参数"
    >
      <template #actions>
        <button class="anx-btn anx-btn--primary anx-btn--sm" @click="createSample">生成示例配置</button>
        <button class="anx-btn anx-btn--sm" @click="mode = 'raw'">空白开始</button>
      </template>
    </EmptyState>

    <!-- 主体 -->
    <div v-else class="kh-cfg kh-collapse">
      <!-- 左：Host 窗格 -->
      <aside v-if="mode === 'form'" class="anx-panel kh-cfg-pane">
        <div class="anx-panel__head kh-cfg-pane__head">
          <div class="kh-searchfield kh-grow">
            <Icon name="search" :size="14" />
            <input v-model="hostQuery" class="anx-input" type="text" placeholder="筛选 Host…" aria-label="筛选 Host" />
          </div>
          <button class="anx-btn anx-btn--primary anx-btn--sm" title="新增 Host" @click="addHost">
            <Icon name="plus" :size="15" />
          </button>
        </div>

        <div class="kh-cfg-pane__list">
          <div v-if="!hosts.length" class="anx-empty kh-empty-inline">
            <div class="anx-empty__icon"><Icon name="file-text" :size="22" /></div>
            <p class="anx-empty__title">还没有 Host</p>
            <button class="anx-btn anx-btn--primary anx-btn--sm" @click="addHost">新增第一条</button>
          </div>

          <button
            v-for="h in filteredHosts"
            :key="h.id"
            type="button"
            class="kh-host-item"
            :aria-current="selectedId === h.id ? 'true' : undefined"
            @click="selectedId = h.id"
          >
            <span class="kh-host-item__ico"><Icon name="server" :size="14" /></span>
            <span class="kh-host-item__body">
              <span class="kh-host-item__name">
                <strong>{{ h.alias || '(未命名)' }}</strong>
                <span v-if="issueByAlias[(h.alias || '').toLowerCase()]?.error" class="kh-dot kh-dot--danger" />
                <span v-else-if="issueByAlias[(h.alias || '').toLowerCase()]?.warn" class="kh-dot kh-dot--warn" />
              </span>
              <span class="kh-host-item__sub">
                {{ h.user ? h.user + '@' : '' }}{{ h.hostname || '—' }}{{ h.port && h.port !== 22 ? ':' + h.port : '' }}
              </span>
              <span v-if="h.note" class="kh-host-item__note">{{ h.note }}</span>
            </span>
          </button>
        </div>

        <div class="anx-panel__foot kh-cfg-pane__foot">
          <span>{{ hosts.length }} 条 Host</span>
          <span v-if="!doc.hasManagedBlock" class="kh-fine kh-warn-text" title="保存后会创建墨钥托管块">
            <Icon name="info" :size="12" /> 保存后创建托管块
          </span>
        </div>
      </aside>

      <!-- 右：编辑器 -->
      <section class="anx-panel kh-cfg-editor" :class="{ 'kh-cfg-editor--full': mode !== 'form' }">
        <!-- Form 模式 -->
        <template v-if="mode === 'form'">
          <div v-if="!selected" class="anx-empty">
            <div class="anx-empty__icon"><Icon name="file-text" :size="24" /></div>
            <p class="anx-empty__title">选择或新增一个 Host</p>
            <p class="anx-empty__text">左侧列表中的每一条都对应 config 里的一个 Host 段</p>
          </div>

          <template v-else>
            <div class="anx-panel__head">
              <div>
                <h2>{{ selected.alias || '未命名 Host' }}</h2>
                <p>{{ selected.managed ? '墨钥托管' : '已有配置（保存后纳入托管）' }}</p>
              </div>
              <div class="anx-panel__head-actions">
                <button class="anx-btn anx-btn--ghost anx-btn--sm" title="复制" @click="duplicateHost(selected.id)">
                  <Icon name="copy" :size="14" />
                  复制
                </button>
                <button
                  class="anx-btn anx-btn--danger-quiet anx-btn--sm"
                  title="移除"
                  @click="removeHost(selected.id)"
                >
                  <Icon name="trash" :size="14" />
                  移除
                </button>
              </div>
            </div>

            <div class="anx-panel__body kh-form">
              <div class="kh-form-grid">
                <FieldInput
                  label="Host 别名"
                  required
                  mono
                  placeholder="github-work"
                  :model-value="selected.alias"
                  hint="用于 ssh 别名 直接连接，不能含空格"
                  @update:model-value="patchHost('alias', $event)"
                />
                <FieldInput
                  label="主机名 / IP"
                  mono
                  placeholder="github.com 或 192.168.1.10"
                  :model-value="selected.hostname"
                  hint="留空则直接用别名作为域名"
                  @update:model-value="patchHost('hostname', $event)"
                />
              </div>

              <div class="kh-form-grid">
                <FieldInput
                  label="登录用户"
                  mono
                  placeholder="root / ubuntu / git"
                  :model-value="selected.user"
                  @update:model-value="patchHost('user', $event)"
                />
                <div class="anx-field kh-field">
                  <label class="anx-field__label" for="cfg-port">端口</label>
                  <input
                    id="cfg-port"
                    class="anx-input anx-input--mono"
                    type="number"
                    min="1"
                    max="65535"
                    placeholder="22"
                    :value="selected.port || ''"
                    @input="patchHost('port', Number($event.target.value) || 0)"
                  />
                  <p v-if="selected.port > 65535" class="anx-field__error">端口超出 1-65535 范围</p>
                </div>
              </div>

              <div class="anx-field kh-field">
                <span class="anx-field__label">IdentityFile（私钥）</span>
                <div class="kh-id-row">
                  <SelectMenu
                    :model-value="matchKeyName(selected.identity)"
                    :options="keySelectOptions"
                    placeholder="（不指定）"
                    aria-label="选择已有密钥"
                    @update:model-value="patchHost('identity', $event ? '~/.ssh/' + $event : '')"
                  />
                  <input
                    class="anx-input anx-input--mono"
                    type="text"
                    placeholder="~/.ssh/id_ed25519"
                    :value="selected.identity"
                    @input="patchHost('identity', $event.target.value)"
                  />
                </div>
                <p class="anx-field__hint">
                  选择已有密钥会自动填写为 <code class="kh-code-inline">~/.ssh/文件名</code>；也可手动输入任意路径
                </p>
              </div>

              <FieldInput
                label="备注"
                placeholder="工作账号 / 公司 GitLab 等，仅作说明"
                :model-value="selected.note"
                hint="备注会以注释形式写在 Host 上方，不影响 SSH 行为"
                @update:model-value="patchHost('note', $event)"
              />

              <!-- 额外参数（原样保留） -->
              <div v-if="selected.extra?.filter((e) => e[0] !== 'HostAlias').length" class="anx-field kh-field">
                <span class="anx-field__label">其它参数（原样保留）</span>
                <div class="kh-extra-list">
                  <div
                    v-for="(pair, i) in selected.extra.filter((e) => e[0] !== 'HostAlias')"
                    :key="i"
                    class="kh-extra-row"
                  >
                    <code>{{ pair[0] }}</code>
                    <span>{{ pair[1] }}</span>
                  </div>
                </div>
                <p class="anx-field__hint">
                  这些是墨钥未识别为常用字段的配置（如 ProxyJump、ForwardAgent），
                  保存时会原样写入，不会被丢弃。
                </p>
              </div>

              <!-- 该 Host 的实时预览 -->
              <div class="anx-field kh-field">
                <span class="anx-field__label">将写入的内容</span>
                <pre class="kh-pre">{{ renderPreview(selected) }}</pre>
              </div>
            </div>
          </template>
        </template>

        <!-- Raw 模式 -->
        <template v-else>
          <div class="anx-panel__head">
            <div>
              <h2>原文编辑</h2>
              <p>直接编辑完整文件。保存时若校验不通过（如 Host 重复、密钥不存在）将被拒绝</p>
            </div>
            <div class="anx-panel__head-actions">
              <span class="anx-t-mono">{{ rawText.split('\n').length }} 行</span>
            </div>
          </div>
          <textarea
            v-model="rawText"
            class="anx-textarea kh-raw-editor"
            spellcheck="false"
            placeholder="Host my-server&#10;    HostName 192.168.1.10&#10;    User root&#10;    Port 22&#10;    IdentityFile ~/.ssh/id_ed25519&#10;    IdentitiesOnly yes"
          />
        </template>
      </section>

      <!-- 右：校验清单 -->
      <aside class="anx-panel kh-cfg-issues">
        <div class="anx-panel__head kh-cfg-issues__head">
          <div>
            <h2>校验清单</h2>
            <div class="kh-actions">
              <span v-if="!issues.length" class="anx-badge anx-badge--ok">
                <Icon name="check" :size="12" /> 全部通过
              </span>
              <span v-if="errors.length" class="anx-badge anx-badge--danger">{{ errors.length }} 错误</span>
              <span v-if="warnings.length" class="anx-badge anx-badge--warn">{{ warnings.length }} 提醒</span>
              <span v-if="infos.length" class="anx-badge anx-badge--neutral">{{ infos.length }} 建议</span>
            </div>
          </div>
        </div>

        <div class="kh-cfg-issues__body">
          <div v-if="!issues.length" class="anx-empty kh-empty-inline">
            <div class="anx-empty__icon kh-ok-icon"><Icon name="check" :size="22" /></div>
            <p class="anx-empty__title">没有发现问题</p>
            <p class="anx-empty__text">所有 Host 的别名、路径与端口都通过了检查。</p>
          </div>

          <div v-for="(i, idx) in issues" :key="idx" class="kh-issue" :class="'kh-issue--' + i.level">
            <Icon
              :name="i.level === 'error' ? 'alert-triangle' : 'info'"
              :size="15"
              class="kh-issue__icon"
            />
            <div class="kh-grow">
              <div class="kh-issue__msg">{{ i.message }}</div>
              <div v-if="i.hint" class="kh-issue__hint">{{ i.hint }}</div>
            </div>
          </div>
        </div>

        <!-- 保存栏 -->
        <div class="anx-panel__foot kh-cfg-save">
          <div class="kh-fine" :class="dirty ? 'kh-warn-text' : ''">
            <Icon :name="dirty ? 'info' : 'check'" :size="13" />
            {{ dirty ? '有未保存的更改' : '已是最新' }}
          </div>
          <div class="kh-actions">
            <button class="anx-btn anx-btn--ghost anx-btn--sm" :disabled="!dirty" @click="discard">放弃</button>
            <button
              class="anx-btn anx-btn--primary anx-btn--sm"
              :disabled="!dirty || saving || hasBlocking"
              @click="askSave"
            >
              <span v-if="saving" class="anx-btn__spinner" />
              保存
            </button>
          </div>
          <p v-if="hasBlocking" class="kh-fine kh-danger-text">
            存在 {{ errors.length }} 个错误，修复后才能保存
          </p>
        </div>
      </aside>
    </div>

    <!-- ══════ 保存确认 ══════ -->
    <AppModal v-model="saveOpen" title="保存 config" :dismissable="false">
      <div class="kh-confirm">
        <span class="kh-cf-icon kh-ok-icon"><Icon name="check" :size="20" /></span>
        <p class="anx-t-sm">
          即将写入 <code class="kh-code-inline">~/.ssh/config</code>。
        </p>
        <ul class="kh-check-list">
          <li>保存前会自动备份当前文件，可随时回滚</li>
          <li>保存后文件权限会被设为 600（仅属主可读写）</li>
          <li v-if="mode === 'form'">表单模式只重写墨钥托管块，其余内容原样保留</li>
        </ul>
        <FieldInput
          v-model="saveNote"
          label="备份备注（可选）"
          placeholder="例如：新增公司 GitLab 配置"
        />
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="saveOpen = false">取消</button>
        <button class="anx-btn anx-btn--primary" :disabled="saving" @click="doSave">
          <span v-if="saving" class="anx-btn__spinner" />
          确认保存
        </button>
      </template>
    </AppModal>

    <!-- ══════ 备份历史抽屉 ══════ -->
    <AppDrawer v-model="backupsOpen" title="config 备份历史">
      <div v-if="!backups.length" class="anx-empty">
        <div class="anx-empty__icon"><Icon name="clock" :size="22" /></div>
        <p class="anx-empty__title">还没有备份</p>
        <p class="anx-empty__text">每次保存 config 时墨钥都会自动创建一份备份</p>
      </div>
      <ul v-else class="kh-list">
        <li v-for="b in backups" :key="b.name" class="kh-list__item">
          <span class="kh-host-item__ico"><Icon name="clock" :size="14" /></span>
          <div class="kh-list__body">
            <div class="kh-list__title">{{ b.name }}</div>
            <div class="kh-list__text kh-mono">
              {{ b.createdAt?.slice(0, 19).replace('T', ' ') || '—' }}
              · {{ b.lines }} 行 · {{ fmtSize(b.size) }}
              <span class="anx-tag kh-ml-sm">{{ reasonLabel(b.reason) }}</span>
            </div>
          </div>
          <div class="kh-actions">
            <button class="anx-iconbtn" title="预览" aria-label="预览备份" @click="previewBackup(b)">
              <Icon name="eye" :size="15" />
            </button>
            <button class="anx-btn anx-btn--primary anx-btn--sm" @click="askRollback(b)">回滚</button>
            <button
              class="anx-iconbtn kh-danger-text"
              title="删除"
              aria-label="删除备份"
              @click="deleteBackup(b)"
            >
              <Icon name="trash" :size="15" />
            </button>
          </div>
        </li>
      </ul>
    </AppDrawer>

    <!-- 备份预览 -->
    <AppModal v-model="previewOpen" :title="'预览 · ' + previewName" wide>
      <pre class="kh-pre kh-pre--plain">{{ previewText }}</pre>
    </AppModal>

    <!-- 回滚确认 -->
    <AppModal v-model="rollbackOpen" title="回滚 config" :dismissable="false">
      <div v-if="rollbackTarget" class="kh-confirm">
        <span class="kh-cf-icon kh-cf-icon--warn"><Icon name="alert-triangle" :size="20" /></span>
        <p class="anx-t-sm">
          将把 <code class="kh-code-inline">~/.ssh/config</code> 恢复为
          <code class="kh-code-inline">{{ rollbackTarget.name }}</code> 的内容。
        </p>
        <ul class="kh-check-list">
          <li>当前文件会先被另存为一份 <code class="kh-code-inline">rollback</code> 备份，操作可逆</li>
          <li v-if="dirty">你有未保存的编辑，回滚后这些改动将丢失</li>
        </ul>
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="rollbackOpen = false">取消</button>
        <button class="anx-btn anx-btn--primary" :disabled="rollingBack" @click="doRollback">
          <span v-if="rollingBack" class="anx-btn__spinner" />
          确认回滚
        </button>
      </template>
    </AppModal>
  </div>
</template>
