<script setup>
/**
 * 连通性诊断（PRD 3.1.6）
 *
 * 布局：左 1fr 测试区 + 右 348px 结果区
 * 组成：
 *   - 目标选择（config 别名 / 直连）+ 参数表单 + 命令预览
 *   - 终端输出（恒深底，模拟真实 ssh 输出）
 *   - 结果卡：耗时、退出码、远端标识
 *   - 智能诊断：把 ssh 报错翻译成可执行建议，附动作按钮
 *   - 平台专属入口 + 测试历史
 *
 * 红线：只有用户点击「开始测试」才会发起网络连接，且会在按钮上明确提示。
 */
import { ref, computed, onMounted } from 'vue'
import * as api from '../lib/api'
import { useToast } from '../lib/toast'
import { useEnvStore } from '../lib/store'
import Icon from '../design/vue/ui/Icon.vue'
import PageHead from '../design/vue/ui/PageHead.vue'
import SelectMenu from '../design/vue/ui/SelectMenu.vue'
import SegmentedControl from '../design/vue/ui/SegmentedControl.vue'
import AlertBox from '../design/vue/ui/AlertBox.vue'
import AppModal from '../components/AppModal.vue'

const toast = useToast()
const store = useEnvStore()

/* ══════ 目标来源 ══════ */
const hosts = ref([])
const keys = ref([])
const sourceMode = ref('alias') // alias | manual
const sourceModeOptions = [
  { value: 'alias', label: '使用 config 别名' },
  { value: 'manual', label: '直连测试' },
]

/** 切换来源时的附带动作（回填别名 / 刷新命令预览） */
function switchMode(v) {
  sourceMode.value = v
  if (v === 'alias') applyAlias()
  refreshPreview()
}

const form = ref({
  alias: '',
  hostname: '',
  port: 22,
  user: '',
  identity: '',
  verbose: false,
  timeoutSecs: 10,
})

const selectedAlias = ref('')

const currentHost = computed(() => hosts.value.find((h) => h.alias === selectedAlias.value) || null)

const keyIdentityOptions = computed(() =>
  keys.value.map((k) => ({ value: '~/.ssh/' + k.filename, label: k.filename }))
)

async function loadTargets() {
  try {
    const doc = await api.readConfig()
    hosts.value = doc.hosts || []
  } catch {
    hosts.value = []
  }
  try {
    keys.value = (await api.scanKeys()) || []
  } catch {
    keys.value = []
  }
  if (hosts.value.length && !selectedAlias.value) {
    selectedAlias.value = hosts.value[0].alias
  }
}

/** 把选中的别名信息回填到表单（用于命令预览与测试） */
function applyAlias() {
  const h = currentHost.value
  form.value.alias = h ? h.alias : ''
  form.value.hostname = h?.hostname || ''
  form.value.port = h?.port || 22
  form.value.user = h?.user || ''
  form.value.identity = h?.identity || ''
}

/** 命令预览（不联网） */
const previewCmd = ref('')

async function refreshPreview() {
  try {
    const opts = buildOpts()
    previewCmd.value = await api.previewTestCommand(opts, form.value.timeoutSecs)
  } catch {
    previewCmd.value = ''
  }
}

function buildOpts() {
  if (sourceMode.value === 'alias') {
    return {
      alias: form.value.alias,
      hostname: '',
      port: 0,
      user: '',
      identity: '',
      timeoutSecs: form.value.timeoutSecs,
      verbose: form.value.verbose,
    }
  }
  return {
    alias: '',
    hostname: form.value.hostname,
    port: form.value.port,
    user: form.value.user,
    identity: form.value.identity,
    timeoutSecs: form.value.timeoutSecs,
    verbose: form.value.verbose,
  }
}

/* ══════ 测试执行 ══════ */
const running = ref(false)
const result = ref(null)
const terminal = ref('')
const confirmOpen = ref(false)

function askRun() {
  // 红线：发起网络请求前二次确认（可在偏好里关闭）
  const opts = buildOpts()
  if (sourceMode.value === 'manual' && !opts.hostname) {
    toast.push('请填写主机名，或选择一个 config 别名', 'warn')
    return
  }
  if (sourceMode.value === 'alias' && !opts.alias) {
    toast.push('请选择一个 Host 别名', 'warn')
    return
  }
  if (store.rawPrefs.confirmBeforeConnect) {
    confirmOpen.value = true
  } else {
    runTest()
  }
}

/** 终端逐行打字机效果 */
function typeOut(text) {
  terminal.value += text
}

async function runTest() {
  confirmOpen.value = false
  running.value = true
  result.value = null
  const opts = buildOpts()
  const target = sourceMode.value === 'alias' ? opts.alias : `${opts.user ? opts.user + '@' : ''}${opts.hostname}`
  terminal.value = `$ ${previewCmd.value || `ssh ${target}`}\n`

  try {
    const res = await api.testConnection(opts)
    result.value = res

    // 模拟终端实时输出，营造透明感
    if (res.stdout) typeOut(res.stdout + '\n')
    if (res.stderr) typeOut(res.stderr + '\n')
    typeOut(`\n[进程退出，代码 ${res.exitCode}，耗时 ${res.elapsedMs}ms]\n`)

    if (res.success) {
      toast.push(`连接成功（${res.elapsedMs}ms）`, 'success')
    } else {
      toast.push(res.summary || '连接失败', 'danger')
    }
    await loadHistory()
  } catch (e) {
    typeOut(`\n[墨钥错误] ${e.message}\n`)
    toast.push(e.message || '测试失败', 'danger')
  } finally {
    running.value = false
  }
}

/* ══════ 诊断动作路由 ══════ */
const fixBusy = ref(false)

async function runAction(action) {
  switch (action) {
    case 'fix-perms':
      fixBusy.value = true
      try {
        const n = await api.fixAllPermissions()
        toast.push(`已修复 ${n} 项权限`, 'success')
        await store.refreshEnv()
      } catch (e) {
        toast.push(e.message || '修复失败', 'danger')
      } finally {
        fixBusy.value = false
      }
      break
    case 'start-agent':
      try {
        const st = await api.agentStart()
        toast.push(st.running ? 'ssh-agent 已启动' : '启动失败', st.running ? 'success' : 'danger')
        await store.refreshAgent()
      } catch (e) {
        toast.push(e.message || '启动 agent 失败', 'danger')
      }
      break
    case 'copy-public':
      copyCurrentPublic()
      break
    case 'pick-key':
      toast.push('请在下方「直连」表单中选择一把密钥', 'info')
      sourceMode.value = 'manual'
      break
    case 'open-vault':
      toast.push('请到「密钥库」确认密钥是否存在', 'info')
      break
    case 'retry-longer':
      form.value.timeoutSecs = Math.min(120, form.value.timeoutSecs + 10)
      toast.push(`超时已调整为 ${form.value.timeoutSecs} 秒，可再次测试`, 'info')
      break
    case 'retry-verbose':
      form.value.verbose = true
      toast.push('已开启详细模式，再次测试可看到更多线索', 'info')
      break
    case 'show-known-hosts':
      await showKnownHosts()
      break
    case 'check-port':
      toast.push('请确认服务器 sshd 正在监听该端口', 'info')
      break
    default:
      break
  }
  refreshPreview()
}

async function copyCurrentPublic() {
  const h = currentHost.value
  const idName = (form.value.identity || '').split(/[/\\]/).pop() || h?.alias
  if (!idName) {
    toast.push('未指定密钥，无法复制公钥', 'warn')
    return
  }
  try {
    const pub = await api.readPublicKey(idName)
    await navigator.clipboard.writeText(pub)
    toast.push(`已复制 ${idName} 的公钥，可粘贴到服务器 authorized_keys`, 'success')
  } catch (e) {
    toast.push(e.message || '读取公钥失败', 'danger')
  }
}

/* known_hosts 处理 */
const khOpen = ref(false)
const khEntries = ref([])
const khHost = ref('')

async function showKnownHosts() {
  const host = form.value.hostname || form.value.alias
  khHost.value = host
  try {
    khEntries.value = (await api.knownHostsEntries(host)) || []
    khOpen.value = true
  } catch (e) {
    toast.push(e.message || '读取 known_hosts 失败', 'danger')
  }
}

const removingKh = ref(false)

async function removeKnownHost() {
  removingKh.value = true
  try {
    const n = await api.removeKnownHost(khHost.value)
    toast.push(`已移除 ${n} 条 known_hosts 记录`, 'success')
    khOpen.value = false
  } catch (e) {
    toast.push(e.message || '移除失败', 'danger')
  } finally {
    removingKh.value = false
  }
}

/* ══════ 历史 ══════ */
const history = ref([])

async function loadHistory() {
  try {
    history.value = (await api.readTestHistory(30)) || []
  } catch {
    history.value = []
  }
}

async function clearHistory() {
  try {
    await api.clearTestHistory()
    history.value = []
    toast.push('历史已清空', 'info')
  } catch (e) {
    toast.push(e.message || '清空失败', 'danger')
  }
}

/* ══════ 平台专属入口 ══════ */
const platformTips = computed(() => {
  const p = store.rawEnv.platform
  // 平台未知时不猜测：猜成 Linux 会把 Windows 用户引向 systemctl
  if (!p) return null
  if (p === 'windows') {
    return {
      title: 'Windows 相关',
      items: [
        { icon: 'shield', text: 'ssh-agent 是系统服务，启用需管理员权限', action: 'start-agent', label: '启动 agent' },
        { icon: 'folder', text: '密钥位于 %USERPROFILE%\\.ssh', action: null },
        { icon: 'info', text: 'Windows Terminal 可直接使用 ssh 别名', action: null },
      ],
    }
  }
  if (p === 'macos') {
    return {
      title: 'macOS 相关',
      items: [
        { icon: 'shield', text: 'Keychain 可通过 ssh-add --apple-use-keychain 集成', action: null },
        { icon: 'folder', text: '密钥位于 ~/.ssh', action: null },
        { icon: 'info', text: '检查「系统设置 → 隐私与安全性 → 完全磁盘访问」', action: null },
      ],
    }
  }
  return {
    title: 'Linux 相关',
    items: [
      { icon: 'shield', text: '确认 ssh-agent 已随桌面会话启动', action: 'start-agent', label: '启动 agent' },
      { icon: 'folder', text: '密钥位于 ~/.ssh，权限应为 700/600', action: 'fix-perms', label: '修复权限' },
      { icon: 'terminal', text: '可用 systemctl status ssh 检查服务端状态', action: null },
    ],
  }
})

/* 便捷动作 */
function quickAlias(alias) {
  sourceMode.value = 'alias'
  selectedAlias.value = alias
  applyAlias()
  refreshPreview()
}

/** 诊断动作的中文按钮文案 */
function actionLabel(action) {
  return (
    {
      'fix-perms': '一键修复权限',
      'start-agent': '启动 ssh-agent',
      'add-agent': '加入 ssh-agent',
      'copy-public': '复制公钥',
      'pick-key': '选择密钥',
      'open-vault': '打开密钥库',
      'retry-longer': '延长超时',
      'retry-verbose': '开启详细模式重试',
      'show-known-hosts': '查看 known_hosts',
      'check-port': '检查端口',
    }[action] || '处理'
  )
}

onMounted(async () => {
  await loadTargets()
  applyAlias()
  await refreshPreview()
  await loadHistory()
})
</script>

<template>
  <div class="kh-page">
    <PageHead
      title="连接测试"
      desc="按 Host 别名或直连地址执行一次 SSH 握手，失败时给出针对性诊断。不会自动联网。"
    />

    <!-- ══════ 目标选择 ══════ -->
    <section class="anx-panel">
      <div class="anx-panel__head">
        <SegmentedControl
          :model-value="sourceMode"
          :options="sourceModeOptions"
          @update:model-value="switchMode"
        />
        <div class="anx-panel__head-actions">
          <span class="anx-badge anx-badge--neutral">
            <Icon name="shield" :size="12" />
            仅在你点击测试后才会发起网络连接
          </span>
        </div>
      </div>

      <div class="anx-panel__body kh-stack">
        <!-- 别名模式 -->
        <template v-if="sourceMode === 'alias'">
          <div v-if="!hosts.length" class="anx-empty kh-empty-inline">
            <div class="anx-empty__icon"><Icon name="file-text" :size="22" /></div>
            <p class="anx-empty__title">config 里还没有 Host</p>
            <p class="anx-empty__text">先到「SSH Config」添加，或改用直连模式</p>
          </div>
          <template v-else>
            <div class="kh-chips">
              <button
                v-for="h in hosts"
                :key="h.alias"
                type="button"
                class="kh-chip"
                :aria-pressed="selectedAlias === h.alias"
                @click="((selectedAlias = h.alias), applyAlias(), refreshPreview())"
              >
                <Icon name="server" :size="13" />
                {{ h.alias }}
                <span v-if="h.note" class="kh-chip__note">{{ h.note }}</span>
              </button>
            </div>
            <div v-if="currentHost" class="kh-target-detail">
              <div class="kh-target-detail__item">
                <span>目标</span>
                <code>{{ currentHost.user ? currentHost.user + '@' : '' }}{{ currentHost.hostname || currentHost.alias }}{{ currentHost.port && currentHost.port !== 22 ? ':' + currentHost.port : '' }}</code>
              </div>
              <div class="kh-target-detail__item">
                <span>密钥</span>
                <code>{{ currentHost.identity || '未指定（回退默认）' }}</code>
              </div>
            </div>
          </template>
        </template>

        <!-- 直连模式 -->
        <div v-else class="kh-form-grid">
          <div class="anx-field kh-field">
            <label class="anx-field__label" for="dlg-host">主机名 / IP <span class="anx-req">*</span></label>
            <input
              id="dlg-host"
              v-model="form.hostname"
              class="anx-input anx-input--mono"
              type="text"
              placeholder="192.168.1.10"
              @input="refreshPreview"
            />
          </div>
          <div class="anx-field kh-field">
            <label class="anx-field__label" for="dlg-port">端口</label>
            <input
              id="dlg-port"
              v-model.number="form.port"
              class="anx-input anx-input--mono"
              type="number"
              min="1"
              max="65535"
              placeholder="22"
              @input="refreshPreview"
            />
          </div>
          <div class="anx-field kh-field">
            <label class="anx-field__label" for="dlg-user">登录用户</label>
            <input
              id="dlg-user"
              v-model="form.user"
              class="anx-input anx-input--mono"
              type="text"
              placeholder="root"
              @input="refreshPreview"
            />
          </div>
          <div class="anx-field kh-field">
            <label class="anx-field__label">密钥</label>
            <SelectMenu
              :model-value="form.identity"
              :options="keyIdentityOptions"
              placeholder="（使用默认密钥 / agent）"
              aria-label="选择密钥"
              @update:model-value="((form.identity = $event), refreshPreview())"
            />
          </div>
        </div>
      </div>

      <div class="anx-panel__foot kh-diag-foot">
        <div class="kh-actions kh-wrap">
          <label class="kh-inline-field">
            <span>超时</span>
            <input
              v-model.number="form.timeoutSecs"
              class="anx-input anx-input--mono kh-input-xs"
              type="number"
              min="3"
              max="120"
              @change="refreshPreview"
            />
            <span>秒</span>
          </label>
          <label class="anx-checkline">
            <input v-model="form.verbose" type="checkbox" @change="refreshPreview" />
            <span class="anx-checkline__text">详细模式（-v）</span>
          </label>
        </div>
        <button
          class="anx-btn anx-btn--primary"
          :disabled="running || (sourceMode === 'alias' && !form.alias)"
          :title="sourceMode === 'alias' && !form.alias ? '先选择一个 Host，或改用直连模式' : ''"
          @click="askRun"
        >
          <span v-if="running" class="anx-btn__spinner" />
          <Icon v-else name="activity" :size="16" />
          {{ running ? '测试中…' : '开始测试' }}
        </button>
      </div>

      <div v-if="previewCmd" class="anx-panel__foot kh-cmd-preview">
        <span class="kh-fine">将要执行</span>
        <code>{{ previewCmd }}</code>
      </div>
    </section>

    <!-- ══════ 双栏：终端 + 结果 ══════ -->
    <div class="kh-diag-grid">
      <!-- 终端输出 -->
      <section class="anx-panel">
        <div class="anx-panel__head">
          <div>
            <h2><Icon name="terminal" :size="15" /> 输出</h2>
          </div>
          <div class="anx-panel__head-actions">
            <button class="anx-btn anx-btn--ghost anx-btn--sm" :disabled="!terminal" @click="terminal = ''">
              清空
            </button>
          </div>
        </div>
        <div class="anx-panel__body">
          <div class="kh-terminal">
            <pre v-if="terminal">{{ terminal }}</pre>
            <div v-else class="kh-terminal__empty">
              <Icon name="terminal" :size="24" />
              <span>点击「开始测试」后，这里会显示 ssh 的原始输出</span>
            </div>
          </div>
        </div>
      </section>

      <!-- 结果与诊断 -->
      <aside class="kh-diag-side">
        <!-- 结果卡 -->
        <section v-if="result" class="anx-panel">
          <div class="kh-result-head">
            <span
              class="kh-result-seal"
              :class="result.success ? 'kh-result-seal--ok' : 'kh-result-seal--fail'"
            >
              <Icon :name="result.success ? 'check' : 'alert-triangle'" :size="20" />
            </span>
            <div class="kh-grow">
              <div class="anx-t-h3">{{ result.success ? '连接成功' : '连接失败' }}</div>
              <div class="anx-t-mono anx-t-muted">{{ result.target }}</div>
            </div>
          </div>
          <div class="anx-panel__body kh-result-metrics">
            <div class="kh-metric">
              <span>耗时</span>
              <code>{{ result.elapsedMs }} ms</code>
            </div>
            <div class="kh-metric">
              <span>退出码</span>
              <code>{{ result.exitCode }}</code>
            </div>
          </div>
          <div v-if="result.remoteBanner" class="anx-panel__foot kh-remote-banner">
            <Icon name="server" :size="13" />
            <code>{{ result.remoteBanner }}</code>
          </div>
        </section>

        <!-- 智能诊断 -->
        <section class="anx-panel">
          <div class="anx-panel__head">
            <div><h2><Icon name="activity" :size="15" /> 智能诊断</h2></div>
          </div>
          <div class="anx-panel__body">
            <div v-if="!result" class="anx-empty kh-empty-inline">
              <div class="anx-empty__icon"><Icon name="activity" :size="22" /></div>
              <p class="anx-empty__title">等待测试结果</p>
              <p class="anx-empty__text">测试完成后，这里会给出针对性的处置建议</p>
            </div>
            <div v-else-if="!result.diagnoses?.length" class="anx-empty kh-empty-inline">
              <div class="anx-empty__icon kh-ok-icon"><Icon name="check" :size="22" /></div>
              <p class="anx-empty__title">没有需要处理的问题</p>
            </div>
            <ul v-else class="kh-list">
              <li
                v-for="(d, i) in result.diagnoses"
                :key="i"
                class="kh-list__item kh-diag-item"
                :class="'kh-diag-item--' + d.level"
              >
                <span class="kh-diag-item__icon">
                  <Icon
                    :name="d.level === 'ok' ? 'check-circle' : d.level === 'error' ? 'alert-triangle' : 'info'"
                    :size="16"
                  />
                </span>
                <div class="kh-list__body">
                  <div class="kh-list__title">{{ d.title }}</div>
                  <p class="kh-list__text">{{ d.detail }}</p>
                  <p v-if="d.fix" class="kh-list__text kh-diag-fix">{{ d.fix }}</p>
                  <button
                    v-if="d.action"
                    class="anx-btn anx-btn--ghost anx-btn--sm kh-mt-sm"
                    :disabled="fixBusy"
                    @click="runAction(d.action)"
                  >
                    <span v-if="fixBusy && d.action === 'fix-perms'" class="anx-btn__spinner" />
                    {{ actionLabel(d.action) }}
                  </button>
                </div>
              </li>
            </ul>
          </div>
        </section>

        <!-- 平台专属入口：平台未知时不渲染 -->
        <section v-if="platformTips" class="anx-panel">
          <div class="anx-panel__head">
            <div><h2><Icon name="sliders" :size="15" /> {{ platformTips.title }}</h2></div>
          </div>
          <div class="anx-panel__body">
            <ul class="kh-list">
              <li v-for="(p, i) in platformTips.items" :key="i" class="kh-list__item kh-platform-item">
                <Icon :name="p.icon" :size="15" />
                <span class="kh-grow">{{ p.text }}</span>
                <button v-if="p.action" class="anx-btn anx-btn--ghost anx-btn--sm" @click="runAction(p.action)">
                  {{ p.label }}
                </button>
              </li>
            </ul>
          </div>
        </section>
      </aside>
    </div>

    <!-- ══════ 测试历史 ══════ -->
    <section class="anx-panel">
      <div class="anx-panel__head">
        <div>
          <h2><Icon name="clock" :size="15" /> 测试历史</h2>
          <p>{{ history.length }} 条记录</p>
        </div>
        <div class="anx-panel__head-actions">
          <button v-if="history.length" class="anx-btn anx-btn--ghost anx-btn--sm" @click="clearHistory">
            清空
          </button>
        </div>
      </div>
      <div class="anx-panel__body anx-panel__body--tight">
        <div v-if="!history.length" class="anx-empty kh-empty-inline">
          <p class="anx-empty__text">还没有测试记录</p>
        </div>
        <div v-else class="anx-table-wrap">
          <table class="anx-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>目标</th>
                <th>结果</th>
                <th>耗时</th>
                <th>说明</th>
                <th class="is-num">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(h, i) in history" :key="i">
                <td><code class="kh-mono kh-t-xs">{{ h.at?.slice(0, 19).replace('T', ' ') }}</code></td>
                <td><code class="kh-mono kh-t-xs">{{ h.target }}</code></td>
                <td>
                  <span class="anx-badge" :class="h.success ? 'anx-badge--ok' : 'anx-badge--danger'">
                    {{ h.success ? '成功' : '失败' }}
                  </span>
                </td>
                <td><code class="kh-mono kh-t-xs">{{ h.elapsedMs }}ms</code></td>
                <td><span class="kh-hist-sum">{{ h.summary }}</span></td>
                <td class="is-num">
                  <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="quickAlias(h.target)">重测</button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </section>

    <!-- 联网二次确认 -->
    <AppModal v-model="confirmOpen" title="确认发起网络连接" :dismissable="false">
      <div class="kh-confirm">
        <span class="kh-cf-icon kh-cf-icon--warn"><Icon name="alert-triangle" :size="20" /></span>
        <p class="anx-t-sm">
          即将通过 ssh 连接
          <code class="kh-code-inline">{{ sourceMode === 'alias' ? form.alias : form.hostname }}</code>。
        </p>
        <ul class="kh-check-list">
          <li>这将产生一次真实的网络请求（仅此一次）</li>
          <li>墨钥不会记录或上传任何连接内容</li>
          <li>这次测试会写入本地审计日志</li>
        </ul>
        <pre v-if="previewCmd" class="kh-pre">{{ previewCmd }}</pre>
        <label class="anx-checkline">
          <input type="checkbox" checked disabled />
          <span class="anx-checkline__text">不再提示可在「设置」中调整</span>
        </label>
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="confirmOpen = false">取消</button>
        <button class="anx-btn anx-btn--primary" @click="runTest">确认测试</button>
      </template>
    </AppModal>

    <!-- known_hosts 查看 -->
    <AppModal v-model="khOpen" :title="'known_hosts · ' + khHost">
      <div class="kh-confirm">
        <p class="anx-t-sm">
          墨钥在 <code class="kh-code-inline">~/.ssh/known_hosts</code> 中找到
          {{ khEntries.length }} 条与 <code class="kh-code-inline">{{ khHost }}</code> 相关的记录。
        </p>
        <pre v-if="khEntries.length" class="kh-pre kh-pre--plain">{{ khEntries.join('\n') }}</pre>
        <p v-else class="anx-t-xs anx-t-muted">没有找到相关记录。若之前连接过，可能是被哈希存储了。</p>
        <AlertBox
          tone="warn"
          title="移除记录前请先核实指纹"
          text="只有在与服务器管理员核实指纹后，才应移除记录。贸然移除会让你暴露于中间人攻击。"
        />
      </div>
      <template #footer>
        <button class="anx-btn anx-btn--ghost" @click="khOpen = false">关闭</button>
        <button
          class="anx-btn anx-btn--danger"
          :disabled="removingKh || !khEntries.length"
          @click="removeKnownHost"
        >
          <span v-if="removingKh" class="anx-btn__spinner" />
          移除这些记录
        </button>
      </template>
    </AppModal>
  </div>
</template>
