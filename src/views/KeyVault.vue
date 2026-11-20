<script setup>
/**
 * 密钥库（PRD 3.1.2）
 *
 * 组成：
 *   - 顶部工具栏：搜索、按算法/口令筛选、分组 chips、生成按钮
 *   - 主表格：文件名 / 算法 / 指纹 / 权限 / 口令 / 创建时间 / 操作
 *   - 生成弹窗：算法、位数、文件名、注释、口令（含强度提示）、写入位置预览
 *   - 详情抽屉（488px）：公钥（可复制）、私钥（二次确认后查看）、部署指引、
 *     加入 agent、改口令、删除
 *   - 轮换提醒横幅：超过阈值天数的密钥提示轮换
 */
import { ref, computed, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import * as api from '../lib/api'
import { useToast } from '../lib/toast'
import { useEnvStore } from '../lib/store'
import AppIcon from '../components/AppIcon.vue'
import AppModal from '../components/AppModal.vue'
import AppDrawer from '../components/AppDrawer.vue'
import AppErrorState from '../components/AppErrorState.vue'

const toast = useToast()
const store = useEnvStore()
const route = useRoute()
const router = useRouter()

/* ══════ 列表 ══════ */
const loading = ref(true)
const keys = ref([])
const loadError = ref(null)

async function load() {
  loading.value = true
  loadError.value = null
  try {
    keys.value = (await api.scanKeys()) || []
  } catch (e) {
    loadError.value = e
    keys.value = []
  } finally {
    loading.value = false
  }
}

/* ══════ 筛选 ══════ */
const q = ref('')
const filterAlgo = ref('all')
const filterFlag = ref('all')

// 顶栏全局搜索把关键词带到 ?q=，消费后清掉 query，避免刷新时重复应用
watch(
  () => route.query.q,
  (v) => {
    if (typeof v === 'string' && v.trim()) {
      q.value = v
      router.replace({ query: {} })
    }
  },
  { immediate: true }
)

const algoOptions = computed(() => {
  const set = new Set(keys.value.map((k) => baseAlgo(k.algorithm)))
  return ['all', ...[...set].sort()]
})

function baseAlgo(label = '') {
  if (label.startsWith('ED25519')) return 'ED25519'
  if (label.startsWith('RSA')) return 'RSA'
  if (label.startsWith('ECDSA')) return 'ECDSA'
  return label.split('-')[0] || '其它'
}

const filtered = computed(() => {
  const kw = q.value.trim().toLowerCase()
  return keys.value.filter((k) => {
    if (kw) {
      const hay = `${k.filename} ${k.algorithm} ${k.comment || ''} ${k.fingerprint || ''}`.toLowerCase()
      if (!hay.includes(kw)) return false
    }
    if (filterAlgo.value !== 'all' && baseAlgo(k.algorithm) !== filterAlgo.value) return false
    if (filterFlag.value === 'passphrase' && !k.hasPassphrase) return false
    if (filterFlag.value === 'nopass' && k.hasPassphrase) return false
    if (filterFlag.value === 'badperm' && k.permCompliant) return false
    return true
  })
})

const stats = computed(() => ({
  total: keys.value.length,
  passphrase: keys.value.filter((k) => k.hasPassphrase).length,
  badPerm: keys.value.filter((k) => !k.permCompliant).length,
  referencing: keys.value.filter((k) => referencedSet.value.has(k.filename)).length,
}))

/* config 中引用的密钥（用于「已在 config 中使用」标记） */
const referencedSet = ref(new Set())

async function loadReferenced() {
  try {
    // 后端直接返回被引用的 IdentityFile 列表，前端不再自己解析路径
    const list = (await api.referencedIdentities()) || []
    const names = list.map((p) => p.split(/[/\\]/).pop()).filter(Boolean)
    referencedSet.value = new Set(names)
  } catch {
    referencedSet.value = new Set()
  }
}

/* 轮换提醒：超过 N 天的密钥 */
const ROTATE_DAYS = 365
const staleKeys = computed(() =>
  keys.value.filter((k) => {
    if (!k.createdAt) return false
    const days = (Date.now() - new Date(k.createdAt).getTime()) / 86400000
    return days > ROTATE_DAYS
  })
)
const bannerDismissed = ref(false)

/* ══════ 生成弹窗 ══════ */
const genOpen = ref(false)
const gen = ref({
  algorithm: 'ed25519',
  bits: 4096,
  filename: '',
  comment: '',
  passphrase: '',
  confirmPass: '',
  showPass: false,
})
const generating = ref(false)
const genError = ref('')

function openGen() {
  const p = store.rawPrefs
  gen.value = {
    algorithm: p.defaultAlgorithm || 'ed25519',
    bits: p.defaultRsaBits || 4096,
    filename: suggestFilename(p.defaultAlgorithm || 'ed25519'),
    comment: p.defaultComment || '',
    passphrase: '',
    confirmPass: '',
    showPass: false,
  }
  genError.value = ''
  genOpen.value = true
}

function suggestFilename(algo) {
  const base = algo === 'rsa' ? 'id_rsa' : 'id_ed25519'
  const existing = new Set(keys.value.map((k) => k.filename))
  if (!existing.has(base)) return base
  for (let i = 2; i < 999; i++) {
    const c = `${base}_${i}`
    if (!existing.has(c)) return c
  }
  return `${base}_new`
}

const filenameIssue = computed(() => {
  const n = gen.value.filename.trim()
  if (!n) return '文件名不能为空'
  if (n.startsWith('.')) return '文件名不能以点开头'
  if (!/^[A-Za-z0-9_.-]+$/.test(n)) return '仅允许字母、数字、下划线、连字符与点'
  if (n.includes('..')) return '不能包含连续的点'
  if (keys.value.some((k) => k.filename === n)) return '同名密钥已存在，请换一个'
  return ''
})

const passIssue = computed(() => {
  const p = gen.value.passphrase
  if (!p) return ''
  if (p.length < 8) return '口令建议至少 8 位'
  if (!/[A-Za-z]/.test(p) || !/\d/.test(p)) return '建议同时包含字母与数字'
  return ''
})

const passStrength = computed(() => {
  const p = gen.value.passphrase
  if (!p) return { level: 0, label: '未设置（不推荐）', tone: 'idle' }
  let score = 0
  if (p.length >= 8) score++
  if (p.length >= 12) score++
  if (/[A-Z]/.test(p) && /[a-z]/.test(p)) score++
  if (/\d/.test(p)) score++
  if (/[^A-Za-z0-9]/.test(p)) score++
  if (score <= 2) return { level: 1, label: '弱', tone: 'danger' }
  if (score <= 3) return { level: 2, label: '中', tone: 'warn' }
  return { level: 3, label: '强', tone: 'ok' }
})

const passMismatch = computed(
  () => gen.value.passphrase && gen.value.confirmPass !== gen.value.passphrase
)

const canGenerate = computed(
  () =>
    !filenameIssue.value &&
    !generating.value &&
    (!gen.value.passphrase || (!passMismatch.value && gen.value.passphrase.length >= 8))
)

async function doGenerate() {
  if (!canGenerate.value) return
  generating.value = true
  genError.value = ''
  try {
    const meta = await api.generateKey({
      algorithm: gen.value.algorithm,
      bits: gen.value.algorithm === 'rsa' ? gen.value.bits : 0,
      filename: gen.value.filename.trim(),
      comment: gen.value.comment.trim(),
      passphrase: gen.value.passphrase,
    })
    toast.push(`已生成 ${meta.filename}`, 'success')
    genOpen.value = false
    await load()
    await store.refreshEnv()
    // 生成后直接打开详情，方便用户立刻复制公钥去部署
    openDetail(meta)
  } catch (e) {
    genError.value = e.message || '生成失败'
  } finally {
    generating.value = false
  }
}

/* ══════ 详情抽屉 ══════ */
const detailOpen = ref(false)
const current = ref(null)
const publicKey = ref('')
const loadingPub = ref(false)
const privateKey = ref('')
const showPrivate = ref(false)
const privateConfirmOpen = ref(false)
const privateAck = ref(false)
const tab = ref('public')

async function openDetail(k) {
  current.value = k
  detailOpen.value = true
  tab.value = 'public'
  publicKey.value = ''
  privateKey.value = ''
  showPrivate.value = false
  loadingPub.value = true
  try {
    publicKey.value = await api.readPublicKey(k.filename)
  } catch (e) {
    publicKey.value = ''
    toast.push(e.message || '读取公钥失败', 'warn')
  } finally {
    loadingPub.value = false
  }
}

async function loadPrivate() {
  if (!current.value) return
  try {
    privateKey.value = await api.readPrivateKey(current.value.filename)
    showPrivate.value = true
    privateConfirmOpen.value = false
    privateAck.value = false
    toast.push('私钥已显示，请勿截图或外传；关闭抽屉后即被清除', 'warn')
  } catch (e) {
    toast.push(e.message || '读取私钥失败', 'danger')
  }
}

watch(detailOpen, (v) => {
  if (!v) {
    // 关闭抽屉立即丢弃私钥明文
    privateKey.value = ''
    showPrivate.value = false
  }
})

/* 部署指引 */
const deployTab = ref('github')
const deployTargets = [
  { key: 'github', label: 'GitHub' },
  { key: 'gitlab', label: 'GitLab' },
  { key: 'server', label: 'Linux 服务器' },
  { key: 'nas', label: 'NAS / 群晖' },
]

const deployText = computed(() => {
  const pub = publicKey.value || '<你的公钥>'
  switch (deployTab.value) {
    case 'github':
      return {
        where: 'GitHub → Settings → SSH and GPG keys → New SSH key',
        steps: [
          '复制下方公钥全文',
          '打开 GitHub 设置页，粘贴到 Key 输入框',
          'Title 建议填「墨钥 · ' + (current.value?.filename || '') + '」便于识别',
          '保存后用「诊断」页测试 ssh -T git@github.com',
        ],
      }
    case 'gitlab':
      return {
        where: 'GitLab → Preferences → SSH Keys',
        steps: [
          '复制下方公钥全文',
          '打开 GitLab 用户设置 → SSH Keys',
          '粘贴公钥，Expiration 建议设置为 1 年后',
          '保存后用「诊断」页测试连接',
        ],
      }
    case 'server':
      return {
        where: '服务器的 ~/.ssh/authorized_keys',
        steps: [
          '复制下方公钥全文',
          '在服务器上执行（把公钥粘贴进去）：',
          'mkdir -p ~/.ssh && chmod 700 ~/.ssh',
          'echo "' + pub + '" >> ~/.ssh/authorized_keys',
          'chmod 600 ~/.ssh/authorized_keys',
        ],
      }
    default:
      return {
        where: '群晖 DSM → 控制面板 → 终端机与 SNMP → SSH',
        steps: [
          '复制下方公钥全文',
          'DSM → 控制面板 → 用户 → 高级 → 用户密钥 → 新增',
          '粘贴公钥并保存',
          '确认已启用 SSH 服务，默认端口 22',
        ],
      }
  }
})

async function copyText(text, label) {
  try {
    await navigator.clipboard.writeText(text)
    toast.push(label || '已复制', 'success')
    // 复制公钥是分发行为，留痕（不记录内容）
    if (label?.includes('公钥')) {
      api
        .appendAudit('key', 'key.copyPublic', `复制公钥 ${current.value?.filename || ''}`, 'ok', current.value?.filename || '')
        .catch(() => {})
    }
  } catch {
    toast.push('复制失败，请手动选中复制', 'danger')
  }
}

/* agent 操作 */
const agentBusy = ref(false)

async function addToAgent(k) {
  agentBusy.value = true
  try {
    await api.agentAddKey(k.filename, 0)
    toast.push(`已把 ${k.filename} 加入 ssh-agent`, 'success')
    await store.refreshAgent()
  } catch (e) {
    toast.push(e.message || '加入 agent 失败', 'danger')
  } finally {
    agentBusy.value = false
  }
}

async function removeFromAgent(k) {
  try {
    await api.agentRemoveKey(k.filename)
    toast.push(`已从 agent 移除 ${k.filename}`, 'success')
    await store.refreshAgent()
  } catch (e) {
    toast.push(e.message || '移除失败', 'danger')
  }
}

/* 权限修复 */
async function fixPerm(k) {
  try {
    await api.fixPathPermission(k.path, false)
    toast.push(`${k.filename} 权限已修复为 600`, 'success')
    await load()
  } catch (e) {
    toast.push(e.message || '修复失败', 'danger')
  }
}

/* 删除 */
const delOpen = ref(false)
const delTarget = ref(null)
const delTyped = ref('')

function askDelete(k) {
  delTarget.value = k
  delTyped.value = ''
  delOpen.value = true
}

const delBlocking = computed(() => {
  if (!delTarget.value) return []
  const issues = []
  if (referencedSet.value.has(delTarget.value.filename)) {
    issues.push('该密钥正被 ~/.ssh/config 引用，删除后相关 Host 将无法连接')
  }
  return issues
})

async function doDelete() {
  if (!delTarget.value) return
  if (store.rawPrefs.confirmDeleteTyped && delTyped.value !== delTarget.value.filename) return
  try {
    const removed = await api.deleteKey(delTarget.value.filename)
    toast.push(`已删除 ${removed.join('、')}`, 'success')
    delOpen.value = false
    detailOpen.value = false
    await load()
    await loadReferenced()
    await store.refreshEnv()
  } catch (e) {
    toast.push(e.message || '删除失败', 'danger')
  }
}

/* 改口令 */
const passOpen = ref(false)
const passForm = ref({ old: '', next: '', confirm: '', show: false })
const passBusy = ref(false)

function askPassphrase(k) {
  current.value = k
  passForm.value = { old: '', next: '', confirm: '', show: false }
  passOpen.value = true
}

async function doPassphrase() {
  if (passForm.value.next !== passForm.value.confirm) return
  passBusy.value = true
  try {
    await api.setKeyPassphrase(current.value.filename, passForm.value.old, passForm.value.next)
    toast.push('口令已更新', 'success')
    passOpen.value = false
    await load()
  } catch (e) {
    toast.push(e.message || '更新口令失败', 'danger')
  } finally {
    passBusy.value = false
  }
}

/* 初始化 */
onMounted(async () => {
  await Promise.allSettled([load(), loadReferenced()])
})

/** 指纹缩略展示 */
function shortFp(fp) {
  if (!fp) return '—'
  const m = fp.match(/SHA256:[A-Za-z0-9+/=]+/)
  const s = m ? m[0] : fp
  return s.length > 24 ? `${s.slice(0, 20)}…` : s
}

/** 字节可读化 */
function formatSize(n) {
  if (!n) return '—'
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(2)} MB`
}

defineExpose({ load })
</script>

<template>
  <div class="vault">
    <div class="page-head">
      <div>
        <h1>密钥库</h1>
        <p class="sub">本机 ~/.ssh 中的密钥。生成、复制、部署与轮换都在这里完成，私钥不会离开该目录。</p>
      </div>
    </div>

    <!-- 轮换提醒 -->
    <div v-if="staleKeys.length && !bannerDismissed" class="banner warn rotate">
      <AppIcon name="clock" :size="16" />
      <div>
        <strong>有 {{ staleKeys.length }} 把密钥使用已超过 {{ ROTATE_DAYS }} 天</strong>
        <p>
          长期不更换的密钥一旦泄露，风险敞口会持续扩大。建议生成新密钥并完成部署后再下线旧的。
        </p>
      </div>
      <div class="banner-actions">
        <button class="btn sm" @click="q = staleKeys[0].filename">查看</button>
        <button class="icon-btn" @click="bannerDismissed = true">
          <AppIcon name="close" :size="14" />
        </button>
      </div>
    </div>

    <!-- 工具栏 -->
    <div class="toolbar">
      <div class="tb-left">
        <div class="search">
          <AppIcon name="search" :size="15" />
          <input v-model="q" type="text" placeholder="搜索文件名、注释或指纹…" />
          <button v-if="q" class="clear" @click="q = ''">
            <AppIcon name="close" :size="12" />
          </button>
        </div>
        <select v-model="filterAlgo" class="select sm">
          <option value="all">全部算法</option>
          <option v-for="a in algoOptions.filter((x) => x !== 'all')" :key="a" :value="a">
            {{ a }}
          </option>
        </select>
        <select v-model="filterFlag" class="select sm">
          <option value="all">全部状态</option>
          <option value="passphrase">已设口令</option>
          <option value="nopass">无口令</option>
          <option value="badperm">权限异常</option>
        </select>
      </div>
      <div class="tb-right">
        <button class="btn ghost sm" :disabled="loading" @click="load">
          <AppIcon name="refresh" :size="14" />
          刷新
        </button>
        <button class="btn primary" :disabled="!!loadError" @click="openGen">
          <AppIcon name="plus" :size="15" />
          生成密钥
        </button>
      </div>
    </div>

    <!-- 统计条：读取失败时显示未知，而不是把空数组说成 0 -->
    <div class="stat-row">
      <span class="stat"><b>{{ loadError ? '—' : stats.total }}</b> 把密钥</span>
      <span class="sep" />
      <span class="stat" :class="{ hot: !loadError && stats.passphrase > 0 }">
        <b>{{ loadError ? '—' : stats.passphrase }}</b> 已设口令
      </span>
      <span class="sep" />
      <span class="stat" :class="{ bad: !loadError && stats.badPerm > 0 }">
        <b>{{ loadError ? '—' : stats.badPerm }}</b> 权限异常
      </span>
      <span class="sep" />
      <span class="stat"><b>{{ loadError ? '—' : stats.referencing }}</b> 已在 config 引用</span>
    </div>

    <!-- 主内容 -->
    <div class="card table-card">
      <div v-if="loading" class="empty">
        <span class="spin lg" />
        <p>正在读取 ~/.ssh …</p>
      </div>

      <AppErrorState
        v-else-if="loadError"
        :title="'无法读取 ~/.ssh'"
        :detail="loadError.message"
        @retry="load"
      />

      <div v-else-if="keys.length === 0" class="empty">
        <span class="empty-seal"><AppIcon name="key" :size="26" /></span>
        <p>还没有任何密钥</p>
        <span>生成一把新密钥，或确认 ~/.ssh 中已有密钥文件</span>
        <button class="btn primary sm" @click="openGen">
          <AppIcon name="plus" :size="14" />
          生成第一把密钥
        </button>
      </div>

      <div v-else-if="filtered.length === 0" class="empty">
        <AppIcon name="search" :size="24" />
        <p>没有匹配的密钥</p>
        <span>换个关键词或清空筛选条件试试</span>
        <button class="btn sm" @click="((q = ''), (filterAlgo = 'all'), (filterFlag = 'all'))">
          清空筛选
        </button>
      </div>

      <div v-else class="table-scroll">
      <table class="ds-table key-table">
        <thead>
          <tr>
            <th>文件名</th>
            <th>算法</th>
            <th>指纹</th>
            <th>权限</th>
            <th>口令</th>
            <th class="ta-r">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="k in filtered"
            :key="k.filename"
            class="row"
            tabindex="0"
            role="button"
            :aria-label="`查看密钥 ${k.filename} 详情`"
            @click="openDetail(k)"
            @keydown.enter="openDetail(k)"
            @keydown.space.prevent="openDetail(k)"
          >
            <td>
              <div class="cell-name">
                <span class="k-ico"><AppIcon name="key" :size="14" /></span>
                <div class="k-txt">
                  <strong>{{ k.filename }}</strong>
                  <span v-if="k.comment || k.createdAt" class="cmt">
                    {{ [k.comment, (k.createdAt || '').slice(0, 10)].filter(Boolean).join(' · ') }}
                  </span>
                </div>
                <span v-if="referencedSet.has(k.filename)" class="tag ref" title="已被 ~/.ssh/config 引用">
                  已引用
                </span>
              </div>
            </td>
            <td><span class="tag">{{ k.algorithm }}</span></td>
            <td>
              <code class="fp" :title="k.fingerprint || ''">{{ shortFp(k.fingerprint) }}</code>
            </td>
            <td>
              <span class="pill" :class="k.permCompliant ? 'ok' : 'warn'">
                {{ k.permDisplay || '—' }}
              </span>
            </td>
            <td>
              <span class="pill" :class="k.hasPassphrase ? 'ok' : 'idle'">
                <AppIcon :name="k.hasPassphrase ? 'lock' : 'unlock'" :size="11" />
                {{ k.hasPassphrase ? '已保护' : '无' }}
              </span>
            </td>
            <td class="ta-r" @click.stop>
              <div class="row-actions">
                <button
                  v-if="!k.permCompliant"
                  class="btn xs warn"
                  title="修复为 600"
                  @click="fixPerm(k)"
                >
                  修复权限
                </button>
                <button class="btn xs" title="加入 ssh-agent" aria-label="加入 ssh-agent" :disabled="agentBusy" @click="addToAgent(k)">
                  <AppIcon name="shield" :size="12" />
                  agent
                </button>
                <button class="btn xs" title="查看详情" aria-label="查看详情" @click="openDetail(k)">
                  <AppIcon name="eye" :size="12" />
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
      </div>
    </div>

    <!-- ══════ 生成密钥弹窗 ══════ -->
    <AppModal v-model="genOpen" title="生成 SSH 密钥" wide>
      <div class="form">
        <div v-if="genError" class="banner danger sm">
          <AppIcon name="warning" :size="14" />
          <span>{{ genError }}</span>
        </div>

        <div class="field">
          <label>算法</label>
          <div class="algo-cards">
            <button
              v-for="a in [
                { id: 'ed25519', t: 'ED25519', d: '现代首选，密钥短、速度快、安全性高' },
                { id: 'rsa', t: 'RSA', d: '兼容性最好，适合老旧服务器与设备' },
              ]"
              :key="a.id"
              class="algo-card"
              :class="{ on: gen.algorithm === a.id }"
              @click="((gen.algorithm = a.id), (gen.filename = suggestFilename(a.id)))"
            >
              <span class="ac-check">
                <AppIcon v-if="gen.algorithm === a.id" name="check" :size="12" />
              </span>
              <strong>{{ a.t }}</strong>
              <p>{{ a.d }}</p>
            </button>
          </div>
        </div>

        <div v-if="gen.algorithm === 'rsa'" class="field">
          <label>密钥长度</label>
          <div class="seg">
            <button
              v-for="b in [2048, 3072, 4096]"
              :key="b"
              :class="{ on: gen.bits === b }"
              @click="gen.bits = b"
            >
              {{ b }}
              <span v-if="b === 4096" class="rec">推荐</span>
            </button>
          </div>
          <p class="hint-row">位数越高越安全，但握手开销也越大。4096 是当前通用推荐值。</p>
        </div>

        <div class="field">
          <label>文件名 <span class="req">*</span></label>
          <input v-model="gen.filename" type="text" class="input mono" placeholder="id_ed25519" />
          <p v-if="filenameIssue" class="hint-row danger-text">{{ filenameIssue }}</p>
          <p v-else class="hint-row">
            将生成
            <code>{{ '~/.ssh/' + gen.filename }}</code>
            与
            <code>{{ '~/.ssh/' + gen.filename + '.pub' }}</code>
          </p>
        </div>

        <div class="field">
          <label>注释（通常是邮箱或用途）</label>
          <input v-model="gen.comment" type="text" class="input" placeholder="leeli@example.com" />
          <p class="hint-row">注释会附加在公钥末尾，用于在服务器上区分多把密钥。</p>
        </div>

        <div class="field">
          <label>口令保护（可选，但强烈建议）</label>
          <div class="pass-wrap">
            <input
              v-model="gen.passphrase"
              :type="gen.showPass ? 'text' : 'password'"
              class="input"
              placeholder="留空表示不加密私钥"
              autocomplete="new-password"
            />
            <button class="eye" @click="gen.showPass = !gen.showPass">
              <AppIcon :name="gen.showPass ? 'eyeOff' : 'eye'" :size="14" />
            </button>
          </div>
          <div v-if="gen.passphrase" class="strength">
            <div class="bars">
              <span v-for="i in 3" :key="i" :class="[passStrength.tone, { on: i <= passStrength.level }]" />
            </div>
            <span class="sl" :class="passStrength.tone">强度：{{ passStrength.label }}</span>
          </div>
          <p v-if="passIssue" class="hint-row warn-text">{{ passIssue }}</p>

          <template v-if="gen.passphrase">
            <input
              v-model="gen.confirmPass"
              :type="gen.showPass ? 'text' : 'password'"
              class="input mt"
              placeholder="再次输入口令"
              autocomplete="new-password"
            />
            <p v-if="passMismatch" class="hint-row danger-text">两次输入的口令不一致</p>
          </template>

          <div v-if="gen.passphrase" class="tipbox">
            <AppIcon name="info" :size="13" />
            <span>
              设置口令后，非交互场景（如 git push）需要把密钥加入 ssh-agent。
              生成完成后墨钥会引导你完成这一步。
            </span>
          </div>
        </div>
      </div>

      <template #footer>
        <button class="btn ghost" @click="genOpen = false">取消</button>
        <button class="btn primary" :disabled="!canGenerate" @click="doGenerate">
          <span v-if="generating" class="spin" />
          {{ generating ? '生成中…' : '生成密钥' }}
        </button>
      </template>
    </AppModal>

    <!-- ══════ 详情抽屉 ══════ -->
    <AppDrawer v-model="detailOpen" :title="current?.filename || '密钥详情'">
      <template v-if="current">
        <div class="d-hero">
          <span class="d-seal"><AppIcon name="key" :size="22" /></span>
          <div>
            <strong>{{ current.filename }}</strong>
            <div class="d-tags">
              <span class="tag">{{ current.algorithm }}</span>
              <span class="pill" :class="current.permCompliant ? 'ok' : 'warn'">
                {{ current.permDisplay || '—' }}
              </span>
              <span class="pill" :class="current.hasPassphrase ? 'ok' : 'idle'">
                {{ current.hasPassphrase ? '口令保护' : '无口令' }}
              </span>
            </div>
          </div>
        </div>

        <div class="d-meta">
          <div class="m-item">
            <span>指纹</span>
            <code>{{ current.fingerprint || '不可用' }}</code>
          </div>
          <div class="m-item">
            <span>路径</span>
            <code>{{ current.prettyPath }}</code>
          </div>
          <div class="m-item">
            <span>注释</span>
            <code>{{ current.comment || '—' }}</code>
          </div>
          <div class="m-item">
            <span>创建时间</span>
            <code>{{ (current.createdAt || '').slice(0, 19).replace('T', ' ') || '—' }}</code>
          </div>
          <div class="m-item">
            <span>文件大小</span>
            <code>{{ formatSize(current.size) }}</code>
          </div>
          <div class="m-item">
            <span>config 引用</span>
            <code>{{ referencedSet.has(current.filename) ? '是' : '否' }}</code>
          </div>
        </div>

        <div v-if="!current.permCompliant" class="banner warn sm">
          <AppIcon name="warning" :size="14" />
          <div>
            <span>该私钥权限过宽，OpenSSH 可能拒绝加载。</span>
            <button class="btn xs warn" @click="fixPerm(current)">立即修复为 600</button>
          </div>
        </div>

        <!-- 公钥 / 私钥 tabs -->
        <div class="segtabs">
          <button :class="{ on: tab === 'public' }" @click="tab = 'public'">
            <AppIcon name="unlock" :size="13" /> 公钥
          </button>
          <button :class="{ on: tab === 'private' }" @click="tab = 'private'">
            <AppIcon name="lock" :size="13" /> 私钥
          </button>
          <button :class="{ on: tab === 'deploy' }" @click="tab = 'deploy'">
            <AppIcon name="server" :size="13" /> 部署
          </button>
        </div>

        <!-- 公钥 -->
        <div v-if="tab === 'public'" class="tab-pane">
          <div v-if="loadingPub" class="mini-empty"><span class="spin" /> 读取中…</div>
          <template v-else-if="publicKey">
            <div class="rawbox tall">
              <code>{{ publicKey }}</code>
            </div>
            <div class="d-actions">
              <button class="btn primary sm" @click="copyText(publicKey, '公钥已复制到剪贴板')">
                <AppIcon name="copy" :size="13" /> 复制公钥
              </button>
            </div>
            <p class="fine-note">
              公钥可以安全地分发给任何服务器，它无法反推出私钥。
            </p>
          </template>
          <div v-else class="mini-empty">
            <AppIcon name="warning" :size="18" />
            <span>无法读取公钥。若私钥带口令，ssh-keygen 需要口令才能导出公钥。</span>
          </div>
        </div>

        <!-- 私钥 -->
        <div v-else-if="tab === 'private'" class="tab-pane">
          <div v-if="!showPrivate" class="lock-wall">
            <span class="lw-seal"><AppIcon name="lock" :size="22" /></span>
            <strong>私钥是最高敏感信息</strong>
            <p>
              查看后请勿截图、粘贴到聊天工具或上传到任何在线服务。
              墨钥不会保存私钥副本，本次读取会在审计日志中留痕。
            </p>
            <button class="btn danger" @click="((privateConfirmOpen = true), (privateAck = false))">
              <AppIcon name="eye" :size="14" /> 我了解风险，查看私钥
            </button>
          </div>
          <template v-else>
            <div class="rawbox tall danger-box">
              <code>{{ privateKey }}</code>
            </div>
            <div class="d-actions">
              <button class="btn sm" @click="copyText(privateKey, '私钥已复制（请谨慎保管）')">
                <AppIcon name="copy" :size="13" /> 复制私钥
              </button>
              <button class="btn ghost sm" @click="((showPrivate = false), (privateKey = ''))">
                隐藏并清除
              </button>
            </div>
          </template>
        </div>

        <!-- 部署指引 -->
        <div v-else class="tab-pane">
          <div class="deploy-tabs">
            <button
              v-for="t in deployTargets"
              :key="t.key"
              :class="{ on: deployTab === t.key }"
              @click="deployTab = t.key"
            >
              {{ t.label }}
            </button>
          </div>
          <div class="deploy">
            <div class="deploy-where">
              <AppIcon name="folder" :size="13" />
              <span>{{ deployText.where }}</span>
            </div>
            <ol class="deploy-steps">
              <li v-for="(s, i) in deployText.steps" :key="i">
                <template v-if="s.startsWith('echo ') || s.startsWith('mkdir') || s.startsWith('chmod')">
                  <span class="deploy-text">在服务器终端执行：</span>
                  <div class="rawbox">
                    <code>{{ s }}</code>
                    <button class="copy-mini" @click="copyText(s, '命令已复制')">
                      <AppIcon name="copy" :size="12" />
                    </button>
                  </div>
                </template>
                <span v-else>{{ s }}</span>
              </li>
            </ol>
            <button class="btn primary sm" @click="copyText(publicKey, '公钥已复制，可粘贴到目标平台')">
              <AppIcon name="copy" :size="13" /> 复制公钥去部署
            </button>
          </div>
        </div>

        <!-- 危险操作区 -->
        <div class="d-danger">
          <div class="dd-title">管理操作</div>
          <div class="dd-row">
            <button
              v-if="!current.hasPassphrase"
              class="btn sm"
              @click="askPassphrase(current)"
            >
              <AppIcon name="lock" :size="13" /> 设置口令保护
            </button>
            <button class="btn sm" :disabled="agentBusy" @click="addToAgent(current)">
              <AppIcon name="shield" :size="13" /> 加入 ssh-agent
            </button>
            <button class="btn sm" @click="removeFromAgent(current)">
              <AppIcon name="minus" :size="13" /> 从 agent 移除
            </button>
            <button class="btn danger sm" @click="askDelete(current)">
              <AppIcon name="trash" :size="13" /> 删除密钥
            </button>
          </div>
        </div>
      </template>
    </AppDrawer>

    <!-- 私钥二次确认 -->
    <AppModal v-model="privateConfirmOpen" title="确认查看私钥明文" :dismissable="false">
      <div class="confirm">
        <div class="cf-icon danger"><AppIcon name="warning" :size="22" /></div>
        <p class="cf-lead">
          即将读取并显示
          <code>{{ current?.prettyPath }}</code>
          的完整私钥内容。
        </p>
        <ul class="cf-list">
          <li>请确认屏幕没有被录屏或旁人观看</li>
          <li>不要将私钥粘贴到聊天软件、网盘或任何在线服务</li>
          <li>该操作会记录到审计日志（仅记录「查看了哪个密钥」，不记录内容）</li>
        </ul>
        <label class="checkbox">
          <input v-model="privateAck" type="checkbox" />
          <span>我已了解上述风险</span>
        </label>
      </div>
      <template #footer>
        <button class="btn ghost" @click="privateConfirmOpen = false">取消</button>
        <button class="btn danger" :disabled="!privateAck" @click="loadPrivate">
          确认查看
        </button>
      </template>
    </AppModal>

    <!-- 删除确认 -->
    <AppModal v-model="delOpen" title="删除密钥" :dismissable="false">
      <div v-if="delTarget" class="confirm">
        <div class="cf-icon danger"><AppIcon name="trash" :size="20" /></div>
        <p class="cf-lead">
          将永久删除 <code>{{ delTarget.filename }}</code>
          <template v-if="delTarget.hasPublic"> 及其公钥 <code>{{ delTarget.filename }}.pub</code></template>。
        </p>
        <div v-if="delBlocking.length" class="banner danger sm">
          <AppIcon name="warning" :size="14" />
          <div>
            <div v-for="(b, i) in delBlocking" :key="i">{{ b }}</div>
          </div>
        </div>
        <p class="cf-note">
          如果该私钥已在服务器上注册，删除本地文件不会自动撤销服务器授权，
          请记得同时从远程 <code>authorized_keys</code> 中移除对应公钥。
        </p>
        <div v-if="store.rawPrefs.confirmDeleteTyped" class="field">
          <label>请输入文件名以确认</label>
          <input v-model="delTyped" type="text" class="input mono" :placeholder="delTarget.filename" />
        </div>
      </div>
      <template #footer>
        <button class="btn ghost" @click="delOpen = false">取消</button>
        <button
          class="btn danger"
          :disabled="store.rawPrefs.confirmDeleteTyped && delTyped !== delTarget?.filename"
          @click="doDelete"
        >
          确认删除
        </button>
      </template>
    </AppModal>

    <!-- 设置口令 -->
    <AppModal v-model="passOpen" title="设置口令保护">
      <div class="form">
        <p class="hint-row">
          为 <code>{{ current?.filename }}</code> 设置口令后，每次使用该私钥都需要验证。
        </p>
        <div class="field">
          <label>原口令（无口令则留空）</label>
          <input v-model="passForm.old" type="password" class="input" autocomplete="off" />
        </div>
        <div class="field">
          <label>新口令 <span class="req">*</span></label>
          <input v-model="passForm.next" type="password" class="input" autocomplete="new-password" />
        </div>
        <div class="field">
          <label>确认新口令 <span class="req">*</span></label>
          <input
            v-model="passForm.confirm"
            type="password"
            class="input"
            autocomplete="new-password"
          />
          <p v-if="passForm.confirm && passForm.next !== passForm.confirm" class="hint-row danger-text">
            两次输入不一致
          </p>
        </div>
      </div>
      <template #footer>
        <button class="btn ghost" @click="passOpen = false">取消</button>
        <button
          class="btn primary"
          :disabled="passBusy || !passForm.next || passForm.next !== passForm.confirm"
          @click="doPassphrase"
        >
          <span v-if="passBusy" class="spin" />
          保存
        </button>
      </template>
    </AppModal>
  </div>
</template>

<style scoped>
.vault {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.rotate {
  align-items: flex-start;
}
.banner-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
  flex: 0 0 auto;
}

/* 工具栏 */
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  flex-wrap: wrap;
}
.tb-left,
.tb-right {
  display: flex;
  align-items: center;
  gap: 10px;
}
.search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  height: 36px;
  min-width: 280px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--fg-faint);
  transition: border-color 0.16s;
}
.search:focus-within {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}
.search input {
  flex: 1;
  border: none;
  background: transparent;
  outline: none;
  font-size: 13px;
  color: var(--fg);
  font-family: inherit;
}
.search .clear {
  border: none;
  background: transparent;
  color: var(--fg-faint);
  cursor: pointer;
  display: grid;
  place-items: center;
}
.select.sm {
  height: 36px;
  width: auto;
  min-width: 118px;
}

/* 统计条 */
.stat-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 0 4px;
  font-size: 12.5px;
  color: var(--fg-muted);
}
.stat b {
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--fg);
  margin-right: 3px;
}
.stat.hot b {
  color: var(--accent);
}
.stat.bad b {
  color: var(--warn);
}
.sep {
  width: 1px;
  height: 12px;
  background: var(--border);
}

.table-card {
  padding: 0;
  overflow: hidden;
}
.key-table {
  width: 100%;
  /* 7 列内容在窄窗口下保持可读，交给 .table-scroll 横向滚动，而不是压缩换行。
     table-layout:fixed 用来锁住除文件名外的列宽，否则 auto 布局会把
     第一列撑到 1000px+，把其余列全部推出去。 */
  min-width: 950px;
  table-layout: fixed;
}
.key-table th:nth-child(2) { width: 92px; }
.key-table th:nth-child(3) { width: 190px; }
.key-table th:nth-child(4) { width: 80px; }
.key-table th:nth-child(5) { width: 96px; }
.key-table th:nth-child(6) { width: 150px; }

.key-table .row:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.key-table .row {
  cursor: pointer;
}
.cell-name {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  overflow: hidden;
}
.k-ico {
  width: 26px;
  height: 26px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--accent) 10%, var(--surface));
  color: var(--accent);
  flex: 0 0 auto;
}
.k-txt {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1 1 auto;
  overflow: hidden;
}
.k-txt strong {
  font-size: 13px;
  color: var(--fg);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cmt {
  font-size: 11px;
  color: var(--fg-faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 280px;
}
.tag.ref {
  background: color-mix(in srgb, var(--accent) 12%, var(--surface));
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 30%, var(--border));
}
.fp {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg-muted);
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
.btn.xs {
  height: 26px;
  padding: 0 9px;
  font-size: 11.5px;
  border-radius: 7px;
  gap: 4px;
}

/* 空态 */
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 60px 20px;
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
  width: 60px;
  height: 60px;
  border-radius: 20px;
  display: grid;
  place-items: center;
  background: var(--surface-warm);
  color: var(--fg-faint);
}
.mini-empty {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 26px 4px;
  font-size: 12.5px;
  color: var(--fg-faint);
  justify-content: center;
  text-align: center;
}

/* 表单 */
.form {
  display: flex;
  flex-direction: column;
  gap: 18px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 7px;
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
.warn-text {
  color: var(--warn);
}
.mt {
  margin-top: 8px;
}

.algo-cards {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.algo-card {
  position: relative;
  text-align: left;
  padding: 14px 15px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.16s;
}
.algo-card:hover {
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
}
.algo-card.on {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 5%, var(--surface));
  box-shadow: var(--focus-ring);
}
.algo-card strong {
  font-size: 13.5px;
  color: var(--fg);
  font-family: var(--font-mono);
  display: block;
  margin-bottom: 5px;
}
.algo-card p {
  font-size: 11.5px;
  color: var(--fg-muted);
  margin: 0;
  line-height: 1.55;
}
.ac-check {
  position: absolute;
  top: 11px;
  right: 11px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 1px solid var(--border);
  display: grid;
  place-items: center;
  background: var(--surface);
  color: #fff;
}
.algo-card.on .ac-check {
  background: var(--accent);
  border-color: var(--accent);
}

.seg {
  display: inline-flex;
  padding: 3px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
  gap: 2px;
}
.seg button {
  position: relative;
  border: none;
  background: transparent;
  padding: 7px 18px;
  border-radius: 7px;
  font-size: 12.5px;
  font-family: var(--font-mono);
  color: var(--fg-muted);
  cursor: pointer;
  transition: all 0.16s;
}
.seg button.on {
  background: var(--surface);
  color: var(--fg);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.07);
  font-weight: 600;
}
.rec {
  position: absolute;
  top: -8px;
  right: -4px;
  font-size: 9px;
  background: var(--accent);
  color: #fff;
  padding: 1px 5px;
  border-radius: 20px;
  font-family: var(--font-sans);
  font-weight: 500;
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
.eye:hover {
  color: var(--fg);
}

.strength {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 2px;
}
.bars {
  display: flex;
  gap: 4px;
  flex: 1;
}
.bars span {
  height: 4px;
  flex: 1;
  border-radius: 3px;
  background: var(--border);
  transition: background 0.2s;
}
.bars span.on.danger {
  background: var(--danger);
}
.bars span.on.warn {
  background: var(--warn);
}
.bars span.on.ok {
  background: var(--success);
}
.sl {
  font-size: 11.5px;
  font-family: var(--font-mono);
}
.sl.danger {
  color: var(--danger);
}
.sl.warn {
  color: var(--warn);
}
.sl.ok {
  color: var(--success);
}

.tipbox {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 10px 12px;
  background: color-mix(in srgb, var(--accent) 6%, var(--surface));
  border: 1px solid color-mix(in srgb, var(--accent) 22%, var(--border));
  border-radius: var(--radius-sm);
  font-size: 11.5px;
  color: var(--fg-muted);
  line-height: 1.6;
  margin-top: 6px;
}
.tipbox :deep(svg) {
  color: var(--accent);
  flex: 0 0 auto;
  margin-top: 2px;
}

/* 抽屉 */
.d-hero {
  display: flex;
  gap: 13px;
  align-items: center;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--border);
}
.d-seal {
  width: 44px;
  height: 44px;
  border-radius: 13px;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--accent) 11%, var(--surface));
  color: var(--accent);
  flex: 0 0 auto;
}
.d-hero strong {
  font-size: 14.5px;
  font-family: var(--font-mono);
  color: var(--fg);
  display: block;
  margin-bottom: 6px;
}
.d-tags {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.d-meta {
  display: flex;
  flex-direction: column;
  gap: 9px;
  padding: 16px 0;
  border-bottom: 1px solid var(--border);
}
.m-item {
  display: flex;
  align-items: baseline;
  gap: 12px;
  font-size: 12px;
}
.m-item > span {
  width: 72px;
  color: var(--fg-faint);
  flex: 0 0 auto;
}
.m-item code {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg);
  word-break: break-all;
}

.segtabs {
  display: flex;
  gap: 2px;
  padding: 3px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
  margin: 16px 0 14px;
}
.segtabs button {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: none;
  background: transparent;
  padding: 8px;
  border-radius: 7px;
  font-size: 12.5px;
  color: var(--fg-muted);
  cursor: pointer;
}
.segtabs button.on {
  background: var(--surface);
  color: var(--fg);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.07);
}

.tab-pane {
  padding-bottom: 4px;
}
.rawbox.tall {
  max-height: 200px;
  overflow: auto;
}
.rawbox.tall code {
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.6;
  font-size: 11px;
}
.danger-box {
  border-color: color-mix(in srgb, var(--danger) 30%, var(--border));
  background: color-mix(in srgb, var(--danger) 4%, var(--surface));
}
.d-actions {
  display: flex;
  gap: 8px;
  margin-top: 10px;
}
.fine-note {
  font-size: 11.5px;
  color: var(--fg-faint);
  margin: 10px 0 0;
  line-height: 1.6;
}

.lock-wall {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 11px;
  padding: 26px 18px;
  background: var(--surface-warm);
  border-radius: var(--radius-md);
  border: 1px dashed var(--border);
}
.lw-seal {
  width: 52px;
  height: 52px;
  border-radius: 17px;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--danger) 10%, var(--surface));
  color: var(--danger);
}
.lock-wall strong {
  font-size: 14px;
  color: var(--fg);
}
.lock-wall p {
  font-size: 12px;
  color: var(--fg-muted);
  line-height: 1.7;
  margin: 0;
  max-width: 320px;
}

.deploy-tabs {
  display: flex;
  gap: 6px;
  margin-bottom: 14px;
  flex-wrap: wrap;
}
.deploy-tabs button {
  border: 1px solid var(--border);
  background: var(--surface);
  padding: 6px 12px;
  border-radius: 20px;
  font-size: 12px;
  color: var(--fg-muted);
  cursor: pointer;
  transition: all 0.16s;
}
.deploy-tabs button.on {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.deploy-where {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  color: var(--fg-muted);
  padding: 9px 12px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
  margin-bottom: 12px;
}
.deploy-steps {
  margin: 0 0 14px;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 9px;
}
.deploy-steps li {
  font-size: 12.5px;
  color: var(--fg-muted);
  line-height: 1.65;
}
.deploy-text {
  display: block;
  margin-bottom: 5px;
  color: var(--fg);
}

.d-danger {
  margin-top: 22px;
  padding-top: 16px;
  border-top: 1px solid var(--border);
}
.dd-title {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  color: var(--fg-faint);
  font-family: var(--font-mono);
  margin-bottom: 11px;
}
.dd-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
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
.cf-icon.danger {
  background: color-mix(in srgb, var(--danger) 11%, var(--surface));
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
.cf-note code {
  font-family: var(--font-mono);
  color: var(--accent);
}
</style>
