import { createApp } from 'vue'
import { createRouter, createWebHashHistory } from 'vue-router'

/* 鉴枢设计系统 · 引入顺序即依赖顺序：fonts → tokens → brand → base → components → motion → shell */
import './design/fonts/fonts.css'
import './design/css/index.css'
/* 墨钥自有样式：只放业务布局补充，引入在资产之后 */
import './styles/app.css'

import App from './App.vue'
import { useTheme } from './lib/theme.js'

import Onboarding from './views/Onboarding.vue'
import EnvCheck from './views/EnvCheck.vue'
import KeyVault from './views/KeyVault.vue'
import ConfigEditor from './views/ConfigEditor.vue'
import Diagnostics from './views/Diagnostics.vue'
import SecurityCenter from './views/SecurityCenter.vue'
import Settings from './views/Settings.vue'
import { useEnvStore } from './lib/store.js'

const router = createRouter({
  // 桌面应用用 hash 路由，避免自定义协议下的 history 问题
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/key-vault' },
    { path: '/onboarding', name: 'onboarding', component: Onboarding, meta: { bare: true } },
    { path: '/env-check', name: 'env-check', component: EnvCheck },
    { path: '/key-vault', name: 'key-vault', component: KeyVault },
    { path: '/config', name: 'config', component: ConfigEditor },
    { path: '/diagnostics', name: 'diagnostics', component: Diagnostics },
    { path: '/security', name: 'security', component: SecurityCenter },
    { path: '/settings', name: 'settings', component: Settings },
    { path: '/:pathMatch(.*)*', redirect: '/key-vault' },
  ],
})

// ── 首次进入守卫 ──────────────────────────────────────────────
// 未完成引导（onboardedAt 为空）时，任何主页面路由都先重定向到引导页；
// 引导完成（store.markOnboarded）后自然放行。
// 注意：后端未就绪（prefs.available === false，如浏览器预览模式）时不劫持，
// 否则会把用户锁死在引导页。
let bootReady = false
router.beforeEach(async (to) => {
  if (to.name === 'onboarding') return true
  const store = useEnvStore()
  if (!bootReady) {
    await store.init()
    bootReady = true
  }
  if (!store.onboarded.value && store.rawPrefs.available) return { name: 'onboarding' }
  return true
})

const app = createApp(App)
app.use(router)

// 主题由 index.html 首帧预置，这里只同步响应式状态
const { initTheme } = useTheme()
initTheme()

app.mount('#app')
