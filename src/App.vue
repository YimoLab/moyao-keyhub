<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import AppIcon from './components/AppIcon.vue'
import ToastHost from './components/ToastHost.vue'
import { useTheme } from './lib/theme.js'
import * as api from './lib/api.js'
import { useEnvStore } from './lib/store.js'

const route = useRoute()
const router = useRouter()
const { theme, toggleTheme } = useTheme()
const store = useEnvStore()
const env = store.env
const derived = store.derived

const isBare = computed(() => route.meta.bare === true)
const searchValue = ref('')
const searchInput = ref(null)
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
  { name: 'config', label: 'SSH Config', icon: 'config' },
  { name: 'diagnostics', label: '连接测试', icon: 'pulse' },
  { name: 'security', label: '安全中心', icon: 'shield' },
]

const navItems = computed(() => NAV)

async function boot() {
  await store.refreshEnv()
}

onMounted(() => {
  boot()
  document.addEventListener('keydown', onGlobalKey)
})
onBeforeUnmount(() => document.removeEventListener('keydown', onGlobalKey))

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
  <div v-if="isBare" class="bare-root">
    <router-view v-slot="{ Component }">
      <transition name="view-fade" mode="out-in">
        <component :is="Component" />
      </transition>
    </router-view>
  </div>

  <div v-else class="app">
    <!-- 题栏：玻璃拟态 -->
    <header class="titlebar" data-tauri-drag-region>
      <span class="titlebar-brand">
        <span class="logo-seal">墨</span>
        <span>墨钥 KeyHub</span>
        <span class="meta">v0.1.0-mvp</span>
      </span>

      <div class="titlebar-right">
        <label class="search">
          <AppIcon name="search" :size="14" :width="1.8" />
          <input
            ref="searchInput"
            v-model="searchValue"
            placeholder="搜索密钥"
            aria-label="搜索密钥"
            @keydown="onSearchKey"
          />
          <span class="kbd">{{ searchKbd }}</span>
        </label>

        <span class="trust-badge">
          <AppIcon name="shield" :size="13" :width="1.8" />
          本地优先 · 数据不出网
        </span>

        <button
          class="theme-btn"
          type="button"
          :title="theme === 'dark' ? '切换到亮色主题' : '切换到暗色主题'"
          :aria-label="theme === 'dark' ? '切换到亮色主题' : '切换到暗色主题'"
          @click="toggleTheme"
        >
          <AppIcon :name="theme === 'dark' ? 'sun' : 'moon'" :size="16" :width="1.8" />
        </button>
      </div>

      <div v-if="isDesktop" class="win-controls">
        <button class="win-btn" aria-label="最小化" title="最小化" @click="winMin">
          <AppIcon name="minimize" :size="14" :width="1.7" />
        </button>
        <button class="win-btn" aria-label="最大化或还原" title="最大化 / 还原" @click="winMax">
          <AppIcon name="maximize" :size="12" :width="1.7" />
        </button>
        <button class="win-btn close" aria-label="关闭" title="关闭" @click="winClose">
          <AppIcon name="close" :size="13" :width="1.7" />
        </button>
      </div>
    </header>

    <!-- 侧栏 -->
    <aside class="sidebar">
      <span class="nav-label">工作区</span>
      <RouterLink
        v-for="item in navItems"
        :key="item.name"
        class="nav-item"
        :class="{ active: route.name === item.name }"
        :to="{ name: item.name }"
      >
        <AppIcon :name="item.icon" :size="18" />
        {{ item.label }}
      </RouterLink>

      <span class="nav-label" style="margin-top: 8px">系统</span>
      <RouterLink
        class="nav-item"
        :class="{ active: route.name === 'settings' }"
        :to="{ name: 'settings' }"
      >
        <AppIcon name="settings" :size="18" />
        设置
      </RouterLink>

      <RouterLink
        class="nav-item"
        :class="{ active: route.name === 'env-check' }"
        :to="{ name: 'env-check' }"
      >
        <AppIcon name="monitor" :size="18" />
        环境自检
      </RouterLink>

      <!-- 环境状态卡 -->
      <div class="sidebar-foot">
        <span class="nav-label" style="padding: 0 0 2px">环境状态</span>
        <div class="env-line">
          <span>OpenSSH</span>
          <b :class="derived.sshTone">{{ env.sshLabel }}</b>
        </div>
        <div class="env-line">
          <span>Git</span>
          <b :class="derived.gitTone">{{ env.gitVersion || '—' }}</b>
        </div>
        <div class="env-line">
          <span>~/.ssh 权限</span>
          <b :class="derived.permTone">{{ env.permLabel }}</b>
        </div>
        <div class="env-line">
          <span>ssh-agent</span>
          <b :class="derived.agentTone">{{ env.agentLabel }}</b>
        </div>
      </div>
    </aside>

    <!-- 主区 -->
    <main class="main">
      <router-view v-slot="{ Component }">
        <transition name="view-fade" mode="out-in">
          <component :is="Component" />
        </transition>
      </router-view>
    </main>
  </div>

  <ToastHost />
</template>

<style scoped>
.bare-root {
  height: 100vh;
  overflow: auto;
}
</style>
