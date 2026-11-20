/**
 * 主题引擎：亮 / 暗双主题，偏好持久化到 localStorage。
 * 复用设计稿的契约——仅切换 html[data-theme]，组件代码零改动。
 */
import { ref } from 'vue'

const KEY = 'keyhub-theme'
const theme = ref('light')

export function useTheme() {
  function apply(next) {
    theme.value = next
    document.documentElement.setAttribute('data-theme', next)
  }

  function initTheme() {
    let saved = 'light'
    try {
      saved = localStorage.getItem(KEY) === 'dark' ? 'dark' : 'light'
    } catch {
      /* localStorage 不可用时退回亮色 */
    }
    apply(saved)
  }

  function toggleTheme() {
    const next = theme.value === 'dark' ? 'light' : 'dark'
    apply(next)
    try {
      localStorage.setItem(KEY, next)
    } catch {
      /* 忽略持久化失败 */
    }
  }

  return { theme, initTheme, toggleTheme }
}
