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
import Icon from '../design/vue/ui/Icon.vue'
import PageHead from '../design/vue/ui/PageHead.vue'
import StatCard from '../design/vue/ui/StatCard.vue'
import EmptyState from '../design/vue/ui/EmptyState.vue'
import AlertBox from '../design/vue/ui/AlertBox.vue'
import SelectMenu from '../design/vue/ui/SelectMenu.vue'
import SegmentedControl from '../design/vue/ui/SegmentedControl.vue'
import TabBar from '../design/vue/ui/TabBar.vue'
import KvList from '../design/vue/ui/KvList.vue'
import FieldInput from '../design/vue/ui/FieldInput.vue'
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

/** SelectMenu 需要 { value, label } 结构 */
const algoSelectOptions = computed(() =>
  algoOptions.value.map((v) => ({ value: v, label: v === 'all' ? '全部算法' : v }))
)
const statusOptions = [
  { value: 'all', label: '全部状态' },
  { value: 'passphrase', label: '已设口令' },
  { value: 'nopass', label: '无口令' },
  { value: 'badperm', label: '权限异常' },
]
const rsaBitOptions = [
  { value: '2048', label: '2048' },
  { value: '3072', label: '3072' },
  { value: '4096', label: '4096 · 推荐' },
]

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

/** SegmentedControl 的 modelValue 是字符串，这里与数字位数互转 */
const rsaBitsModel = computed({
  get: () => String(gen.value.bits),
  set: (v) => {
    gen.value.bits = Number(v)
  },
})

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
  <div class="kh-page">
    <PageHead
      title="密钥库"
      desc="本机 ~/.ssh 中的密钥。生成、复制、部署与轮换都在这里完成，私钥不会离开该目录。"
    >
      <template #actions>
        <button class="anx-btn anx-btn--ghost anx-btn--sm" :disabled="loading" @click="load">
          <Icon name="refresh" :size="15" />
          刷新
        </button>
        <button class="anx-btn anx-btn--primary" :disabled="!!loadError" @click="openGen">
          <Icon name="plus" :size="16" />
          生成密钥
        </button>
      </template>
    </PageHead>

    <!-- 轮换提醒 -->
    <div v-if="staleKeys.length && !bannerDismissed" class="anx-alert anx-alert--warn">
      <Icon name="clock" :size="18" />
      <div class="anx-alert__body">
        <p class="anx-alert__title">
          有 {{ staleKeys.length }} 把密钥使用已超过 {{ ROTATE_DAYS }} 天
        </p>
        <p class="anx-alert__text">
          长期不更换的密钥一旦泄露，风险敞口会持续扩大。建议生成新密钥并完成部署后再下线旧的。
        </p>
      </div>
      <div class="kh-actions kh-alert-actions">
        <button class="anx-btn anx-btn--sm" @click="q = staleKeys[0].filename">查看</button>
        <button class="anx-iconbtn" aria-label="关闭提醒" @click="bannerDismissed = true">
          <Icon name="close" :size="15" />
        </button>
      </div>
    </div>

    <!-- 统计条：读取失败时显示未知，而不是把空数组说成 0 -->
    <div class="anx-grid-4">
      <StatCard label="密钥总数" :value="loadError ? '—' : String(stats.total)" unit="把" />
      <StatCard label="已设口令" :value="loadError ? '—' : String(stats.passphrase)" unit="把" />
      <StatCard label="权限异常" :value="loadError ? '—' : String(stats.badPerm)" unit="项" />
      <StatCard label="已在 config 引用" :value="loadError ? '—' : String(stats.referencing)" unit="把" />
    </div>

    <!-- 工具栏 -->
    <div class="kh-toolbar">
      <div class="kh-toolbar__left">
        <div class="kh-searchfield">
          <Icon name="search" :size="15" />
          <input
            v-model="q"
            class="anx-input"
            type="text"
            placeholder="搜索文件名、注释或指纹…"
            aria-label="搜索密钥"
          />
          <button v-if="q" class="kh-searchfield__clear" aria-label="清空搜索" @click="q = ''">
            <Icon name="close" :size="13" />
          </button>
        </div>
        <SelectMenu
          v-model="filterAlgo"
          :options="algoSelectOptions"
          aria-label="按算法筛选"
          style="width: 148px"
        />
        <SelectMenu
          v-model="filterFlag"
          :options="statusOptions"
          aria-label="按状态筛选"
          style="width: 148px"
        />
      </div>
    </div>

    <!-- 主内容 -->
    <section class="anx-panel">
      <div class="anx-panel__head">
        <div>
          <h2>密钥列表</h2>
          <p>{{ loadError ? '读取失败' : `${filtered.length} / ${keys.length} 条` }}</p>
        </div>
      </div>

      <div class="anx-panel__body anx-panel__body--tight">
        <div v-if="loading" class="anx-empty kh-empty-inline">
          <span class="kh-spin kh-spin--lg" />
          <p class="anx-empty__title">正在读取 ~/.ssh …</p>
        </div>

        <AppErrorState
          v-else-if="loadError"
          title="无法读取 ~/.ssh"
          :detail="loadError.message"
          @retry="load"
        />

        <EmptyState
          v-else-if="keys.length === 0"
          icon="key"
          title="还没有任何密钥"
          text="生成一把新密钥，或确认 ~/.ssh 中已有密钥文件"
        >
          <template #actions>
            <button class="anx-btn anx-btn--primary anx-btn--sm" @click="openGen">
              <Icon name="plus" :size="15" />
              生成第一把密钥
            </button>
          </template>
        </EmptyState>

        <EmptyState
          v-else-if="filtered.length === 0"
          icon="search"
          title="没有匹配的密钥"
          text="换个关键词或清空筛选条件试试"
        >
          <template #actions>
            <button
              class="anx-btn anx-btn--sm"
              @click="((q = ''), (filterAlgo = 'all'), (filterFlag = 'all'))"
            >
              清空筛选
            </button>
          </template>
        </EmptyState>

        <div v-else class="anx-table-wrap">
          <table class="anx-table anx-table--clickable kh-key-table">
            <thead>
              <tr>
                <th>文件名</th>
                <th>算法</th>
                <th>指纹</th>
                <th>权限</th>
                <th>口令</th>
                <th class="is-num">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="k in filtered"
                :key="k.filename"
                tabindex="0"
                role="button"
                :aria-label="`查看密钥 ${k.filename} 详情`"
                @click="openDetail(k)"
                @keydown.enter="openDetail(k)"
                @keydown.space.prevent="openDetail(k)"
              >
                <td>
                  <div class="anx-cell-main">
                    <span class="kh-key-ico"><Icon name="key" :size="15" /></span>
                    <div class="kh-grow">
                      <div class="anx-cell-title">{{ k.filename }}</div>
                      <div v-if="k.comment || k.createdAt" class="anx-cell-sub">
                        {{ [k.comment, (k.createdAt || '').slice(0, 10)].filter(Boolean).join(' · ') }}
                      </div>
                    </div>
                    <span
                      v-if="referencedSet.has(k.filename)"
                      class="anx-badge anx-badge--outline"
                      title="已被 ~/.ssh/config 引用"
                    >
                      已引用
                    </span>
                  </div>
                </td>
                <td><span class="anx-tag">{{ k.algorithm }}</span></td>
                <td>
                  <code class="kh-mono kh-fp" :title="k.fingerprint || ''">{{ shortFp(k.fingerprint) }}</code>
                </td>
                <td>
                  <span class="anx-badge" :class="k.permCompliant ? 'anx-badge--ok' : 'anx-badge--warn'">
                    {{ k.permDisplay || '—' }}
                  </span>
                </td>
                <td>
                  <span class="anx-badge" :class="k.hasPassphrase ? 'anx-badge--ok' : 'anx-badge--neutral'">
                    <Icon :name="k.hasPassphrase ? 'lock' : 'unlock'" :size="12" />
                    {{ k.hasPassphrase ? '已保护' : '无' }}
                  </span>
                </td>
                <td class="is-num" @click.stop>
                  <div class="anx-cell-actions">
                    <button
                      v-if="!k.permCompliant"
                      class="anx-btn anx-btn--danger-quiet anx-btn--sm"
                      title="修复为 600"
                      @click="fixPerm(k)"
                    >
                      修复权限
                    </button>
                    <button
                      class="anx-btn anx-btn--ghost anx-btn--sm"
                      title="加入 ssh-agent"
                      aria-label="加入 ssh-agent"
                      :disabled="agentBusy"
                      @click="addToAgent(k)"
                    >
                      <Icon name="shield" :size="14" />
                      agent
                    </button>
                    <button
                      class="anx-iconbtn anx-btn--sm"
                      title="查看详情"
                      aria-label="查看详情"
                      @click="openDetail(k)"
                    >
                      <Icon name="eye" :size="15" />
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </section>

    <!-- ══════ 生成密钥弹窗 ══════ -->
    <AppModal v-model="genOpen" title="生成 SSH 密钥" wide>
      <div class="kh-form">
        <AlertBox v-if="genError" tone="danger" title="生成失败" :text="genError" />

        <div class="anx-field kh-field">
          <span class="anx-field__label">算法</span>
          <div class="kh-choice-row">
            <button
              v-for="a in [
                { id: 'ed25519', t: 'ED25519', d: '现代首选，密钥短、速度快、安全性高' },
                { id: 'rsa', t: 'RSA', d: '兼容性最好，适合老旧服务器与设备' },
              ]"
              :key="a.id"
              type="button"
              class="kh-choice"
              :aria-pressed="gen.algorithm === a.id"
              @click="((gen.algorithm = a.id), (gen.filename = suggestFilename(a.id)))"
            >
              <strong>{{ a.t }}</strong>
              <span>{{ a.d }}</span>
            </button>
          </div>
        </div>

        <div v-if="gen.algorithm === 'rsa'" class="anx-field kh-field">
          <span class="anx-field__label">密钥长度</span>
          <SegmentedControl v-model="rsaBitsModel" :options="rsaBitOptions" class="kh-seg-wide" />
          <p class="anx-field__hint">位数越高越安全，但握手开销也越大。4096 是当前通用推荐值。</p>
        </div>

        <FieldInput
          v-model="gen.filename"
          label="文件名"
          required
          mono
          placeholder="id_ed25519"
          :error="filenameIssue"
          :hint="filenameIssue ? '' : `将生成 ~/.ssh/${gen.filename} 与 ~/.ssh/${gen.filename}.pub`"
        />

        <FieldInput
          v-model="gen.comment"
          label="注释（通常是邮箱或用途）"
          placeholder="leeli@example.com"
          hint="注释会附加在公钥末尾，用于在服务器上区分多把密钥。"
        />

        <div class="anx-field kh-field">
          <label class="anx-field__label" for="gen-pass">口令保护（可选，但强烈建议）</label>
          <div class="kh-pass">
            <input
              id="gen-pass"
              v-model="gen.passphrase"
              :type="gen.showPass ? 'text' : 'password'"
              class="anx-input"
              placeholder="留空表示不加密私钥"
              autocomplete="new-password"
            />
            <button
              type="button"
              class="anx-input-wrap__action"
              :aria-label="gen.showPass ? '隐藏口令' : '显示口令'"
              @click="gen.showPass = !gen.showPass"
            >
              <Icon :name="gen.showPass ? 'eye-off' : 'eye'" :size="16" />
            </button>
          </div>

          <div v-if="gen.passphrase" class="kh-strength-row">
            <div class="anx-strength kh-grow" aria-hidden="true">
              <span
                v-for="i in 3"
                :key="i"
                class="anx-strength__bar"
                :class="i <= passStrength.level ? `is-on-${passStrength.level}` : ''"
              />
            </div>
            <span class="kh-fine">强度：{{ passStrength.label }}</span>
          </div>
          <p v-if="passIssue" class="anx-field__error">{{ passIssue }}</p>

          <template v-if="gen.passphrase">
            <input
              v-model="gen.confirmPass"
              :type="gen.showPass ? 'text' : 'password'"
              class="anx-input kh-mt-sm"
              placeholder="再次输入口令"
              autocomplete="new-password"
            />
            <p v-if="passMismatch" class="anx-field__error">两次输入的口令不一致</p>
          </template>

          <AlertBox
            v-if="gen.passphrase"
            tone="info"
            text="设置口令后，非交互场景（如 git push）需要把密钥加入 ssh-agent。生成完成后墨钥会引导你完成这一步。"
          />
        </div>
      </div>

      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="genOpen = false">取消</button>
        <button class="anx-btn anx-btn--primary" :disabled="!canGenerate" @click="doGenerate">
          <span v-if="generating" class="anx-btn__spinner" />
          {{ generating ? '生成中…' : '生成密钥' }}
        </button>
      </template>
    </AppModal>

    <!-- ══════ 详情抽屉 ══════ -->
    <AppDrawer v-model="detailOpen" :title="current?.filename || '密钥详情'">
      <template v-if="current">
        <div class="kh-detail">
          <div class="kh-detail__hero">
            <span class="kh-detail__seal"><Icon name="key" :size="22" /></span>
            <div class="kh-grow">
              <strong class="anx-t-h2">{{ current.filename }}</strong>
              <div class="kh-actions kh-mt-sm">
                <span class="anx-tag">{{ current.algorithm }}</span>
                <span
                  class="anx-badge"
                  :class="current.permCompliant ? 'anx-badge--ok' : 'anx-badge--warn'"
                >
                  {{ current.permDisplay || '—' }}
                </span>
                <span
                  class="anx-badge"
                  :class="current.hasPassphrase ? 'anx-badge--ok' : 'anx-badge--neutral'"
                >
                  {{ current.hasPassphrase ? '口令保护' : '无口令' }}
                </span>
              </div>
            </div>
          </div>

          <KvList
            :rows="[
              ['指纹', current.fingerprint || '不可用'],
              ['路径', current.prettyPath],
              ['注释', current.comment || '—'],
              ['创建时间', (current.createdAt || '').slice(0, 19).replace('T', ' ') || '—'],
              ['文件大小', formatSize(current.size)],
              ['config 引用', referencedSet.has(current.filename) ? '是' : '否'],
            ]"
          />

          <div v-if="!current.permCompliant" class="anx-alert anx-alert--warn">
            <Icon name="alert-triangle" :size="18" />
            <div class="anx-alert__body">
              <p class="anx-alert__title">该私钥权限过宽，OpenSSH 可能拒绝加载。</p>
              <p class="anx-alert__text">
                <button class="anx-btn anx-btn--danger-quiet anx-btn--sm" @click="fixPerm(current)">
                  立即修复为 600
                </button>
              </p>
            </div>
          </div>

          <!-- 公钥 / 私钥 / 部署 -->
          <TabBar :active="tab">
            <button
              class="anx-tab"
              role="tab"
              :aria-selected="tab === 'public'"
              @click="tab = 'public'"
            >
              公钥
            </button>
            <button
              class="anx-tab"
              role="tab"
              :aria-selected="tab === 'private'"
              @click="tab = 'private'"
            >
              私钥
            </button>
            <button
              class="anx-tab"
              role="tab"
              :aria-selected="tab === 'deploy'"
              @click="tab = 'deploy'"
            >
              部署
            </button>
          </TabBar>

          <!-- 公钥 -->
          <div v-if="tab === 'public'" class="kh-tabpane">
            <div v-if="loadingPub" class="anx-empty kh-empty-inline">
              <span class="kh-spin kh-spin--lg" />
              <p class="anx-empty__title">读取公钥…</p>
            </div>
            <template v-else-if="publicKey">
              <pre class="kh-pre kh-pre--plain">{{ publicKey }}</pre>
              <div class="kh-actions">
                <button class="anx-btn anx-btn--primary anx-btn--sm" @click="copyText(publicKey, '公钥已复制到剪贴板')">
                  <Icon name="copy" :size="14" />
                  复制公钥
                </button>
              </div>
              <p class="kh-fine">公钥可以安全地分发给任何服务器，它无法反推出私钥。</p>
            </template>
            <AlertBox
              v-else
              tone="warn"
              title="无法读取公钥"
              text="若私钥带口令，ssh-keygen 需要口令才能导出公钥。"
            />
          </div>

          <!-- 私钥 -->
          <div v-else-if="tab === 'private'" class="kh-tabpane">
            <div v-if="!showPrivate" class="kh-lockwall">
              <span class="kh-detail__seal kh-detail__seal--danger">
                <Icon name="lock" :size="22" />
              </span>
              <strong class="anx-t-h3">私钥是最高敏感信息</strong>
              <p class="anx-t-sm anx-t-muted">
                查看后请勿截图、粘贴到聊天工具或上传到任何在线服务。
                墨钥不会保存私钥副本，本次读取会在审计日志中留痕。
              </p>
              <button
                class="anx-btn anx-btn--danger"
                @click="((privateConfirmOpen = true), (privateAck = false))"
              >
                <Icon name="eye" :size="15" />
                我了解风险，查看私钥
              </button>
            </div>
            <template v-else>
              <pre class="kh-pre kh-pre--plain kh-pre--danger">{{ privateKey }}</pre>
              <div class="kh-actions">
                <button class="anx-btn anx-btn--sm" @click="copyText(privateKey, '私钥已复制（请谨慎保管）')">
                  <Icon name="copy" :size="14" />
                  复制私钥
                </button>
                <button
                  class="anx-btn anx-btn--ghost anx-btn--sm"
                  @click="((showPrivate = false), (privateKey = ''))"
                >
                  隐藏并清除
                </button>
              </div>
            </template>
          </div>

          <!-- 部署指引 -->
          <div v-else class="kh-tabpane">
            <div class="kh-deploy-tabs">
              <button
                v-for="t in deployTargets"
                :key="t.key"
                class="anx-btn anx-btn--sm"
                :class="deployTab === t.key ? 'anx-btn--primary' : 'anx-btn--ghost'"
                :aria-pressed="deployTab === t.key"
                @click="deployTab = t.key"
              >
                {{ t.label }}
              </button>
            </div>

            <div class="kh-deploy-where">
              <Icon name="folder" :size="14" />
              <span>{{ deployText.where }}</span>
            </div>

            <ol class="kh-deploy-steps">
              <li v-for="(s, i) in deployText.steps" :key="i">
                <template v-if="s.startsWith('echo ') || s.startsWith('mkdir') || s.startsWith('chmod')">
                  <span class="kh-fine">在服务器终端执行：</span>
                  <div class="kh-cmd">
                    <code class="anx-code anx-code--wrap">{{ s }}</code>
                    <button
                      class="anx-copybtn kh-cmd__copy"
                      aria-label="复制命令"
                      @click="copyText(s, '命令已复制')"
                    >
                      <Icon name="copy" :size="14" />
                    </button>
                  </div>
                </template>
                <span v-else>{{ s }}</span>
              </li>
            </ol>

            <button class="anx-btn anx-btn--primary anx-btn--sm" @click="copyText(publicKey, '公钥已复制，可粘贴到目标平台')">
              <Icon name="copy" :size="14" />
              复制公钥去部署
            </button>
          </div>

          <!-- 管理操作 -->
          <div class="kh-danger-zone">
            <h3 class="anx-section__title">管理操作</h3>
            <div class="kh-actions">
              <button
                v-if="!current.hasPassphrase"
                class="anx-btn anx-btn--ghost anx-btn--sm"
                @click="askPassphrase(current)"
              >
                <Icon name="lock" :size="14" />
                设置口令保护
              </button>
              <button class="anx-btn anx-btn--ghost anx-btn--sm" :disabled="agentBusy" @click="addToAgent(current)">
                <Icon name="shield" :size="14" />
                加入 ssh-agent
              </button>
              <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="removeFromAgent(current)">
                <Icon name="minus" :size="14" />
                从 agent 移除
              </button>
              <button class="anx-btn anx-btn--danger-quiet anx-btn--sm" @click="askDelete(current)">
                <Icon name="trash" :size="14" />
                删除密钥
              </button>
            </div>
          </div>
        </div>
      </template>
    </AppDrawer>

    <!-- 私钥二次确认 -->
    <AppModal v-model="privateConfirmOpen" title="确认查看私钥明文" :dismissable="false">
      <div class="kh-confirm">
        <span class="kh-cf-icon kh-cf-icon--danger"><Icon name="alert-triangle" :size="22" /></span>
        <p class="anx-t-sm">
          即将读取并显示 <code class="kh-code-inline">{{ current?.prettyPath }}</code> 的完整私钥内容。
        </p>
        <ul class="kh-check-list">
          <li>请确认屏幕没有被录屏或旁人观看</li>
          <li>不要将私钥粘贴到聊天软件、网盘或任何在线服务</li>
          <li>该操作会记录到审计日志（仅记录「查看了哪个密钥」，不记录内容）</li>
        </ul>
        <label class="anx-checkline">
          <input v-model="privateAck" type="checkbox" />
          <span class="anx-checkline__text">我已了解上述风险</span>
        </label>
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="privateConfirmOpen = false">取消</button>
        <button class="anx-btn anx-btn--danger" :disabled="!privateAck" @click="loadPrivate">
          确认查看
        </button>
      </template>
    </AppModal>

    <!-- 删除确认 -->
    <AppModal v-model="delOpen" title="删除密钥" :dismissable="false">
      <div v-if="delTarget" class="kh-confirm">
        <span class="kh-cf-icon kh-cf-icon--danger"><Icon name="trash" :size="20" /></span>
        <p class="anx-t-sm">
          将永久删除 <code class="kh-code-inline">{{ delTarget.filename }}</code>
          <template v-if="delTarget.hasPublic">
            及其公钥 <code class="kh-code-inline">{{ delTarget.filename }}.pub</code>
          </template>。
        </p>
        <AlertBox v-if="delBlocking.length" tone="danger" :lines="delBlocking" />
        <p class="anx-t-xs anx-t-muted">
          如果该私钥已在服务器上注册，删除本地文件不会自动撤销服务器授权，
          请记得同时从远程 <code class="kh-code-inline">authorized_keys</code> 中移除对应公钥。
        </p>
        <FieldInput
          v-if="store.rawPrefs.confirmDeleteTyped"
          v-model="delTyped"
          label="请输入文件名以确认"
          mono
          :placeholder="delTarget.filename"
        />
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="delOpen = false">取消</button>
        <button
          class="anx-btn anx-btn--danger"
          :disabled="store.rawPrefs.confirmDeleteTyped && delTyped !== delTarget?.filename"
          @click="doDelete"
        >
          确认删除
        </button>
      </template>
    </AppModal>

    <!-- 设置口令 -->
    <AppModal v-model="passOpen" title="设置口令保护">
      <div class="kh-form">
        <p class="anx-t-sm anx-t-muted">
          为 <code class="kh-code-inline">{{ current?.filename }}</code> 设置口令后，每次使用该私钥都需要验证。
        </p>
        <FieldInput
          v-model="passForm.old"
          label="原口令（无口令则留空）"
          type="password"
          autocomplete="off"
        />
        <FieldInput
          v-model="passForm.next"
          label="新口令"
          required
          type="password"
          autocomplete="new-password"
        />
        <FieldInput
          v-model="passForm.confirm"
          label="确认新口令"
          required
          type="password"
          autocomplete="new-password"
          :error="passForm.confirm && passForm.next !== passForm.confirm ? '两次输入不一致' : ''"
        />
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="passOpen = false">取消</button>
        <button
          class="anx-btn anx-btn--primary"
          :disabled="passBusy || !passForm.next || passForm.next !== passForm.confirm"
          @click="doPassphrase"
        >
          <span v-if="passBusy" class="anx-btn__spinner" />
          保存
        </button>
      </template>
    </AppModal>
  </div>
</template>
