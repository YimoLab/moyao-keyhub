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
import Icon from '../design/vue/ui/Icon.vue'
import PageHead from '../design/vue/ui/PageHead.vue'
import AlertBox from '../design/vue/ui/AlertBox.vue'

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
  <div class="kh-page">
    <PageHead
      title="环境自检"
      desc="检测 SSH 客户端、Git、~/.ssh 目录与 ssh-agent 的就绪状态。全程只读，不改动任何文件。"
    >
      <template #actions>
        <button class="anx-btn anx-btn--ghost anx-btn--sm" :disabled="detecting" @click="runDetect">
          <span v-if="detecting" class="anx-btn__spinner" />
          <Icon v-else name="refresh" :size="15" />
          {{ detecting ? '检测中…' : '重新检测' }}
        </button>
      </template>
    </PageHead>

    <AlertBox
      v-if="allOk && report"
      tone="ok"
      text="全部就绪，当前环境可以正常使用墨钥的全部功能。"
    />

    <AlertBox
      v-if="detectError"
      tone="danger"
      title="无法完成环境检测"
      :lines="[
        detectError.message,
        '这不代表 OpenSSH 或 Git 缺失，请确认桌面后端已连接后重新检测。',
      ]"
    />

    <div class="kh-env-grid">
      <section
        v-for="c in checks"
        :key="c.key"
        class="anx-panel kh-envcard"
        :class="detecting ? 'kh-envcard--checking' : c.ok === null ? 'kh-envcard--unknown' : c.ok ? 'kh-envcard--ok' : c.key === 'git' || c.key === 'agent' ? 'kh-envcard--unknown' : 'kh-envcard--bad'"
      >
        <div class="anx-panel__head kh-envcard__head">
          <span class="kh-envcard__icon"><Icon :name="c.icon" :size="18" /></span>
          <div class="kh-grow">
            <h2>{{ c.title }}</h2>
            <p>{{ c.desc }}</p>
          </div>
          <span
            class="anx-badge"
            :class="
              detecting
                ? 'anx-badge--neutral'
                : c.ok === null
                  ? 'anx-badge--neutral'
                  : c.ok
                    ? 'anx-badge--ok'
                    : c.key === 'git' || c.key === 'agent'
                      ? 'anx-badge--neutral'
                      : 'anx-badge--danger'
            "
          >
            <span v-if="detecting" class="anx-btn__spinner" />
            {{ detecting ? '检测中' : c.value }}
          </span>
        </div>

        <div class="anx-panel__body kh-envcard__body">
          <p class="anx-t-sm anx-t-muted">{{ c.hint }}</p>

          <!-- 就地修复入口：只有明确判定为「不合规」时才出现 -->
          <div v-if="!detecting && c.ok === false && c.key === 'ssh'" class="kh-actions">
            <button class="anx-btn anx-btn--sm" @click="loadGuide">查看安装指引</button>
            <button
              v-if="report?.platform === 'windows'"
              class="anx-btn anx-btn--primary anx-btn--sm"
              :disabled="enabling"
              @click="enableOpenSsh"
            >
              <span v-if="enabling" class="anx-btn__spinner" />
              一键启用系统 OpenSSH
            </button>
          </div>
          <div v-if="!detecting && c.ok === false && c.key === 'dir'" class="kh-actions">
            <button class="anx-btn anx-btn--sm" @click="router.push({ name: 'security' })">
              去安全中心修复权限
            </button>
          </div>
          <div v-if="!detecting && c.ok === false && c.key === 'agent'" class="kh-actions">
            <button class="anx-btn anx-btn--sm" @click="router.push({ name: 'security' })">
              前往安全中心
            </button>
          </div>
        </div>
      </section>
    </div>

    <!-- 安装指引 -->
    <section v-if="showGuide && guide" class="anx-panel">
      <div class="anx-panel__head">
        <div><h2>OpenSSH 安装指引</h2></div>
        <div class="anx-panel__head-actions">
          <button class="anx-iconbtn" aria-label="关闭" @click="showGuide = false">
            <Icon name="close" :size="16" />
          </button>
        </div>
      </div>
      <div class="anx-panel__body">
        <pre class="kh-pre">{{ guide }}</pre>
      </div>
    </section>
  </div>
</template>
