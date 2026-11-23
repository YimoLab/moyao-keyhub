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
import Icon from '../design/vue/ui/Icon.vue'
import BrandMark from '../components/BrandMark.vue'
import AlertBox from '../design/vue/ui/AlertBox.vue'
import EmptyState from '../design/vue/ui/EmptyState.vue'
import AppModal from '../components/AppModal.vue'

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
  <div class="kh-onb">
    <!-- 顶部品牌区 -->
    <header class="kh-onb__head" data-tauri-drag-region>
      <div class="kh-onb__brand">
        <span class="kh-brand-mark"><BrandMark /></span>
        <div>
          <div class="anx-t-strong">墨钥 KeyHub</div>
          <div class="anx-t-xs anx-t-muted">SSH 密钥可视化管理 · 零命令行</div>
        </div>
      </div>
      <div class="kh-onb__head-actions">
        <button
          v-if="isDesktop"
          class="anx-iconbtn"
          aria-label="最小化"
          title="最小化"
          @click="winMin"
        >
          <Icon name="minimize" :size="15" />
        </button>
        <button
          v-if="isDesktop"
          class="anx-iconbtn kh-win-btn--close"
          aria-label="关闭"
          title="关闭"
          @click="winClose"
        >
          <Icon name="close" :size="15" />
        </button>
        <button class="anx-btn anx-btn--ghost anx-btn--sm" @click="skip">跳过引导</button>
      </div>
    </header>

    <div class="kh-onb__body">
      <!-- 步骤指示器 -->
      <nav class="kh-steps" aria-label="引导步骤">
        <div
          v-for="(s, i) in ['环境检测', '导入密钥', '准备就绪']"
          :key="s"
          class="kh-step"
          :class="{ 'is-on': step === i + 1, 'is-done': step > i + 1 }"
          :aria-current="step === i + 1 ? 'step' : undefined"
        >
          <span class="kh-step__dot">
            <Icon v-if="step > i + 1" name="check" :size="13" />
            <template v-else>{{ i + 1 }}</template>
          </span>
          <span class="kh-step__label">{{ s }}</span>
          <span v-if="i < 2" class="kh-step__line" />
        </div>
      </nav>

      <!-- ══════ 第 1 步 ══════ -->
      <section v-if="step === 1" class="anx-panel kh-onb__pane">
        <div class="anx-panel__body kh-onb__pane-body">
          <div class="kh-onb__intro">
            <h1 class="anx-t-h1">先确认一下你的环境</h1>
            <p class="anx-t-sm anx-t-muted">
              墨钥全程调用系统原生 <code class="kh-code-inline">ssh</code> /
              <code class="kh-code-inline">ssh-keygen</code>，不引入任何第三方 SSH 实现，也不会上传你的任何数据。
            </p>
          </div>

          <AlertBox
            v-if="detectError"
            tone="danger"
            title="无法完成环境检测"
            :lines="[
              detectError.message,
              '这不代表 OpenSSH 或 Git 缺失。可重新检测，或先跳过引导进入主界面。',
            ]"
          />

          <ul class="kh-list">
            <li
              v-for="c in checks"
              :key="c.key"
              class="kh-list__item kh-onb-check"
              :class="c.ok ? 'is-ok' : detectError ? 'is-unknown' : c.key === 'git' || c.key === 'agent' ? 'is-unknown' : 'is-bad'"
            >
              <span class="kh-onb-check__icon"><Icon :name="c.icon" :size="18" /></span>
              <div class="kh-list__body">
                <div class="kh-list__title">
                  {{ c.title }}
                  <span
                    class="anx-badge"
                    :class="
                      progress[c.key] === -1
                        ? 'anx-badge--danger'
                        : progress[c.key] < 2
                          ? 'anx-badge--neutral'
                          : c.ok
                            ? 'anx-badge--ok'
                            : c.key === 'git' || c.key === 'agent'
                              ? 'anx-badge--neutral'
                              : 'anx-badge--danger'
                    "
                  >
                    <span v-if="progress[c.key] >= 0 && progress[c.key] < 2" class="anx-btn__spinner" />
                    {{ progress[c.key] === -1 ? '检测失败' : progress[c.key] < 2 ? '检测中' : c.value }}
                  </span>
                </div>
                <p class="kh-list__text">{{ c.desc }}</p>
                <p class="kh-list__text kh-fine">{{ c.hint }}</p>
              </div>
            </li>
          </ul>

          <div v-if="!detecting && report && !report.preferred" class="anx-alert anx-alert--warn">
            <Icon name="alert-triangle" :size="18" />
            <div class="anx-alert__body">
              <p class="anx-alert__title">未检测到可用的 SSH 客户端</p>
              <p class="anx-alert__text">墨钥依赖系统 OpenSSH 完成密钥生成与连接测试，请先安装。</p>
              <div class="kh-actions kh-mt-sm">
                <button class="anx-btn anx-btn--primary anx-btn--sm" @click="loadGuide">查看安装指引</button>
                <button
                  v-if="report.platform === 'windows'"
                  class="anx-btn anx-btn--sm"
                  :disabled="enabling"
                  @click="enableOpenSsh"
                >
                  <span v-if="enabling" class="anx-btn__spinner" />
                  一键启用系统 OpenSSH
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ══════ 第 2 步 ══════ -->
      <section v-else-if="step === 2" class="anx-panel kh-onb__pane">
        <div class="anx-panel__body kh-onb__pane-body">
          <div class="kh-onb__intro">
            <h1 class="anx-t-h1">发现这些已有密钥</h1>
            <p class="anx-t-sm anx-t-muted">
              墨钥只<strong>读取</strong> <code class="kh-code-inline">~/.ssh</code>
              下已存在的文件用于展示，不会复制、移动或上传。勾选仅表示「希望在墨钥中看到它」。
            </p>
          </div>

          <div v-if="scanning" class="kh-sk-rows">
            <span v-for="i in 4" :key="i" class="anx-sk kh-sk-row" />
          </div>

          <AlertBox
            v-else-if="scanError"
            tone="danger"
            title="扫描失败"
            :text="scanError.message"
          />

          <EmptyState
            v-else-if="keys.length === 0"
            icon="key"
            title="还没有任何密钥"
            text="没关系，下一步可以立即生成一把新的。"
          />

          <template v-else>
            <label class="anx-checkline">
              <input type="checkbox" :checked="allSelected" @change="toggleAll" />
              <span class="anx-checkline__text">
                全选（{{ selected.size }} / {{ keys.length }}）
              </span>
            </label>

            <ul class="kh-keypick">
              <li
                v-for="k in keys"
                :key="k.filename"
                class="kh-keypick__row"
                :class="{ 'is-on': selected.has(k.filename) }"
                @click="toggle(k.filename)"
              >
                <label class="anx-checkline" @click.stop>
                  <input
                    type="checkbox"
                    :checked="selected.has(k.filename)"
                    @change="toggle(k.filename)"
                  />
                  <span class="anx-visually-hidden">选择 {{ k.filename }}</span>
                </label>
                <span class="kh-host-item__ico"><Icon name="key" :size="16" /></span>
                <div class="kh-grow">
                  <div class="kh-actions">
                    <strong class="kh-mono">{{ k.filename }}</strong>
                    <span class="anx-tag">{{ k.algorithm }}</span>
                    <span v-if="k.hasPassphrase" class="anx-tag anx-tag--warn">有口令</span>
                    <span v-if="!k.hasPublic" class="anx-tag">无公钥</span>
                  </div>
                  <code class="kh-onb__fp">{{ k.fingerprint || '指纹不可用' }}</code>
                </div>
                <div class="kh-keypick__meta">
                  <span class="anx-badge" :class="k.permCompliant ? 'anx-badge--ok' : 'anx-badge--warn'">
                    {{ k.permDisplay || '—' }}
                  </span>
                  <span class="anx-t-xs anx-t-muted">{{ (k.createdAt || '').slice(0, 10) || '—' }}</span>
                </div>
              </li>
            </ul>
          </template>
        </div>
      </section>

      <!-- ══════ 第 3 步 ══════ -->
      <section v-else class="anx-panel kh-onb__pane">
        <div class="anx-panel__body kh-onb__pane-body kh-onb__done">
          <span class="kh-onb__seal"><Icon name="check" :size="30" /></span>
          <h1 class="anx-t-h1">一切就绪</h1>
          <p class="anx-t-sm anx-t-muted">墨钥已经了解你的环境，接下来可以：</p>
          <ul class="kh-list">
            <li class="kh-list__item">
              <Icon name="key" :size="16" />
              <span>生成或管理密钥，一键复制公钥部署到服务器</span>
            </li>
            <li class="kh-list__item">
              <Icon name="file-text" :size="16" />
              <span>可视化编辑 connections，改完自动备份</span>
            </li>
            <li class="kh-list__item">
              <Icon name="activity" :size="16" />
              <span>测试连通性，失败时给出可执行的修复建议</span>
            </li>
            <li class="kh-list__item">
              <Icon name="shield" :size="16" />
              <span>体检权限与 agent 状态，守住安全底线</span>
            </li>
          </ul>
          <p class="kh-fine">
            你的密钥始终保存在 <code class="kh-code-inline">~/.ssh</code>，墨钥不持有任何私钥副本，
            也不会发起未经你确认的网络请求。
          </p>
        </div>
      </section>

      <!-- 底部操作栏 -->
      <footer class="kh-onb__foot">
        <button v-if="step > 1" class="anx-btn anx-btn--ghost" @click="prev">上一步</button>
        <span v-else />
        <div class="kh-actions">
          <span v-if="step === 1 && detectError" class="kh-fine kh-danger-text">
            检测失败，可重新检测或跳过引导
          </span>
          <span class="kh-fine">第 {{ step }} / 3 步</span>
          <button v-if="step < 3" class="anx-btn anx-btn--primary" :disabled="!canNext" @click="next">
            下一步
          </button>
          <button v-else class="anx-btn anx-btn--primary" :disabled="finishing" @click="finish">
            <span v-if="finishing" class="anx-btn__spinner" />
            进入密钥库
          </button>
        </div>
      </footer>
    </div>

    <!-- 安装指引弹窗 -->
    <AppModal v-model="showGuide" :title="guide?.title || 'OpenSSH 安装指引'">
      <div class="kh-guide">
        <div v-for="(s, i) in guide?.steps || []" :key="i" class="kh-guide__step">
          <span class="kh-guide__num">{{ i + 1 }}</span>
          <div class="kh-grow">
            <strong>{{ s.title }}</strong>
            <p class="anx-t-xs anx-t-muted">{{ s.note }}</p>
            <div v-if="s.command" class="kh-cmd">
              <code class="anx-code anx-code--wrap">{{ s.command }}</code>
              <button
                class="anx-copybtn kh-cmd__copy"
                aria-label="复制命令"
                @click="toast.copy(s.command, '命令已复制')"
              >
                <Icon name="copy" :size="14" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </AppModal>
  </div>
</template>
