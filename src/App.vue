<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import Icon from './design/vue/ui/Icon.vue'
import RouteFade from './design/vue/ui/RouteFade.vue'
import PageSkeleton from './components/PageSkeleton.vue'
import OverlayHost from './design/vue/overlay/OverlayHost.vue'
import ToastHost from './design/vue/overlay/ToastHost.vue'
import WindowControls from './components/WindowControls.vue'
import BrandMark from './components/BrandMark.vue'
import UpdateDialog from './components/UpdateDialog.vue'
import { useTheme } from './lib/theme.js'
import { maybeAutoCheck } from './lib/updater.js'
import { toast } from './lib/toast.js'
import * as api from './lib/api.js'
import { useEnvStore } from './lib/store.js'

const route = useRoute()
const router = useRouter()
const { theme, toggleTheme } = useTheme()
const store = useEnvStore()
const env = store.env
const derived = store.derived

/** 侧栏环境卡的 agent 值：只说结论，完整状态放 title，避免窄栏里被截断 */
const agentShort = computed(() => {
  if (env.error) return '未知'
  if (!env.agentRunning) return '未运行'
  return env.agentCount > 0 ? `已加载 ${env.agentCount} 把` : '未加载密钥'
})

const isBare = computed(() => route.meta.bare === true)

/* ══════ 标题栏风格 ══════
 * 两种都是自绘，窗口保持无边框：
 *   windows — 最小化 / 最大化 / 关闭在右上角
 *   mac     — 红黄绿交通灯在左上角
 * 偏好 titlebarStyle：auto（Windows/Linux 用 windows，macOS 用 mac）| windows | mac。
 * 风格在启动时冻结；设置页切换后提示重启，重启才会重新解析。
 * ?chrome=mac|windows 仅用于开发预览，不写入偏好。 */
const isMacOS = /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent)
const bootChrome = ref('')

function resolveChrome() {
  const pref = store.rawPrefs.titlebarStyle || 'auto'
  if (pref === 'mac') return 'mac'
  if (pref === 'windows' || pref === 'native') return 'windows'
  return isMacOS ? 'mac' : 'windows'
}

const chrome = computed(() => {
  const preview = route.query.chrome
  if (preview === 'mac' || preview === 'windows') return preview
  return bootChrome.value || 'windows'
})
const searchValue = ref('')
const searchInput = ref(null)
const navOpen = ref(false)
/** 快捷键标签：macOS 用 ⌘K，其它平台用 Ctrl K */
const searchKbd = /Mac/i.test(navigator.userAgent) ? '⌘K' : 'Ctrl K'

/* 桌面端使用无边框窗口，标题栏由前端绘制；浏览器预览下不显示窗口按钮 */
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
function winMax() {
  appWindow?.toggleMaximize().catch(() => {})
}
function winClose() {
  appWindow?.close().catch(() => {})
}


const NAV = [
  { name: 'key-vault', label: '密钥库', icon: 'key' },
  { name: 'config', label: 'SSH Config', icon: 'file-text' },
  { name: 'diagnostics', label: '连接测试', icon: 'activity' },
  { name: 'security', label: '安全中心', icon: 'shield' },
]

const SYS_NAV = [
  { name: 'settings', label: '设置', icon: 'sliders' },
  { name: 'env-check', label: '环境自检', icon: 'monitor' },
]

/**
 * 首次导航是否已经完成。
 * 路由守卫要先读取偏好与环境摘要（决定是否跳引导页），这段等待期间
 * RouterView 还没有组件；没有这个标志，主区域会先空白一大片再出现骨架。
 */
const routeReady = ref(false)

const pageTitle = computed(() => {
  if (!routeReady.value) return '加载中…'
  const hit = [...NAV, ...SYS_NAV].find((n) => n.name === route.name)
  return hit?.label || '墨钥 KeyHub'
})

/** 守卫阻塞时 route 还没解析，用 hash 推断目标页面，让启动骨架与页面结构一致 */
const bootVariant = computed(() => {
  const path = (window.location.hash.replace(/^#/, '') || '/key-vault').split('?')[0]
  return (
    {
      '/key-vault': 'vault',
      '/config': 'config',
      '/security': 'security',
      '/diagnostics': 'diagnostics',
      '/env-check': 'env',
      '/settings': 'settings',
    }[path] || 'generic'
  )
})

async function boot() {
  await store.refreshEnv()
}

onMounted(() => {
  // 桌面壳只保留一个窗口滚动上下文，关闭浏览器的滚动位置恢复
  if ('scrollRestoration' in history) history.scrollRestoration = 'manual'
  window.scrollTo(0, 0)
  boot()
  router.isReady().finally(() => {
    routeReady.value = true
  })
  document.addEventListener('keydown', onGlobalKey)
})

// 偏好加载完成后冻结标题栏风格（切换偏好后重启应用生效）
watch(
  () => store.rawPrefs.loaded,
  (loaded) => {
    if (!loaded || bootChrome.value) return
    bootChrome.value = resolveChrome()
  },
  { immediate: true }
)

/* ══════ 更新提示（默认关闭自动检查，首次进入主界面告知一次） ══════ */
const UPDATE_NOTICE_KEY = 'keyhub.update.notice.v1'
let updateBootDone = false

function showUpdateNoticeOnce() {
  if (store.rawPrefs.autoCheckUpdates) return
  try {
    if (localStorage.getItem(UPDATE_NOTICE_KEY)) return
    localStorage.setItem(UPDATE_NOTICE_KEY, '1')
  } catch {
    return
  }
  setTimeout(() => {
    toast('可在「设置 → 应用更新」中开启自动检查更新', 'info')
  }, 1500)
}

watch(
  [() => store.rawPrefs.loaded, isBare],
  ([loaded, bare]) => {
    if (!loaded || bare || updateBootDone || !store.rawPrefs.available) return
    updateBootDone = true
    maybeAutoCheck(store.rawPrefs)
    showUpdateNoticeOnce()
  },
  { immediate: true }
)
onBeforeUnmount(() => document.removeEventListener('keydown', onGlobalKey))

// 路由变化时收起窄屏抽屉，并把内容滚回顶部（浮层导航布局下滚动容器是窗口）
router.afterEach(() => {
  navOpen.value = false
  nextTick(() => {
    window.scrollTo(0, 0)
    document.querySelector('.anx-shell__main')?.scrollTo?.(0, 0)
    document.querySelector('.anx-shell__body')?.scrollTo?.(0, 0)
    document.querySelector('.kh-bare')?.scrollTo?.(0, 0)
  })
})

/** Ctrl/Cmd + K：聚焦全局搜索 */
function onGlobalKey(e) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    // 引导页没有外壳搜索框，不拦截
    if (!searchInput.value) return
    e.preventDefault()
    searchInput.value.focus()
    searchInput.value.select()
  }
}

// 全局搜索：回车跳到密钥库并把词带过去
function submitSearch() {
  const q = searchValue.value.trim()
  if (!q) return
  router.push({ name: 'key-vault', query: { q } })
  searchValue.value = ''
}

function onSearchKey(e) {
  if (e.key === 'Enter') submitSearch()
  if (e.key === 'Escape') {
    searchValue.value = ''
    searchInput.value?.blur()
  }
}
</script>

<template>
  <div v-if="isBare" class="kh-bare">
    <RouteFade />
  </div>

  <div v-else class="anx-shell" :class="{ 'kh-has-titlebar': isDesktop }">
    <!-- ══════ 独立标题栏：只放窗口控制，不混入导航栏 ══════ -->
    <header
      v-if="isDesktop"
      class="kh-titlebar"
      :class="'kh-titlebar--' + chrome"
      data-tauri-drag-region
    >
      <WindowControls
        v-if="chrome === 'mac'"
        variant="mac"
        class="kh-titlebar__controls"
        @minimize="winMin"
        @maximize="winMax"
        @close="winClose"
      />

      <div v-if="chrome === 'windows'" class="kh-titlebar__brand" data-tauri-drag-region>
        <span class="kh-titlebar__mark" aria-hidden="true" data-tauri-drag-region><BrandMark /></span>
        <span class="kh-titlebar__title" data-tauri-drag-region>墨钥 KeyHub</span>
      </div>
      <span
        v-else
        class="kh-titlebar__title kh-titlebar__title--center"
        data-tauri-drag-region
      >墨钥 KeyHub</span>

      <WindowControls
        v-if="chrome === 'windows'"
        variant="windows"
        class="kh-titlebar__controls kh-titlebar__controls--win"
        @minimize="winMin"
        @maximize="winMax"
        @close="winClose"
      />
    </header>

    <!-- ══════ 玻璃侧栏 ══════ -->
    <aside class="anx-shell__side" :class="{ 'is-open': navOpen }" aria-label="侧栏">
      <div class="anx-brandbar">
        <span class="anx-brandbar__mark kh-brand-mark" aria-hidden="true"><BrandMark /></span>
        <div>
          <div class="anx-brandbar__name">墨钥 KeyHub</div>
          <span class="anx-brandbar__sub">本地优先 · 数据不出网</span>
        </div>
      </div>

      <nav class="anx-sidenav" aria-label="主导航">
        <div class="anx-sidenav__group">
          <span class="anx-sidenav__label">工作区</span>
          <RouterLink
            v-for="item in NAV"
            :key="item.name"
            class="anx-sidenav__item"
            :to="{ name: item.name }"
            :aria-current="route.name === item.name ? 'page' : undefined"
          >
            <Icon :name="item.icon" :size="17" />
            {{ item.label }}
          </RouterLink>
        </div>

        <div class="anx-sidenav__group">
          <span class="anx-sidenav__label">系统</span>
          <RouterLink
            v-for="item in SYS_NAV"
            :key="item.name"
            class="anx-sidenav__item"
            :to="{ name: item.name }"
            :aria-current="route.name === item.name ? 'page' : undefined"
          >
            <Icon :name="item.icon" :size="17" />
            {{ item.label }}
          </RouterLink>
        </div>
      </nav>

      <!-- 环境状态卡 -->
      <div class="anx-shell__sidefoot kh-env">
        <span class="anx-sidenav__label">环境状态</span>
        <div class="kh-env__row">
          <span>OpenSSH</span>
          <b :class="'kh-tone--' + derived.sshTone" :title="env.sshLabel">{{ env.sshLabel }}</b>
        </div>
        <div class="kh-env__row">
          <span>Git</span>
          <b :class="'kh-tone--' + derived.gitTone" :title="env.gitVersion || '未检测到'">
            {{ env.gitVersion || '—' }}
          </b>
        </div>
        <div class="kh-env__row">
          <span>~/.ssh 权限</span>
          <b :class="'kh-tone--' + derived.permTone" :title="env.permLabel">{{ env.permLabel }}</b>
        </div>
        <div class="kh-env__row">
          <span>ssh-agent</span>
          <b :class="'kh-tone--' + derived.agentTone" :title="env.agentLabel">{{ agentShort }}</b>
        </div>
      </div>
    </aside>

    <!-- 窄屏遮罩：必须是侧栏的后续兄弟 -->
    <button
      class="anx-shell__scrim"
      type="button"
      aria-label="关闭侧栏"
      @click="navOpen = false"
    />

    <!-- ══════ 主区 ══════ -->
    <div class="anx-shell__main">
      <header class="anx-topbar" data-tauri-drag-region>
        <button
          class="anx-iconbtn anx-mobile-nav-toggle"
          type="button"
          aria-label="打开侧栏"
          @click="navOpen = true"
        >
          <Icon name="menu" :size="17" />
        </button>

        <div class="anx-topbar__crumbs">
          <span class="anx-topbar__crumb">墨钥</span>
          <span class="anx-topbar__crumb">/</span>
          <h1 class="anx-topbar__title">{{ pageTitle }}</h1>
        </div>

        <div class="anx-topbar__actions">
          <label class="anx-input-wrap kh-search">
            <Icon name="search" :size="15" class="kh-search__icon" />
            <input
              ref="searchInput"
              v-model="searchValue"
              class="anx-input"
              placeholder="搜索密钥"
              aria-label="搜索密钥"
              @keydown="onSearchKey"
            />
            <span class="kh-kbd">{{ searchKbd }}</span>
          </label>

          <span class="anx-badge anx-badge--neutral kh-trust">
            <Icon name="shield" :size="13" />
            本地优先 · 数据不出网
          </span>

          <button
            class="anx-iconbtn"
            type="button"
            :title="theme === 'dark' ? '切换到亮色主题' : '切换到暗色主题'"
            :aria-label="theme === 'dark' ? '切换到亮色主题' : '切换到暗色主题'"
            @click="toggleTheme({ origin: $event.currentTarget })"
          >
            <Icon :name="theme === 'dark' ? 'sun' : 'moon'" :size="17" />
          </button>
        </div>
      </header>

      <main class="anx-shell__body">
        <div class="anx-shell__inner">
          <!-- 首次导航完成前显示骨架，避免守卫读取偏好期间出现大片空白 -->
          <PageSkeleton v-if="!routeReady" :variant="bootVariant" />
          <RouteFade v-else />
        </div>
      </main>
    </div>
  </div>

  <OverlayHost />
  <ToastHost />
  <UpdateDialog />
</template>
