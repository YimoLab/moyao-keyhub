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
import AppIcon from '../components/AppIcon.vue'
import AppModal from '../components/AppModal.vue'

const toast = useToast()
const store = useEnvStore()

/* ══════ 目标来源 ══════ */
const hosts = ref([])
const keys = ref([])
const sourceMode = ref('alias') // alias | manual

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
  <div class="diag">
    <div class="page-head">
      <div>
        <h1>连接测试</h1>
        <p class="sub">按 Host 别名或直连地址执行一次 SSH 握手，失败时给出针对性诊断。不会自动联网。</p>
      </div>
    </div>

    <!-- 顶部：目标选择 -->
    <div class="card seg-card">
      <div class="sc-head">
        <div class="sc-tabs">
          <button :class="{ on: sourceMode === 'alias' }" @click="((sourceMode = 'alias'), applyAlias(), refreshPreview())">
            <AppIcon name="config" :size="14" /> 使用 config 别名
          </button>
          <button :class="{ on: sourceMode === 'manual' }" @click="((sourceMode = 'manual'), refreshPreview())">
            <AppIcon name="terminal" :size="14" /> 直连测试
          </button>
        </div>
        <div class="sc-note">
          <AppIcon name="shield" :size="12" />
          仅在你点击测试后才会发起网络连接
        </div>
      </div>

      <!-- 别名模式 -->
      <div v-if="sourceMode === 'alias'" class="sc-body">
        <div v-if="!hosts.length" class="sc-empty">
          <AppIcon name="config" :size="20" />
          <span>config 里还没有 Host，先到「SSH Config」添加，或改用直连模式</span>
        </div>
        <template v-else>
          <div class="alias-chips">
            <button
              v-for="h in hosts"
              :key="h.alias"
              class="chip"
              :class="{ on: selectedAlias === h.alias }"
              @click="((selectedAlias = h.alias), applyAlias(), refreshPreview())"
            >
              <AppIcon name="server" :size="12" />
              {{ h.alias }}
              <span v-if="h.note" class="chip-note">{{ h.note }}</span>
            </button>
          </div>
          <div v-if="currentHost" class="alias-detail">
            <div class="ad-item">
              <span>目标</span>
              <code>{{ currentHost.user ? currentHost.user + '@' : '' }}{{ currentHost.hostname || currentHost.alias }}{{ currentHost.port && currentHost.port !== 22 ? ':' + currentHost.port : '' }}</code>
            </div>
            <div class="ad-item">
              <span>密钥</span>
              <code>{{ currentHost.identity || '未指定（回退默认）' }}</code>
            </div>
          </div>
        </template>
      </div>

      <!-- 直连模式 -->
      <div v-else class="sc-body">
        <div class="manual-grid">
          <div class="field">
            <label>主机名 / IP <span class="req">*</span></label>
            <input v-model="form.hostname" type="text" class="input mono" placeholder="192.168.1.10" @input="refreshPreview" />
          </div>
          <div class="field">
            <label>端口</label>
            <input v-model.number="form.port" type="number" min="1" max="65535" class="input mono" placeholder="22" @input="refreshPreview" />
          </div>
          <div class="field">
            <label>登录用户</label>
            <input v-model="form.user" type="text" class="input mono" placeholder="root" @input="refreshPreview" />
          </div>
          <div class="field">
            <label>密钥</label>
            <select v-model="form.identity" class="select mono" @change="refreshPreview">
              <option value="">（使用默认密钥 / agent）</option>
              <option v-for="k in keys" :key="k.filename" :value="'~/.ssh/' + k.filename">
                {{ k.filename }}
              </option>
            </select>
          </div>
        </div>
      </div>

      <!-- 通用选项 + 执行 -->
      <div class="sc-foot">
        <div class="opt-row">
          <label class="opt">
            <span>超时</span>
            <input v-model.number="form.timeoutSecs" type="number" min="3" max="120" class="input tiny mono" @change="refreshPreview" />
            <span class="unit">秒</span>
          </label>
          <label class="checkbox">
            <input v-model="form.verbose" type="checkbox" @change="refreshPreview" />
            <span>详细模式（-v）</span>
          </label>
        </div>
        <button
          class="btn primary"
          :disabled="running || (sourceMode === 'alias' && !form.alias)"
          :title="sourceMode === 'alias' && !form.alias ? '先选择一个 Host，或改用直连模式' : ''"
          @click="askRun"
        >
          <span v-if="running" class="spin" />
          <AppIcon v-else name="pulse" :size="15" />
          {{ running ? '测试中…' : '开始测试' }}
        </button>
      </div>

      <!-- 命令预览 -->
      <div v-if="previewCmd" class="cmd-preview">
        <span class="cp-label">将要执行</span>
        <code>{{ previewCmd }}</code>
      </div>
    </div>

    <!-- 双栏：终端 + 结果 -->
    <div class="diag-grid">
      <!-- 终端输出 -->
      <div class="card term-card">
        <div class="card-head">
          <span class="ch-title"><AppIcon name="terminal" :size="14" /> 输出</span>
          <button class="btn ghost xs" :disabled="!terminal" @click="terminal = ''">清空</button>
        </div>
        <div class="terminal" :class="{ empty: !terminal }">
          <pre v-if="terminal">{{ terminal }}</pre>
          <div v-else class="term-empty">
            <AppIcon name="terminal" :size="22" />
            <span>点击「开始测试」后，这里会显示 ssh 的原始输出</span>
          </div>
        </div>
      </div>

      <!-- 结果与诊断 -->
      <aside class="result-pane">
        <!-- 结果卡 -->
        <div v-if="result" class="card res-card" :class="result.success ? 'ok' : 'fail'">
          <div class="res-head">
            <span class="res-seal" :class="result.success ? 'ok' : 'fail'">
              <AppIcon :name="result.success ? 'check' : 'warning'" :size="20" />
            </span>
            <div class="res-title">
              <strong>{{ result.success ? '连接成功' : '连接失败' }}</strong>
              <span>{{ result.target }}</span>
            </div>
          </div>
          <div class="res-metrics">
            <div class="rm">
              <span>耗时</span>
              <code>{{ result.elapsedMs }} ms</code>
            </div>
            <div class="rm">
              <span>退出码</span>
              <code>{{ result.exitCode }}</code>
            </div>
          </div>
          <div v-if="result.remoteBanner" class="res-banner">
            <AppIcon name="server" :size="12" />
            <code>{{ result.remoteBanner }}</code>
          </div>
        </div>

        <!-- 智能诊断 -->
        <div class="card diag-card">
          <div class="card-head">
            <span class="ch-title"><AppIcon name="pulse" :size="14" /> 智能诊断</span>
          </div>
          <div v-if="!result" class="diag-empty">
            <AppIcon name="pulse" :size="22" />
            <span>测试完成后，这里会给出针对性的处置建议</span>
          </div>
          <div v-else-if="!result.diagnoses?.length" class="diag-empty">
            <AppIcon name="check" :size="20" />
            <span>没有需要处理的问题</span>
          </div>
          <ul v-else class="diag-list">
            <li
              v-for="(d, i) in result.diagnoses"
              :key="i"
              class="diag-item"
              :class="d.level"
            >
              <span class="di-icon">
                <AppIcon
                  :name="d.level === 'ok' ? 'check' : d.level === 'error' ? 'warning' : 'info'"
                  :size="15"
                />
              </span>
              <div class="di-body">
                <strong>{{ d.title }}</strong>
                <p>{{ d.detail }}</p>
                <p v-if="d.fix" class="di-fix">{{ d.fix }}</p>
                <button
                  v-if="d.action"
                  class="btn xs"
                  :disabled="fixBusy"
                  @click="runAction(d.action)"
                >
                  <span v-if="fixBusy && d.action === 'fix-perms'" class="spin" />
                  {{ actionLabel(d.action) }}
                </button>
              </div>
            </li>
          </ul>
        </div>

        <!-- 平台专属入口：平台未知时不渲染 -->
        <div v-if="platformTips" class="card plat-card">
          <div class="card-head">
            <span class="ch-title"><AppIcon name="settings" :size="14" /> {{ platformTips.title }}</span>
          </div>
          <ul class="plat-list">
            <li v-for="(p, i) in platformTips.items" :key="i">
              <AppIcon :name="p.icon" :size="13" />
              <span>{{ p.text }}</span>
              <button v-if="p.action" class="btn xs" @click="runAction(p.action)">
                {{ p.label }}
              </button>
            </li>
          </ul>
        </div>
      </aside>
    </div>

    <!-- 测试历史 -->
    <div class="card hist-card">
      <div class="card-head">
        <span class="ch-title"><AppIcon name="clock" :size="14" /> 测试历史</span>
        <button v-if="history.length" class="btn ghost xs" @click="clearHistory">清空</button>
      </div>
      <div v-if="!history.length" class="hist-empty">还没有测试记录</div>
      <table v-else class="ds-table hist-table">
        <thead>
          <tr>
            <th>时间</th>
            <th>目标</th>
            <th>结果</th>
            <th>耗时</th>
            <th>说明</th>
            <th class="ta-r">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(h, i) in history" :key="i">
            <td><code class="mono-sm">{{ h.at?.slice(0, 19).replace('T', ' ') }}</code></td>
            <td><code class="mono-sm">{{ h.target }}</code></td>
            <td>
              <span class="pill" :class="h.success ? 'ok' : 'danger'">
                {{ h.success ? '成功' : '失败' }}
              </span>
            </td>
            <td><code class="mono-sm">{{ h.elapsedMs }}ms</code></td>
            <td><span class="hist-sum">{{ h.summary }}</span></td>
            <td class="ta-r">
              <button class="btn xs" @click="quickAlias(h.target)">重测</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- 联网二次确认 -->
    <AppModal v-model="confirmOpen" title="确认发起网络连接" :dismissable="false">
      <div class="confirm">
        <div class="cf-icon warn"><AppIcon name="warning" :size="20" /></div>
        <p class="cf-lead">
          即将通过 ssh 连接
          <code>{{ sourceMode === 'alias' ? form.alias : form.hostname }}</code>。
        </p>
        <ul class="cf-list">
          <li>这将产生一次真实的网络请求（仅此一次）</li>
          <li>墨钥不会记录或上传任何连接内容</li>
          <li>这次测试会写入本地审计日志</li>
        </ul>
        <div v-if="previewCmd" class="rawbox">
          <code>{{ previewCmd }}</code>
        </div>
        <label class="checkbox">
          <input type="checkbox" checked disabled />
          <span class="muted">不再提示可在「设置」中调整</span>
        </label>
      </div>
      <template #footer>
        <button class="btn ghost" @click="confirmOpen = false">取消</button>
        <button class="btn primary" @click="runTest">确认测试</button>
      </template>
    </AppModal>

    <!-- known_hosts 查看 -->
    <AppModal v-model="khOpen" :title="'known_hosts · ' + khHost">
      <div class="confirm">
        <p class="cf-lead">
          墨钥在 <code>~/.ssh/known_hosts</code> 中找到 {{ khEntries.length }} 条与
          <code>{{ khHost }}</code> 相关的记录。
        </p>
        <div v-if="khEntries.length" class="rawbox tall">
          <code v-for="(e, i) in khEntries" :key="i">{{ e }}</code>
        </div>
        <p v-else class="cf-note">没有找到相关记录。若之前连接过，可能是被哈希存储了。</p>
        <div class="banner warn sm">
          <AppIcon name="warning" :size="14" />
          <span>
            只有在与服务器管理员核实指纹后，才应移除记录。贸然移除会让你暴露于中间人攻击。
          </span>
        </div>
      </div>
      <template #footer>
        <button class="btn ghost" @click="khOpen = false">关闭</button>
        <button class="btn danger" :disabled="removingKh || !khEntries.length" @click="removeKnownHost">
          <span v-if="removingKh" class="spin" />
          移除这些记录
        </button>
      </template>
    </AppModal>
  </div>
</template>

<style scoped>
.diag {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* 目标选择卡 */
.seg-card {
  padding: 0;
  overflow: hidden;
}
.sc-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border);
  flex-wrap: wrap;
}
.sc-tabs {
  display: flex;
  gap: 3px;
  padding: 3px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
}
.sc-tabs button {
  display: flex;
  align-items: center;
  gap: 7px;
  border: none;
  background: transparent;
  padding: 7px 15px;
  border-radius: 7px;
  font-size: 12.5px;
  color: var(--fg-muted);
  cursor: pointer;
  transition: all 0.16s;
}
.sc-tabs button.on {
  background: var(--surface);
  color: var(--fg);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.07);
}
.sc-note {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11.5px;
  color: var(--accent);
  background: var(--accent-soft);
  padding: 5px 11px;
  border-radius: 20px;
}
.sc-body {
  padding: 16px 18px;
}
.sc-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  padding: 22px;
  font-size: 12.5px;
  color: var(--fg-faint);
  text-align: center;
}

.alias-chips {
  display: flex;
  gap: 7px;
  flex-wrap: wrap;
  margin-bottom: 14px;
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 13px;
  background: var(--surface-warm);
  border: 1px solid var(--border);
  border-radius: 20px;
  font-size: 12px;
  font-family: var(--font-mono);
  color: var(--fg-muted);
  cursor: pointer;
  transition: all 0.16s;
}
.chip:hover {
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
}
.chip.on {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.chip-note {
  font-family: var(--font-sans);
  font-size: 10.5px;
  opacity: 0.75;
}
.alias-detail {
  display: flex;
  gap: 24px;
  flex-wrap: wrap;
  padding: 12px 14px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
}
.ad-item {
  display: flex;
  align-items: baseline;
  gap: 9px;
  font-size: 12px;
}
.ad-item > span {
  color: var(--fg-faint);
}
.ad-item code {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg);
}

.manual-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 13px;
}
@media (max-width: 1100px) {
  .manual-grid {
    grid-template-columns: 1fr 1fr;
  }
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field label {
  font-size: 12px;
  font-weight: 600;
  color: var(--fg);
}
.req {
  color: var(--danger);
}

.sc-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 13px 18px;
  border-top: 1px solid var(--border);
  background: var(--surface-warm);
  flex-wrap: wrap;
}
.opt-row {
  display: flex;
  align-items: center;
  gap: 18px;
}
.opt {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  color: var(--fg-muted);
}
.input.tiny {
  width: 62px;
  height: 30px;
  text-align: center;
  padding: 0 6px;
}
.unit {
  font-size: 11.5px;
  color: var(--fg-faint);
}
.checkbox {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  color: var(--fg-muted);
  cursor: pointer;
  user-select: none;
}
.checkbox input {
  width: 14px;
  height: 14px;
  accent-color: var(--accent);
  cursor: pointer;
}
.checkbox .muted {
  color: var(--fg-faint);
}

.cmd-preview {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 11px 18px;
  border-top: 1px solid var(--border);
  background: var(--surface);
  overflow-x: auto;
}
.cp-label {
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  color: var(--fg-faint);
  font-family: var(--font-mono);
  flex: 0 0 auto;
}
.cmd-preview code {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--accent);
  white-space: nowrap;
}

/* 双栏 */
.diag-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 348px;
  gap: 16px;
  align-items: start;
}
@media (max-width: 1200px) {
  .diag-grid {
    grid-template-columns: 1fr;
  }
}

.card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 13px 16px;
  border-bottom: 1px solid var(--border);
}
.ch-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--fg);
}
.btn.xs {
  height: 26px;
  padding: 0 9px;
  font-size: 11.5px;
  border-radius: 7px;
  gap: 4px;
}

/* 终端 */
.term-card {
  padding: 0;
  overflow: hidden;
}
.terminal {
  background: var(--terminal-bg, #10151f);
  padding: 16px 18px;
  min-height: 300px;
  max-height: 480px;
  overflow: auto;
}
.terminal.empty {
  display: grid;
  place-items: center;
}
.terminal pre {
  margin: 0;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.7;
  color: #d6deeb;
  white-space: pre-wrap;
  word-break: break-all;
}
.term-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  color: #5f6b80;
  font-size: 12px;
  text-align: center;
}

/* 结果栏 */
.result-pane {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.res-card {
  padding: 16px;
  border-width: 1px;
}
.res-card.ok {
  border-color: color-mix(in srgb, var(--success) 34%, var(--border));
  background: color-mix(in srgb, var(--success) 4%, var(--surface));
}
.res-card.fail {
  border-color: color-mix(in srgb, var(--danger) 34%, var(--border));
  background: color-mix(in srgb, var(--danger) 4%, var(--surface));
}
.res-head {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 14px;
}
.res-seal {
  width: 42px;
  height: 42px;
  border-radius: 14px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
}
.res-seal.ok {
  background: var(--success-soft);
  color: var(--success-ink);
}
.res-seal.fail {
  background: var(--danger-soft);
  color: var(--danger);
}
.res-title strong {
  display: block;
  font-size: 14px;
  color: var(--fg);
  margin-bottom: 3px;
}
.res-title span {
  font-size: 11.5px;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.res-metrics {
  display: flex;
  gap: 22px;
  padding: 11px 0;
  border-top: 1px solid var(--border);
}
.rm {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.rm span {
  font-size: 10.5px;
  color: var(--fg-faint);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.rm code {
  font-family: var(--font-mono);
  font-size: 14px;
  color: var(--fg);
  font-weight: 600;
}
.res-banner {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 8px 11px;
  background: var(--surface);
  border-radius: var(--radius-sm);
  font-size: 11px;
  color: var(--fg-muted);
  margin-top: 4px;
}
.res-banner code {
  font-family: var(--font-mono);
  font-size: 11px;
  word-break: break-all;
}

/* 诊断 */
.diag-card,
.plat-card {
  padding: 0;
  overflow: hidden;
}
.diag-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 9px;
  padding: 30px 18px;
  color: var(--fg-faint);
  font-size: 12px;
  text-align: center;
}
.diag-list {
  list-style: none;
  padding: 12px;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 9px;
  max-height: 460px;
  overflow-y: auto;
}
.diag-item {
  display: flex;
  gap: 10px;
  padding: 11px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--surface-warm);
}
.diag-item.error {
  border-color: color-mix(in srgb, var(--danger) 30%, var(--border));
  background: color-mix(in srgb, var(--danger) 4%, var(--surface));
}
.diag-item.warn {
  border-color: color-mix(in srgb, var(--warn) 30%, var(--border));
  background: color-mix(in srgb, var(--warn) 4%, var(--surface));
}
.diag-item.ok {
  border-color: color-mix(in srgb, var(--success) 30%, var(--border));
  background: color-mix(in srgb, var(--success) 4%, var(--surface));
}
.di-icon {
  flex: 0 0 auto;
  margin-top: 1px;
}
.diag-item.error .di-icon {
  color: var(--danger);
}
.diag-item.warn .di-icon {
  color: var(--warn);
}
.diag-item.ok .di-icon {
  color: var(--success);
}
.diag-item.info .di-icon {
  color: var(--accent);
}
.di-body {
  flex: 1;
  min-width: 0;
}
.di-body strong {
  display: block;
  font-size: 12.5px;
  color: var(--fg);
  margin-bottom: 4px;
  line-height: 1.5;
}
.di-body p {
  font-size: 11.5px;
  color: var(--fg-muted);
  margin: 0 0 5px;
  line-height: 1.6;
}
.di-fix {
  padding: 7px 10px;
  background: var(--surface);
  border-radius: 7px;
  border-left: 2px solid var(--accent);
  color: var(--fg) !important;
}
.di-body .btn {
  margin-top: 7px;
}

/* 平台入口 */
.plat-list {
  list-style: none;
  padding: 12px;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.plat-list li {
  display: flex;
  align-items: center;
  gap: 9px;
  font-size: 12px;
  color: var(--fg-muted);
  padding: 9px 11px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
  line-height: 1.5;
}
.plat-list li :deep(svg) {
  color: var(--fg-faint);
  flex: 0 0 auto;
}
.plat-list li span {
  flex: 1;
}
.plat-list li .btn {
  flex: 0 0 auto;
}

/* 历史 */
.hist-card {
  padding: 0;
  overflow: hidden;
}
.hist-empty {
  padding: 30px;
  text-align: center;
  font-size: 12.5px;
  color: var(--fg-faint);
}
.hist-table {
  width: 100%;
}
.mono-sm {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg-muted);
}
.hist-sum {
  font-size: 12px;
  color: var(--fg-muted);
}
.ta-r {
  text-align: right;
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
.cf-note {
  font-size: 12px;
  color: var(--fg-muted);
  line-height: 1.7;
  margin: 0;
  padding: 10px 12px;
  background: var(--surface-warm);
  border-radius: var(--radius-sm);
}
.rawbox.tall {
  max-height: 260px;
  overflow: auto;
}
.rawbox.tall code {
  display: block;
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.6;
}
</style>
