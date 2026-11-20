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
import AppIcon from '../components/AppIcon.vue'
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
  <div class="cfg">
    <div class="page-head">
      <div>
        <h1>SSH Config</h1>
        <p class="sub">可视化编辑 ~/.ssh/config。墨钥只重写自己托管的块，块外的配置一字不动。</p>
      </div>
    </div>

    <!-- 顶部状态条 -->
    <div class="cfg-head">
      <div class="ch-left">
        <div class="ch-path">
          <AppIcon name="file" :size="14" />
          <code>{{ doc?.prettyPath || '~/.ssh/config' }}</code>
          <span v-if="doc?.exists" class="pill" :class="doc.permCompliant ? 'ok' : 'warn'">
            {{ doc.permDisplay || '—' }}
          </span>
          <span v-else class="pill idle">文件不存在</span>
        </div>
        <div v-if="doc?.hasManagedBlock" class="ch-managed">
          <AppIcon name="check" :size="12" />
          含墨钥托管块
        </div>
      </div>
      <div class="ch-right">
        <div class="seg small">
          <button :class="{ on: mode === 'form' }" @click="mode = 'form'">表单</button>
          <button :class="{ on: mode === 'raw' }" @click="mode = 'raw'">原文</button>
        </div>
        <button class="btn ghost sm" @click="backupsOpen = true">
          <AppIcon name="clock" :size="14" />
          备份历史
          <span v-if="backups.length" class="badge">{{ backups.length }}</span>
        </button>
      </div>
    </div>

    <!-- 加载 / 错误 -->
    <div v-if="loading" class="empty">
      <span class="spin lg" />
      <p>正在解析 config …</p>
    </div>

    <AppErrorState
      v-else-if="loadError"
      :title="'无法读取 ~/.ssh/config'"
      :detail="loadError.message"
      @retry="load"
    />

    <!-- 不存在时引导 -->
    <div v-else-if="!doc?.exists" class="empty">
      <span class="empty-seal"><AppIcon name="config" :size="26" /></span>
      <p>还没有 SSH config 文件</p>
      <span>config 能让你用简短的别名代替冗长的连接参数</span>
      <div class="row">
        <button class="btn primary sm" @click="createSample">生成示例配置</button>
        <button class="btn sm" @click="mode = 'raw'">空白开始</button>
      </div>
    </div>

    <!-- 主体 -->
    <div v-else class="cfg-body">
      <!-- 左：Host 窗格 -->
      <aside v-if="mode === 'form'" class="pane">
        <div class="pane-head">
          <div class="search sm">
            <AppIcon name="search" :size="14" />
            <input v-model="hostQuery" type="text" placeholder="筛选 Host…" />
          </div>
          <button class="btn primary sm icon-only" title="新增 Host" @click="addHost">
            <AppIcon name="plus" :size="15" />
          </button>
        </div>

        <div class="pane-list">
          <div v-if="!hosts.length" class="pane-empty">
            <AppIcon name="config" :size="20" />
            <span>还没有 Host</span>
            <button class="btn xs primary" @click="addHost">新增第一条</button>
          </div>

          <button
            v-for="h in filteredHosts"
            :key="h.id"
            class="host-item"
            :class="{ on: selectedId === h.id }"
            @click="selectedId = h.id"
          >
            <span class="hi-ico"><AppIcon name="server" :size="14" /></span>
            <div class="hi-body">
              <div class="hi-name">
                <strong>{{ h.alias || '(未命名)' }}</strong>
                <span v-if="issueByAlias[(h.alias || '').toLowerCase()]?.error" class="dot-err" />
                <span v-else-if="issueByAlias[(h.alias || '').toLowerCase()]?.warn" class="dot-warn" />
              </div>
              <span class="hi-sub">
                {{ h.user ? h.user + '@' : '' }}{{ h.hostname || '—' }}{{ h.port && h.port !== 22 ? ':' + h.port : '' }}
              </span>
              <span v-if="h.note" class="hi-note">{{ h.note }}</span>
            </div>
          </button>
        </div>

        <div class="pane-foot">
          <span>{{ hosts.length }} 条 Host</span>
          <span v-if="!doc.hasManagedBlock" class="pf-warn" title="保存后会创建墨钥托管块">
            <AppIcon name="info" :size="11" /> 保存后创建托管块
          </span>
        </div>
      </aside>

      <!-- 右：编辑器 -->
      <!-- full：raw 模式下没有左侧窗格，编辑器必须横跨整个 grid 行，否则会被塞进 280px 固定列 -->
      <section class="editor" :class="{ full: mode !== 'form' }">
        <!-- Form 模式 -->
        <template v-if="mode === 'form'">
          <div v-if="!selected" class="empty">
            <AppIcon name="config" :size="24" />
            <p>选择或新增一个 Host</p>
            <span>左侧列表中的每一条都对应 config 里的一个 Host 段</span>
          </div>

          <template v-else>
            <header class="ed-head">
              <div>
                <h3>{{ selected.alias || '未命名 Host' }}</h3>
                <span class="ed-sub">
                  {{ selected.managed ? '墨钥托管' : '已有配置（保存后纳入托管）' }}
                </span>
              </div>
              <div class="ed-actions">
                <button class="btn ghost sm" title="复制" @click="duplicateHost(selected.id)">
                  <AppIcon name="copy" :size="13" /> 复制
                </button>
                <button class="btn ghost sm danger-text" title="移除" @click="removeHost(selected.id)">
                  <AppIcon name="trash" :size="13" /> 移除
                </button>
              </div>
            </header>

            <div class="ed-form">
              <div class="grid2">
                <div class="field">
                  <label>Host 别名 <span class="req">*</span></label>
                  <input
                    :value="selected.alias"
                    type="text"
                    class="input mono"
                    placeholder="github-work"
                    @input="patchHost('alias', $event.target.value)"
                  />
                  <p class="hint-row">用于 <code>ssh 别名</code> 直接连接，不能含空格</p>
                </div>
                <div class="field">
                  <label>主机名 / IP</label>
                  <input
                    :value="selected.hostname"
                    type="text"
                    class="input mono"
                    placeholder="github.com 或 192.168.1.10"
                    @input="patchHost('hostname', $event.target.value)"
                  />
                  <p class="hint-row">留空则直接用别名作为域名</p>
                </div>
              </div>

              <div class="grid2">
                <div class="field">
                  <label>登录用户</label>
                  <input
                    :value="selected.user"
                    type="text"
                    class="input mono"
                    placeholder="root / ubuntu / git"
                    @input="patchHost('user', $event.target.value)"
                  />
                </div>
                <div class="field">
                  <label>端口</label>
                  <input
                    :value="selected.port || ''"
                    type="number"
                    min="1"
                    max="65535"
                    class="input mono"
                    placeholder="22"
                    @input="patchHost('port', Number($event.target.value) || 0)"
                  />
                  <p v-if="selected.port > 65535" class="hint-row danger-text">
                    端口超出 1-65535 范围
                  </p>
                </div>
              </div>

              <div class="field">
                <label>IdentityFile（私钥）</label>
                <div class="id-row">
                  <select
                    class="select mono"
                    :value="matchKeyName(selected.identity)"
                    @change="patchHost('identity', $event.target.value ? '~/.ssh/' + $event.target.value : '')"
                  >
                    <option value="">（不指定）</option>
                    <option v-for="k in keyOptions" :key="k.filename" :value="k.filename">
                      {{ k.filename }} · {{ k.algorithm }}
                    </option>
                  </select>
                  <input
                    :value="selected.identity"
                    type="text"
                    class="input mono"
                    placeholder="~/.ssh/id_ed25519"
                    @input="patchHost('identity', $event.target.value)"
                  />
                </div>
                <p class="hint-row">
                  选择已有密钥会自动填写为 <code>~/.ssh/文件名</code>；也可手动输入任意路径
                </p>
              </div>

              <div class="field">
                <label>备注</label>
                <input
                  :value="selected.note"
                  type="text"
                  class="input"
                  placeholder="工作账号 / 公司 GitLab 等，仅作说明"
                  @input="patchHost('note', $event.target.value)"
                />
                <p class="hint-row">备注会以注释形式写在 Host 上方，不影响 SSH 行为</p>
              </div>

              <!-- 额外参数（原样保留） -->
              <div v-if="selected.extra?.filter((e) => e[0] !== 'HostAlias').length" class="field">
                <label>其它参数（原样保留）</label>
                <div class="extra-list">
                  <div
                    v-for="(pair, i) in selected.extra.filter((e) => e[0] !== 'HostAlias')"
                    :key="i"
                    class="extra-row"
                  >
                    <code>{{ pair[0] }}</code>
                    <span>{{ pair[1] }}</span>
                  </div>
                </div>
                <p class="hint-row">
                  这些是墨钥未识别为常用字段的配置（如 ProxyJump、ForwardAgent），
                  保存时会原样写入，不会被丢弃。
                </p>
              </div>

              <!-- 该 Host 的实时预览 -->
              <div class="field">
                <label>将写入的内容</label>
                <div class="rawbox">
                  <code>{{ renderPreview(selected) }}</code>
                </div>
              </div>
            </div>
          </template>
        </template>

        <!-- Raw 模式 -->
        <template v-else>
          <header class="ed-head">
            <div>
              <h3>原文编辑</h3>
              <span class="ed-sub">
                直接编辑完整文件。保存时若校验不通过（如 Host 重复、密钥不存在）将被拒绝
              </span>
            </div>
            <div class="ed-actions">
              <span class="mono-sm">{{ rawText.split('\n').length }} 行</span>
            </div>
          </header>
          <textarea
            v-model="rawText"
            class="raw-editor"
            spellcheck="false"
            placeholder="Host my-server&#10;    HostName 192.168.1.10&#10;    User root&#10;    Port 22&#10;    IdentityFile ~/.ssh/id_ed25519&#10;    IdentitiesOnly yes"
          />
        </template>
      </section>

      <!-- 右：校验清单 -->
      <aside class="issues">
        <div class="is-head">
          <span class="is-title">校验清单</span>
          <span v-if="!issues.length" class="pill ok"><AppIcon name="check" :size="11" /> 全部通过</span>
          <div v-else class="is-counts">
            <span v-if="errors.length" class="pill danger">{{ errors.length }} 错误</span>
            <span v-if="warnings.length" class="pill warn">{{ warnings.length }} 提醒</span>
            <span v-if="infos.length" class="pill idle">{{ infos.length }} 建议</span>
          </div>
        </div>

        <div class="is-body">
          <div v-if="!issues.length" class="is-ok">
            <span class="is-seal"><AppIcon name="check" :size="20" /></span>
            <strong>没有发现问题</strong>
            <p>所有 Host 的别名、路径与端口都通过了检查。</p>
          </div>

          <div
            v-for="(i, idx) in issues"
            :key="idx"
            class="issue"
            :class="i.level"
          >
            <span class="is-icon">
              <AppIcon
                :name="i.level === 'error' ? 'warning' : i.level === 'warn' ? 'info' : 'info'"
                :size="14"
              />
            </span>
            <div class="is-text">
              <div class="is-msg">{{ i.message }}</div>
              <div v-if="i.hint" class="is-hint">{{ i.hint }}</div>
            </div>
          </div>
        </div>

        <!-- sticky 保存栏 -->
        <div class="is-save">
          <div v-if="dirty" class="dirty-tip">
            <AppIcon name="info" :size="12" />
            有未保存的更改
          </div>
          <div v-else class="dirty-tip clean">
            <AppIcon name="check" :size="12" />
            已是最新
          </div>
          <div class="save-row">
            <button class="btn ghost sm" :disabled="!dirty" @click="discard">放弃</button>
            <button
              class="btn primary sm"
              :disabled="!dirty || saving || hasBlocking"
              @click="askSave"
            >
              <span v-if="saving" class="spin" />
              保存
            </button>
          </div>
          <p v-if="hasBlocking" class="is-block">
            存在 {{ errors.length }} 个错误，修复后才能保存
          </p>
        </div>
      </aside>
    </div>

    <!-- ══════ 保存确认 ══════ -->
    <AppModal v-model="saveOpen" title="保存 config" :dismissable="false">
      <div class="confirm">
        <div class="cf-icon"><AppIcon name="check" :size="20" /></div>
        <p class="cf-lead">即将写入 <code>~/.ssh/config</code>。</p>
        <ul class="cf-list">
          <li>保存前会自动备份当前文件，可随时回滚</li>
          <li>保存后文件权限会被设为 600（仅属主可读写）</li>
          <li v-if="mode === 'form'">表单模式只重写墨钥托管块，其余内容原样保留</li>
        </ul>
        <div class="field">
          <label>备份备注（可选）</label>
          <input v-model="saveNote" type="text" class="input" placeholder="例如：新增公司 GitLab 配置" />
        </div>
      </div>
      <template #footer>
        <button class="btn ghost" @click="saveOpen = false">取消</button>
        <button class="btn primary" :disabled="saving" @click="doSave">
          <span v-if="saving" class="spin" />
          确认保存
        </button>
      </template>
    </AppModal>

    <!-- ══════ 备份历史抽屉 ══════ -->
    <AppDrawer v-model="backupsOpen" title="config 备份历史">
      <div v-if="!backups.length" class="empty">
        <AppIcon name="clock" :size="22" />
        <p>还没有备份</p>
        <span>每次保存 config 时墨钥都会自动创建一份备份</span>
      </div>
      <ul v-else class="bk-list">
        <li v-for="b in backups" :key="b.name" class="bk-item">
          <span class="bk-ico"><AppIcon name="clock" :size="14" /></span>
          <div class="bk-body">
            <div class="bk-name">{{ b.name }}</div>
            <div class="bk-meta">
              <span>{{ b.createdAt?.slice(0, 19).replace('T', ' ') || '—' }}</span>
              <span class="sep-dot">·</span>
              <span>{{ b.lines }} 行</span>
              <span class="sep-dot">·</span>
              <span>{{ fmtSize(b.size) }}</span>
              <span class="tag sm">{{ reasonLabel(b.reason) }}</span>
            </div>
          </div>
          <div class="bk-actions">
            <button class="btn xs" title="预览" @click="previewBackup(b)">
              <AppIcon name="eye" :size="12" />
            </button>
            <button class="btn xs primary" @click="askRollback(b)">回滚</button>
            <button class="btn xs danger-text" title="删除" @click="deleteBackup(b)">
              <AppIcon name="trash" :size="12" />
            </button>
          </div>
        </li>
      </ul>
    </AppDrawer>

    <!-- 备份预览 -->
    <AppModal v-model="previewOpen" :title="'预览 · ' + previewName" wide>
      <div class="rawbox tall">
        <code>{{ previewText }}</code>
      </div>
    </AppModal>

    <!-- 回滚确认 -->
    <AppModal v-model="rollbackOpen" title="回滚 config" :dismissable="false">
      <div v-if="rollbackTarget" class="confirm">
        <div class="cf-icon warn"><AppIcon name="warning" :size="20" /></div>
        <p class="cf-lead">
          将把 <code>~/.ssh/config</code> 恢复为
          <code>{{ rollbackTarget.name }}</code> 的内容。
        </p>
        <ul class="cf-list">
          <li>当前文件会先被另存为一份 <code>rollback</code> 备份，操作可逆</li>
          <li v-if="dirty">你有未保存的编辑，回滚后这些改动将丢失</li>
        </ul>
      </div>
      <template #footer>
        <button class="btn ghost" @click="rollbackOpen = false">取消</button>
        <button class="btn primary" :disabled="rollingBack" @click="doRollback">
          <span v-if="rollingBack" class="spin" />
          确认回滚
        </button>
      </template>
    </AppModal>
  </div>
</template>

<style scoped>
.cfg {
  display: flex;
  flex-direction: column;
  gap: 16px;
  /* 不要用固定 height:100%：它会把 .cfg-body 压进剩余高度，
     配合 editor 的 overflow:hidden，表单会被拦腰截断。 */
  min-height: 100%;
}

/* 顶部 */
.cfg-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  flex-wrap: wrap;
}
.ch-left {
  display: flex;
  align-items: center;
  gap: 14px;
}
.ch-path {
  display: flex;
  align-items: center;
  gap: 9px;
  color: var(--fg-muted);
}
.ch-path code {
  font-family: var(--font-mono);
  font-size: 12.5px;
  color: var(--fg);
}
.ch-managed {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  color: var(--success-ink);
  background: var(--success-soft);
  padding: 3px 9px;
  border-radius: 20px;
}
.ch-right {
  display: flex;
  align-items: center;
  gap: 10px;
}
.seg.small button {
  padding: 5px 13px;
  font-size: 12px;
}
.badge {
  background: var(--accent);
  color: #fff;
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 20px;
  font-family: var(--font-mono);
  margin-left: 2px;
}

/* 主体三栏 */
.cfg-body {
  display: grid;
  grid-template-columns: 308px minmax(0, 1fr) 336px;
  gap: 16px;
  align-items: start;
  min-height: 0;
}
@media (max-width: 1360px) {
  .cfg-body {
    grid-template-columns: 280px minmax(0, 1fr);
  }
  .issues {
    grid-column: 1 / -1;
    /* 掉到下方时限制高度，保证保存栏在多数视口内直接可见 */
    max-height: 62vh;
  }
}

/* 左窗格 */
.pane {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 220px);
}
.pane-head {
  display: flex;
  gap: 8px;
  padding: 12px;
  border-bottom: 1px solid var(--border);
}
.search.sm {
  flex: 1;
  height: 34px;
  min-width: 0;
  padding: 0 11px;
}
.btn.icon-only {
  width: 34px;
  padding: 0;
  justify-content: center;
  flex: 0 0 auto;
}
.pane-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.pane-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 9px;
  padding: 32px 14px;
  color: var(--fg-faint);
  font-size: 12.5px;
  text-align: center;
}
.host-item {
  display: flex;
  gap: 10px;
  padding: 10px 11px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  cursor: pointer;
  text-align: left;
  transition: all 0.14s;
  width: 100%;
}
.host-item:hover {
  background: var(--surface-warm);
}
.host-item.on {
  background: color-mix(in srgb, var(--accent) 7%, var(--surface));
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
}
.hi-ico {
  width: 26px;
  height: 26px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: var(--surface-warm);
  color: var(--fg-muted);
  flex: 0 0 auto;
}
.host-item.on .hi-ico {
  background: color-mix(in srgb, var(--accent) 12%, var(--surface));
  color: var(--accent);
}
.hi-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.hi-name {
  display: flex;
  align-items: center;
  gap: 6px;
}
.hi-name strong {
  font-size: 12.5px;
  font-family: var(--font-mono);
  color: var(--fg);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dot-err,
.dot-warn {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.dot-err {
  background: var(--danger);
}
.dot-warn {
  background: var(--warn);
}
.hi-sub {
  font-size: 11px;
  color: var(--fg-faint);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.hi-note {
  font-size: 10.5px;
  color: var(--fg-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pane-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 9px 13px;
  border-top: 1px solid var(--border);
  font-size: 11px;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.pf-warn {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--warn);
  font-family: var(--font-sans);
}

/* 编辑器 */
.editor {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  min-width: 0;
}
/* raw 模式没有左侧窗格：窄屏横跨整行；宽屏占前两列，校验清单留在第 3 列 */
.editor.full {
  grid-column: 1 / -1;
}
@media (min-width: 1361px) {
  .editor.full {
    grid-column: 1 / 3;
  }
}
.ed-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 15px 18px;
  border-bottom: 1px solid var(--border);
}
.ed-head h3 {
  font-size: 15px;
  margin: 0 0 3px;
  color: var(--fg);
  font-family: var(--font-mono);
}
.ed-sub {
  font-size: 11.5px;
  color: var(--fg-faint);
}
.ed-actions {
  display: flex;
  align-items: center;
  gap: 7px;
  flex: 0 0 auto;
}
.ed-form {
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 17px;
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
@media (max-width: 1100px) {
  .grid2 {
    grid-template-columns: 1fr;
  }
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
.hint-row code {
  font-family: var(--font-mono);
  background: var(--surface-warm);
  padding: 1px 5px;
  border-radius: 5px;
  color: var(--accent);
}
.danger-text {
  color: var(--danger);
}
.id-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 9px;
}
@media (max-width: 900px) {
  .id-row {
    grid-template-columns: 1fr;
  }
}
.extra-list {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 10px 12px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
}
.extra-row {
  display: flex;
  gap: 10px;
  font-size: 11.5px;
}
.extra-row code {
  font-family: var(--font-mono);
  color: var(--accent);
  min-width: 110px;
}
.extra-row span {
  color: var(--fg-muted);
  font-family: var(--font-mono);
  word-break: break-all;
}

.raw-editor {
  width: 100%;
  min-height: 520px;
  padding: 18px;
  border: none;
  outline: none;
  resize: vertical;
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.75;
  color: var(--fg);
  background: var(--surface);
  tab-size: 4;
}

/* 校验清单 */
.issues {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 220px);
}
.is-head {
  padding: 14px 16px;
  border-bottom: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 9px;
}
.is-title {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.is-counts {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.is-body {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.is-ok {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 30px 16px;
  text-align: center;
}
.is-seal {
  width: 46px;
  height: 46px;
  border-radius: 15px;
  display: grid;
  place-items: center;
  background: var(--success-soft);
  color: var(--success-ink);
}
.is-ok strong {
  font-size: 13.5px;
  color: var(--fg);
}
.is-ok p {
  font-size: 12px;
  color: var(--fg-faint);
  margin: 0;
  line-height: 1.6;
}

.issue {
  display: flex;
  gap: 9px;
  padding: 10px 11px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--surface-warm);
}
.issue.error {
  border-color: color-mix(in srgb, var(--danger) 32%, var(--border));
  background: color-mix(in srgb, var(--danger) 4%, var(--surface));
}
.issue.warn {
  border-color: color-mix(in srgb, var(--warn) 32%, var(--border));
  background: color-mix(in srgb, var(--warn) 4%, var(--surface));
}
.is-icon {
  flex: 0 0 auto;
  margin-top: 1px;
}
.issue.error .is-icon {
  color: var(--danger);
}
.issue.warn .is-icon {
  color: var(--warn);
}
.issue.info .is-icon {
  color: var(--accent);
}
.is-text {
  flex: 1;
  min-width: 0;
}
.is-msg {
  font-size: 12.5px;
  color: var(--fg);
  line-height: 1.55;
  margin-bottom: 3px;
}
.is-hint {
  font-size: 11.5px;
  color: var(--fg-muted);
  line-height: 1.6;
}

.is-save {
  border-top: 1px solid var(--border);
  padding: 13px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--surface);
}
.dirty-tip {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11.5px;
  color: var(--warn);
}
.dirty-tip.clean {
  color: var(--fg-faint);
}
.save-row {
  display: flex;
  gap: 9px;
}
.save-row .btn {
  flex: 1;
}
.is-block {
  font-size: 11px;
  color: var(--danger);
  margin: 0;
  line-height: 1.5;
}

/* 空态 */
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 56px 20px;
  text-align: center;
  color: var(--fg-muted);
}
.empty > :deep(svg) {
  color: var(--fg-faint);
}
.empty p {
  margin: 0;
  font-size: 14px;
  color: var(--fg);
  font-weight: 600;
}
.empty span {
  font-size: 12.5px;
  color: var(--fg-faint);
}
.empty-seal {
  width: 58px;
  height: 58px;
  border-radius: 19px;
  display: grid;
  place-items: center;
  background: var(--surface-warm);
  color: var(--fg-faint);
}
.row {
  display: flex;
  gap: 9px;
  margin-top: 4px;
}
.mono-sm {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg-faint);
}

/* 备份列表 */
.bk-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.bk-item {
  display: flex;
  gap: 11px;
  padding: 12px 13px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  align-items: center;
}
.bk-ico {
  width: 30px;
  height: 30px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--surface-warm);
  color: var(--fg-muted);
  flex: 0 0 auto;
}
.bk-body {
  flex: 1;
  min-width: 0;
}
.bk-name {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-bottom: 4px;
}
.bk-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10.5px;
  color: var(--fg-faint);
  flex-wrap: wrap;
}
.sep-dot {
  color: var(--border);
}
.tag.sm {
  font-size: 10px;
  padding: 1px 6px;
}
.bk-actions {
  display: flex;
  gap: 5px;
  flex: 0 0 auto;
}
.btn.xs {
  height: 26px;
  padding: 0 9px;
  font-size: 11.5px;
  border-radius: 7px;
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
  background: var(--success-soft);
  color: var(--success-ink);
}
.cf-icon.warn {
  background: var(--warn-soft);
  color: var(--warn);
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
.cf-list code {
  font-family: var(--font-mono);
  background: var(--surface-warm);
  padding: 1px 4px;
  border-radius: 4px;
}
.rawbox.tall {
  max-height: 460px;
  overflow: auto;
}
.rawbox.tall code {
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.7;
  font-size: 11.5px;
}
</style>
