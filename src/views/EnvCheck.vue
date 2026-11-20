<script setup>
/**
 * 环境自检（独立页面，区别于首次引导 Onboarding）
 * 随时可以从侧栏进来重新体检：SSH / Git / ~/.ssh 权限 / ssh-agent 四项。
 * 只读检测（不创建目录），修复动作交给安全中心或本页的一键启用。
 */
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import * as api from '../lib/api'
import { useEnvStore } from '../lib/store'
import { useToast } from '../lib/toast'
import AppIcon from '../components/AppIcon.vue'

const router = useRouter()
const toast = useToast()
const store = useEnvStore()

const detecting = ref(false)
const report = ref(null)
const detectError = ref(null)

const guide = ref(null)
const showGuide = ref(false)
const enabling = ref(false)

const checks = computed(() => {
  const r = report.value
  // 探测失败 ≠ 环境缺失。失败时所有项进入 unknown，不给出任何结论。
  const known = !!r && !detectError.value
  const unknownHint = '后端未连接，暂时无法评估'
  return [
    {
      key: 'ssh',
      icon: 'terminal',
      title: 'SSH 客户端',
      desc: '双源探测：系统原生 OpenSSH / Git 内置 OpenSSH',
      ok: known ? !!r?.preferred : null,
      value: known ? r?.preferred?.version || '未检测到' : '无法检测',
      hint: !known
        ? unknownHint
        : r?.preferred
          ? r.preferred.detail || '可直接调用 ssh / ssh-keygen / ssh-add'
          : '需要安装 OpenSSH 客户端后才能生成密钥与测试连接',
    },
    {
      key: 'git',
      icon: 'branch',
      title: 'Git',
      desc: '用于 Git 全局身份与按域名指定密钥',
      ok: known ? !!r?.git?.available : null,
      value: known ? r?.git?.version || '未检测到' : '无法检测',
      hint: !known
        ? unknownHint
        : r?.git?.available
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
      ok: known ? !!r?.sshDir?.exists && !!r?.sshDir?.permCompliant : null,
      value: known ? (r?.sshDir?.exists ? r.sshDir.permDisplay || '已就绪' : '不存在') : '无法检测',
      hint: !known
        ? unknownHint
        : r?.sshDir?.permCompliant
          ? `权限合规（${r.sshDir.permDisplay}）`
          : '权限过宽，建议到安全中心一键修复',
    },
    {
      key: 'agent',
      icon: 'shield',
      title: 'ssh-agent',
      desc: '托管带口令的密钥，避免重复输入',
      ok: known ? !!r?.agent?.running : null,
      value: known
        ? r?.agent?.identities?.length
          ? `已加载 ${r.agent.identities.length} 把`
          : r?.agent?.running
            ? '已运行'
            : '未运行'
        : '无法检测',
      hint: !known
        ? unknownHint
        : r?.agent?.running
          ? r.agent.detail || 'agent 可用'
          : '可选：启用后带口令密钥无需重复输入',
    },
  ]
})

const allOk = computed(() => checks.value.every((c) => c.ok))

/** 只读自检：不创建目录、不改动任何文件 */
async function runDetect() {
  detecting.value = true
  detectError.value = null
  try {
    const r = await api.detectEnvironment(false)
    report.value = r
    store.rawEnv.report = r
    store.refreshEnv().catch(() => {})
  } catch (e) {
    detectError.value = e
  } finally {
    detecting.value = false
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

onMounted(runDetect)
</script>

<template>
  <div class="envcheck">
    <header class="ec-head">
      <div>
        <h1>环境自检</h1>
        <p class="ec-sub">
          检测 SSH 客户端、Git、~/.ssh 目录与 ssh-agent 的就绪状态。全程只读，不改动任何文件。
        </p>
      </div>
      <button class="btn sm" :disabled="detecting" @click="runDetect">
        <span v-if="detecting" class="spin" />
        <AppIcon v-else name="refresh" :size="13" />
        {{ detecting ? '检测中…' : '重新检测' }}
      </button>
    </header>

    <div v-if="allOk && report" class="banner ok-banner">
      <AppIcon name="check" :size="16" />
      <span>全部就绪，当前环境可以正常使用墨钥的全部功能。</span>
    </div>

    <div v-if="detectError" class="banner danger ec-banner">
      <AppIcon name="warning" :size="16" />
      <div>
        <strong>无法完成环境检测</strong>
        <p>{{ detectError.message }}</p>
        <p>这不代表 OpenSSH 或 Git 缺失，请确认桌面后端已连接后重新检测。</p>
      </div>
    </div>

    <div class="ec-grid">
      <div
        v-for="c in checks"
        :key="c.key"
        class="ec-card"
        :class="detecting ? 'checking' : c.ok === null ? 'unknown' : c.ok ? 'ok' : 'bad'"
      >
        <div class="ec-top">
          <span class="ec-ico"><AppIcon :name="c.icon" :size="17" /></span>
          <strong>{{ c.title }}</strong>
          <span class="pill" :class="detecting ? 'idle' : c.ok === null ? 'idle' : c.ok ? 'ok' : c.key === 'git' || c.key === 'agent' ? 'idle' : 'danger'">
            <span v-if="detecting" class="spin" />
            {{ detecting ? '检测中' : c.value }}
          </span>
        </div>
        <p class="ec-desc">{{ c.desc }}</p>
        <p class="ec-hint">{{ c.hint }}</p>

        <!-- 就地修复入口：只有明确判定为「不合规」时才出现 -->
        <div v-if="!detecting && c.ok === false && c.key === 'ssh'" class="ec-fix">
          <button class="btn sm" @click="loadGuide">查看安装指引</button>
          <button
            v-if="report?.platform === 'windows'"
            class="btn sm"
            :disabled="enabling"
            @click="enableOpenSsh"
          >
            <span v-if="enabling" class="spin" />
            一键启用系统 OpenSSH
          </button>
        </div>
        <div v-if="!detecting && c.ok === false && c.key === 'dir'" class="ec-fix">
          <button class="btn sm" @click="router.push({ name: 'security' })">
            去安全中心修复权限
          </button>
        </div>
        <div v-if="!detecting && c.ok === false && c.key === 'agent'" class="ec-fix">
          <button class="btn sm" @click="router.push({ name: 'security' })">前往安全中心</button>
        </div>
      </div>
    </div>

    <!-- 安装指引 -->
    <div v-if="showGuide && guide" class="ec-guide">
      <div class="ecg-head">
        <strong>OpenSSH 安装指引</strong>
        <button class="icon-btn" aria-label="关闭" @click="showGuide = false">
          <AppIcon name="close" :width="1.8" />
        </button>
      </div>
      <pre class="ecg-body">{{ guide }}</pre>
    </div>
  </div>
</template>

<style scoped>
.envcheck {
  max-width: 980px;
}
.ec-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 18px;
}
.ec-head h1 {
  margin: 0 0 4px;
  font-size: 19px;
}
.ec-sub {
  margin: 0;
  font-size: 12.5px;
  color: var(--fg-faint);
  line-height: 1.6;
}
.ok-banner,
.ec-banner {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 11px 14px;
  border-radius: var(--radius-md);
  font-size: 13px;
  margin-bottom: 16px;
}
.ok-banner {
  background: var(--success-soft, color-mix(in srgb, #22c55e 12%, transparent));
  color: var(--success-ink, #15803d);
}
.ec-banner {
  background: color-mix(in srgb, var(--danger) 9%, transparent);
  color: var(--danger);
  align-items: flex-start;
}
.ec-banner p {
  margin: 3px 0 0;
  font-size: 12px;
}
.ec-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
  gap: 13px;
}
.ec-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 15px 16px;
  display: flex;
  flex-direction: column;
  gap: 7px;
  border-left: 3px solid var(--border);
}
.ec-card.ok {
  border-left-color: var(--success);
}
.ec-card.bad {
  border-left-color: var(--danger);
}
.ec-card.unknown {
  border-left-color: var(--border);
}
.ec-card.checking {
  border-left-color: var(--accent);
}
.ec-top {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ec-top strong {
  font-size: 13.5px;
  flex: 1;
}
.ec-ico {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: var(--surface-warm);
  color: var(--fg-muted);
  flex: 0 0 auto;
}
.ec-card.ok .ec-ico {
  color: #16a34a;
}
.ec-card.bad .ec-ico {
  color: var(--danger);
}
.ec-desc {
  margin: 0;
  font-size: 11.5px;
  color: var(--fg-muted);
}
.ec-hint {
  margin: 0;
  font-size: 11.5px;
  color: var(--fg-faint);
  line-height: 1.55;
  flex: 1;
}
.ec-fix {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 4px;
}
.ec-guide {
  margin-top: 18px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.ecg-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 15px;
  border-bottom: 1px solid var(--border);
  font-size: 13px;
}
.ecg-body {
  margin: 0;
  padding: 14px 16px;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--fg);
  max-height: 320px;
  overflow: auto;
}
</style>
