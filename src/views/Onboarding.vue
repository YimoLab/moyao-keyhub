<script setup>
/**
 * 首次引导（PRD 3.1.1）· 3 步向导
 *   1. 环境检测：逐项播放检测动画，暴露 ssh / git / ~/.ssh / agent 四项
 *   2. 导入密钥：扫描已有密钥，勾选纳入管理（只读，不改动文件）
 *   3. 完成：写入 onboardedAt，进入密钥库
 *
 * 该页面走 meta.bare 路由，不套应用外壳——引导阶段侧栏毫无意义。
 */
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import * as api from '../lib/api'
import { useEnvStore } from '../lib/store'
import { useToast } from '../lib/toast'
import AppIcon from '../components/AppIcon.vue'

const router = useRouter()
const toast = useToast()
const store = useEnvStore()

const step = ref(1)

/* 引导页没有应用外壳的标题栏，需要自己提供窗口控制 */
const isDesktop = api.isBackendAvailable()
let appWindow = null
if (isDesktop) {
  try {
    appWindow = getCurrentWindow()
  } catch {
    appWindow = null
  }
}
function winMin() {
  appWindow?.minimize().catch(() => {})
}
function winClose() {
  appWindow?.close().catch(() => {})
}

/* ── 第 1 步：环境检测 ── */
const detecting = ref(true)
const report = ref(null)
const detectError = ref(null)
/** 逐项播放：0 待检 → 1 检测中 → 2 完成 */
const progress = ref({ ssh: 0, git: 0, dir: 0, agent: 0 })
const guide = ref(null)
const showGuide = ref(false)
const enabling = ref(false)

const checks = computed(() => {
  const r = report.value
  return [
    {
      key: 'ssh',
      icon: 'terminal',
      title: 'SSH 客户端',
      desc: r?.preferred?.label || '双源探测：系统原生 OpenSSH / Git 内置 OpenSSH',
      ok: !!r?.preferred,
      value: r?.preferred?.version || '未检测到',
      hint: r?.preferred
        ? r.preferred.detail || '可直接调用 ssh / ssh-keygen / ssh-add'
        : '需要安装 OpenSSH 客户端后才能生成密钥与测试连接',
    },
    {
      key: 'git',
      icon: 'branch',
      title: 'Git',
      desc: '用于 Git 全局身份与按域名指定密钥',
      ok: !!r?.git?.available,
      value: r?.git?.version || '未检测到',
      hint: r?.git?.available
        ? r.git.userName
          ? `已配置身份：${r.git.userName}`
          : '未配置全局 user.name'
        : '未安装 Git，仅影响 Git 相关能力，不影响密钥管理',
    },
    {
      key: 'dir',
      icon: 'folder',
      title: '~/.ssh 目录',
      desc: '密钥与 config 的唯一存放位置',
      ok: !!r?.sshDir?.exists,
      value: r?.sshDir?.exists ? r.sshDir.permDisplay || '已就绪' : '不存在',
      hint: r?.sshDir?.created
        ? '本次已为你创建该目录并设置 700 权限'
        : r?.sshDir?.permCompliant
          ? `权限合规（${r.sshDir.permDisplay}）`
          : '权限过宽，建议到安全中心一键修复',
    },
    {
      key: 'agent',
      icon: 'shield',
      title: 'ssh-agent',
      desc: '托管带口令的密钥，避免重复输入',
      ok: !!r?.agent?.running,
      value: r?.agent?.identities?.length
        ? `已加载 ${r.agent.identities.length} 把`
        : r?.agent?.running
          ? '已运行'
          : '未运行',
      hint: r?.agent?.running
        ? r.agent.detail || 'agent 可用'
        : '可选：启用后带口令密钥无需重复输入',
    },
  ]
})

/** 逐项点亮，营造「正在检测」的即时反馈（真实结果随后覆盖） */
async function runDetect() {
  detecting.value = true
  detectError.value = null
  progress.value = { ssh: 1, git: 0, dir: 0, agent: 0 }

  const timers = []
  const tick = (key, delay) =>
    timers.push(
      setTimeout(() => {
        progress.value[key] = 2
        const order = ['ssh', 'git', 'dir', 'agent']
        const next = order[order.indexOf(key) + 1]
        if (next) progress.value[next] = 1
      }, delay)
    )
  tick('ssh', 320)
  tick('git', 640)
  tick('dir', 900)
  tick('agent', 1160)

  try {
    const r = await api.detectEnvironment(true)
    report.value = r
    // 等动画播完再展示结论，避免闪烁
    setTimeout(() => {
      detecting.value = false
      progress.value = { ssh: 2, git: 2, dir: 2, agent: 2 }
      store.rawEnv.report = r
      store.refreshEnv().catch(() => {})
    }, 1400)
  } catch (e) {
    timers.forEach(clearTimeout)
    detectError.value = e
    detecting.value = false
    // 失败时不能停在「检测中」：四项统一标记为检测失败
    progress.value = { ssh: -1, git: -1, dir: -1, agent: -1 }
  }
}

async function loadGuide() {
  try {
    guide.value = await api.getInstallGuide()
    showGuide.value = true
  } catch (e) {
    toast.push(e.message || '获取安装引导失败', 'danger')
  }
}

async function enableOpenSsh() {
  enabling.value = true
  try {
    const msg = await api.enableWindowsOpenSsh()
    toast.push(msg || '已启用系统 OpenSSH 客户端', 'success')
    showGuide.value = false
    await runDetect()
  } catch (e) {
    toast.push(e.message || '启用失败', 'danger')
  } finally {
    enabling.value = false
  }
}

/* ── 第 2 步：导入已有密钥 ── */
const scanning = ref(false)
const keys = ref([])
const selected = ref(new Set())
const scanError = ref(null)

async function runScan() {
  scanning.value = true
  scanError.value = null
  try {
    keys.value = (await api.scanKeys()) || []
    // 默认全选：用户已有的密钥理应被纳入管理视野
    selected.value = new Set(keys.value.map((k) => k.filename))
  } catch (e) {
    scanError.value = e
    keys.value = []
  } finally {
    scanning.value = false
  }
}

function toggle(filename) {
  const s = new Set(selected.value)
  s.has(filename) ? s.delete(filename) : s.add(filename)
  selected.value = s
}

const allSelected = computed(
  () => keys.value.length > 0 && selected.value.size === keys.value.length
)

function toggleAll() {
  selected.value = allSelected.value
    ? new Set()
    : new Set(keys.value.map((k) => k.filename))
}

/* ── 第 3 步：完成 ── */
const finishing = ref(false)

async function finish() {
  finishing.value = true
  try {
    await store.markOnboarded()
    // 只在用户真正勾选了密钥时才留痕，避免无意义记录
    if (selected.value.size > 0) {
      await api
        .appendAudit(
          'key',
          'key.import',
          `引导阶段纳入 ${selected.value.size} 把已有密钥的管理范围`,
          'ok',
          [...selected.value].join(', ')
        )
        .catch(() => {})
    }
    toast.push('环境已就绪，开始使用墨钥', 'success')
    router.replace('/key-vault')
  } catch (e) {
    toast.push(e.message || '初始化失败', 'danger')
  } finally {
    finishing.value = false
  }
}

async function skip() {
  await store.markOnboarded()
  router.replace('/key-vault')
}

/* ── 步骤切换 ── */
function next() {
  if (step.value === 1) {
    step.value = 2
    runScan()
  } else if (step.value === 2) {
    step.value = 3
  }
}

function prev() {
  if (step.value > 1) step.value -= 1
}

const canNext = computed(() => {
  if (step.value === 1) return !detecting.value && !detectError.value
  return true
})

let timer = null
onMounted(() => {
  runDetect()
  timer = null
})
onBeforeUnmount(() => {
  if (timer) clearTimeout(timer)
})
</script>

<template>
  <div class="onb">
    <!-- 顶部品牌区 -->
    <header class="onb-head" data-tauri-drag-region>
      <div class="brand">
        <span class="logo-seal lg"><AppIcon name="key" :size="20" /></span>
        <div class="brand-text">
          <strong>墨钥 KeyHub</strong>
          <span>SSH 密钥可视化管理 · 零命令行</span>
        </div>
      </div>
      <div class="ob-head-actions">
        <button v-if="isDesktop" class="win-btn" aria-label="最小化" title="最小化" @click="winMin">
          <AppIcon name="minimize" :size="14" :width="1.7" />
        </button>
        <button v-if="isDesktop" class="win-btn close" aria-label="关闭" title="关闭" @click="winClose">
          <AppIcon name="close" :size="13" :width="1.7" />
        </button>
        <button class="btn ghost sm" @click="skip">跳过引导</button>
      </div>
    </header>

    <!-- 步骤指示器 -->
    <nav class="steps">
      <div
        v-for="(s, i) in ['环境检测', '导入密钥', '准备就绪']"
        :key="s"
        class="step"
        :class="{ on: step === i + 1, done: step > i + 1 }"
      >
        <span class="dot">
          <AppIcon v-if="step > i + 1" name="check" :size="12" />
          <template v-else>{{ i + 1 }}</template>
        </span>
        <span class="lbl">{{ s }}</span>
        <span v-if="i < 2" class="line" />
      </div>
    </nav>

    <!-- ══════ 第 1 步 ══════ -->
    <section v-if="step === 1" class="pane">
      <div class="pane-head">
        <h1>先确认一下你的环境</h1>
        <p>
          墨钥全程调用系统原生 <code>ssh</code> / <code>ssh-keygen</code>，
          不引入任何第三方 SSH 实现，也不会上传你的任何数据。
        </p>
      </div>

      <div v-if="detectError" class="banner danger">
        <AppIcon name="warning" :size="16" />
        <div>
          <strong>无法完成环境检测</strong>
          <p>{{ detectError.message }}</p>
          <p>这不代表 OpenSSH 或 Git 缺失。可重新检测，或先跳过引导进入主界面。</p>
          <button class="btn sm" @click="runDetect">重新检测</button>
        </div>
      </div>

      <ul class="checks">
        <li v-for="c in checks" :key="c.key" class="check-item" :class="c.ok ? 'ok' : detectError ? 'unknown' : 'bad'">
          <span class="ci-icon"><AppIcon :name="c.icon" :size="18" /></span>
          <div class="ci-body">
            <div class="ci-title">
              <strong>{{ c.title }}</strong>
              <span
                class="pill"
                :class="
                  progress[c.key] === -1
                    ? 'danger'
                    : progress[c.key] < 2
                      ? 'idle'
                      : c.ok
                        ? 'ok'
                        : c.key === 'git' || c.key === 'agent'
                          ? 'idle'
                          : 'danger'
                "
              >
                <span v-if="progress[c.key] >= 0 && progress[c.key] < 2" class="spin" />
                {{ progress[c.key] === -1 ? '检测失败' : progress[c.key] < 2 ? '检测中' : c.value }}
              </span>
            </div>
            <p class="ci-desc">{{ c.desc }}</p>
            <p class="ci-hint">{{ c.hint }}</p>
          </div>
        </li>
      </ul>

      <div v-if="!detecting && report && !report.preferred" class="banner warn">
        <AppIcon name="warning" :size="16" />
        <div>
          <strong>未检测到可用的 SSH 客户端</strong>
          <p>墨钥依赖系统 OpenSSH 完成密钥生成与连接测试，请先安装。</p>
          <div class="row">
            <button class="btn primary sm" @click="loadGuide">查看安装指引</button>
            <button
              v-if="report.platform === 'windows'"
              class="btn sm"
              :disabled="enabling"
              @click="enableOpenSsh"
            >
              <span v-if="enabling" class="spin" />
              一键启用系统 OpenSSH
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- ══════ 第 2 步 ══════ -->
    <section v-else-if="step === 2" class="pane">
      <div class="pane-head">
        <h1>发现这些已有密钥</h1>
        <p>
          墨钥只<strong>读取</strong> <code>~/.ssh</code> 下已存在的文件用于展示，
          不会复制、移动或上传。勾选仅表示「希望在墨钥中看到它」。
        </p>
      </div>

      <div v-if="scanning" class="empty">
        <span class="spin lg" />
        <p>正在扫描 ~/.ssh …</p>
      </div>

      <div v-else-if="scanError" class="banner danger">
        <AppIcon name="warning" :size="16" />
        <div>
          <strong>扫描失败</strong>
          <p>{{ scanError.message }}</p>
          <button class="btn sm" @click="runScan">重试</button>
        </div>
      </div>

      <div v-else-if="keys.length === 0" class="empty">
        <AppIcon name="key" :size="28" />
        <p>还没有任何密钥</p>
        <span>没关系，下一步可以立即生成一把新的。</span>
      </div>

      <template v-else>
        <div class="list-head">
          <label class="checkbox">
            <input type="checkbox" :checked="allSelected" @change="toggleAll" />
            <span>全选（{{ selected.size }} / {{ keys.length }}）</span>
          </label>
        </div>
        <ul class="key-list">
          <li
            v-for="k in keys"
            :key="k.filename"
            class="key-row"
            :class="{ on: selected.has(k.filename) }"
            @click="toggle(k.filename)"
          >
            <label class="checkbox" @click.stop>
              <input
                type="checkbox"
                :checked="selected.has(k.filename)"
                @change="toggle(k.filename)"
              />
            </label>
            <span class="k-icon"><AppIcon name="key" :size="16" /></span>
            <div class="k-main">
              <div class="k-name">
                <strong>{{ k.filename }}</strong>
                <span class="tag">{{ k.algorithm }}</span>
                <span v-if="k.hasPassphrase" class="tag warn">有口令</span>
                <span v-if="!k.hasPublic" class="tag">无公钥</span>
              </div>
              <code class="k-fp">{{ k.fingerprint || '指纹不可用' }}</code>
            </div>
            <div class="k-meta">
              <span class="pill" :class="k.permCompliant ? 'ok' : 'warn'">
                {{ k.permDisplay || '—' }}
              </span>
              <span class="k-time">{{ (k.createdAt || '').slice(0, 10) || '—' }}</span>
            </div>
          </li>
        </ul>
      </template>
    </section>

    <!-- ══════ 第 3 步 ══════ -->
    <section v-else class="pane">
      <div class="done">
        <span class="done-seal"><AppIcon name="check" :size="30" /></span>
        <h1>一切就绪</h1>
        <p>墨钥已经了解你的环境，接下来可以：</p>
        <ul class="done-list">
          <li><AppIcon name="key" :size="15" /> 生成或管理密钥，一键复制公钥部署到服务器</li>
          <li><AppIcon name="config" :size="15" /> 可视化编辑 connections，改完自动备份</li>
          <li><AppIcon name="pulse" :size="15" /> 测试连通性，失败时给出可执行的修复建议</li>
          <li><AppIcon name="shield" :size="15" /> 体检权限与 agent 状态，守住安全底线</li>
        </ul>
        <p class="fine">
          你的密钥始终保存在 <code>~/.ssh</code>，墨钥不持有任何私钥副本，
          也不会发起未经你确认的网络请求。
        </p>
      </div>
    </section>

    <!-- 底部操作栏 -->
    <footer class="onb-foot">
      <button v-if="step > 1" class="btn ghost" @click="prev">上一步</button>
      <span v-else />
      <div class="foot-right">
        <span v-if="step === 1 && detectError" class="step-hint hint-danger">
          检测失败，可重新检测或跳过引导
        </span>
        <span class="step-hint">第 {{ step }} / 3 步</span>
        <button v-if="step < 3" class="btn primary" :disabled="!canNext" @click="next">
          下一步
        </button>
        <button v-else class="btn primary" :disabled="finishing" @click="finish">
          <span v-if="finishing" class="spin" />
          进入密钥库
        </button>
      </div>
    </footer>

    <!-- 安装指引弹窗 -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showGuide && guide" class="modal-wrap" @click.self="showGuide = false">
          <div class="modal">
            <header class="modal-head">
              <h3>{{ guide.title }}</h3>
              <button class="icon-btn" @click="showGuide = false">
                <AppIcon name="close" :size="15" />
              </button>
            </header>
            <div class="modal-body">
              <div v-for="(s, i) in guide.steps" :key="i" class="guide-step">
                <span class="gs-num">{{ i + 1 }}</span>
                <div>
                  <strong>{{ s.title }}</strong>
                  <p>{{ s.note }}</p>
                  <div v-if="s.command" class="rawbox">
                    <code>{{ s.command }}</code>
                    <button class="copy-mini" @click="useToast().copy(s.command, '命令已复制')">
                      <AppIcon name="copy" :size="13" />
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
.onb {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  padding: 0 32px 24px;
  max-width: 940px;
  margin: 0 auto;
}

.onb-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 22px 0 8px;
}
.ob-head-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}
.brand {
  display: flex;
  align-items: center;
  gap: 12px;
}
.logo-seal.lg {
  width: 38px;
  height: 38px;
  border-radius: 12px;
}
.brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.brand-text strong {
  font-size: 15px;
  color: var(--fg);
}
.brand-text span {
  font-size: 12px;
  color: var(--fg-muted);
}

/* 步骤条 */
.steps {
  display: flex;
  align-items: center;
  gap: 0;
  margin: 18px 0 26px;
}
.step {
  display: flex;
  align-items: center;
  gap: 9px;
  flex: 1;
}
.step:last-child {
  flex: 0 0 auto;
}
.dot {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  font-size: 12px;
  font-family: var(--font-mono);
  background: var(--surface-warm);
  color: var(--fg-muted);
  border: 1px solid var(--border);
  transition: all 0.25s ease;
  flex: 0 0 auto;
}
.step.on .dot {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 18%, transparent);
}
.step.done .dot {
  background: var(--success);
  color: #fff;
  border-color: var(--success);
}
.lbl {
  font-size: 13px;
  color: var(--fg-muted);
  white-space: nowrap;
}
.step.on .lbl {
  color: var(--fg);
  font-weight: 600;
}
.line {
  flex: 1;
  height: 1px;
  background: var(--border);
  margin: 0 12px;
}

.pane {
  flex: 1;
  min-height: 0;
}
.pane-head {
  margin-bottom: 20px;
}
.pane-head h1 {
  font-size: 21px;
  margin: 0 0 8px;
  color: var(--fg);
  letter-spacing: -0.01em;
}
.pane-head p {
  font-size: 13.5px;
  line-height: 1.7;
  color: var(--fg-muted);
  margin: 0;
  max-width: 640px;
}
.pane-head code {
  font-family: var(--font-mono);
  font-size: 12.5px;
  background: var(--surface-warm);
  padding: 1px 5px;
  border-radius: 5px;
  color: var(--accent);
}

/* 检测项列表 */
.checks {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.check-item {
  display: flex;
  gap: 14px;
  padding: 15px 17px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  transition: border-color 0.2s;
}
.check-item.bad {
  border-color: color-mix(in srgb, var(--warn) 34%, var(--border));
}
.check-item.ok {
  border-color: color-mix(in srgb, var(--success) 26%, var(--border));
}
.check-item.unknown {
  border-color: var(--border);
}
.ci-icon {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  background: var(--surface-warm);
  color: var(--fg-muted);
  flex: 0 0 auto;
}
.check-item.ok .ci-icon {
  background: var(--success-soft);
  color: var(--success-ink);
}
.ci-body {
  flex: 1;
  min-width: 0;
}
.ci-title {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 3px;
}
.ci-title strong {
  font-size: 14px;
  color: var(--fg);
}
.ci-desc {
  font-size: 12.5px;
  color: var(--fg-muted);
  margin: 0 0 3px;
}
.ci-hint {
  font-size: 12px;
  color: var(--fg-faint);
  margin: 0;
  line-height: 1.55;
}

/* 密钥列表 */
.list-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 4px 10px;
}
.checkbox {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--fg-muted);
  cursor: pointer;
  user-select: none;
}
.checkbox input {
  width: 15px;
  height: 15px;
  accent-color: var(--accent);
  cursor: pointer;
}
.key-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.key-row {
  display: flex;
  align-items: center;
  gap: 13px;
  padding: 12px 15px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all 0.16s;
}
.key-row:hover {
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
}
.key-row.on {
  border-color: color-mix(in srgb, var(--accent) 55%, var(--border));
  background: color-mix(in srgb, var(--accent) 5%, var(--surface));
}
.k-icon {
  width: 30px;
  height: 30px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--accent-soft, color-mix(in srgb, var(--accent) 10%, var(--surface)));
  color: var(--accent);
  flex: 0 0 auto;
}
.k-main {
  flex: 1;
  min-width: 0;
}
.k-name {
  display: flex;
  align-items: center;
  gap: 7px;
  flex-wrap: wrap;
  margin-bottom: 3px;
}
.k-name strong {
  font-size: 13.5px;
  color: var(--fg);
}
.k-fp {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg-faint);
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.k-meta {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
  flex: 0 0 auto;
}
.k-time {
  font-size: 11px;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}

/* 完成页 */
.done {
  text-align: center;
  padding: 34px 0;
  max-width: 560px;
  margin: 0 auto;
}
.done-seal {
  width: 68px;
  height: 68px;
  border-radius: 22px;
  display: inline-grid;
  place-items: center;
  background: var(--success-soft);
  color: var(--success-ink);
  margin-bottom: 18px;
}
.done h1 {
  font-size: 23px;
  margin: 0 0 10px;
  color: var(--fg);
}
.done > p {
  font-size: 13.5px;
  color: var(--fg-muted);
  margin: 0 0 20px;
}
.done-list {
  list-style: none;
  padding: 0;
  margin: 0 0 20px;
  text-align: left;
  display: flex;
  flex-direction: column;
  gap: 11px;
}
.done-list li {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  font-size: 13px;
  color: var(--fg);
  padding: 11px 14px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  line-height: 1.55;
}
.done-list li :deep(svg) {
  color: var(--accent);
  flex: 0 0 auto;
  margin-top: 1px;
}
.fine {
  font-size: 12px;
  color: var(--fg-faint);
  line-height: 1.7;
}
.fine code {
  font-family: var(--font-mono);
  background: var(--surface-warm);
  padding: 1px 5px;
  border-radius: 5px;
}

/* 底部栏 */
.onb-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-top: 22px;
  margin-top: 22px;
  border-top: 1px solid var(--border);
}
.foot-right {
  display: flex;
  align-items: center;
  gap: 14px;
}
.step-hint {
  font-size: 12px;
  color: var(--fg-faint);
  font-family: var(--font-mono);
}
.step-hint.hint-danger {
  color: var(--danger-ink);
  font-family: var(--font-body);
}

/* 空态 */
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 56px 20px;
  color: var(--fg-muted);
  text-align: center;
}
.empty > :deep(svg) {
  color: var(--fg-faint);
}
.empty p {
  margin: 0;
  font-size: 14px;
  color: var(--fg);
}
.empty span {
  font-size: 12.5px;
  color: var(--fg-faint);
}

/* 引导步骤 */
.guide-step {
  display: flex;
  gap: 13px;
  padding: 13px 0;
  border-bottom: 1px dashed var(--border);
}
.guide-step:last-child {
  border-bottom: none;
}
.gs-num {
  width: 22px;
  height: 22px;
  border-radius: 7px;
  background: var(--surface-warm);
  color: var(--fg-muted);
  display: grid;
  place-items: center;
  font-size: 11.5px;
  font-family: var(--font-mono);
  flex: 0 0 auto;
}
.guide-step strong {
  font-size: 13.5px;
  color: var(--fg);
  display: block;
  margin-bottom: 4px;
}
.guide-step p {
  font-size: 12.5px;
  color: var(--fg-muted);
  margin: 0 0 8px;
  line-height: 1.6;
}

.icon-btn {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--fg-muted);
  display: grid;
  place-items: center;
  cursor: pointer;
}
.icon-btn:hover {
  color: var(--fg);
  border-color: var(--fg-faint);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.18s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
