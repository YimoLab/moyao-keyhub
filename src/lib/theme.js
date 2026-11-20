/**
 * 主题适配层：把墨钥原有的 useTheme() 接口接到鉴枢设计系统的主题引擎上。
 *
 * 真正的切换逻辑（View Transitions 圆形涟漪 / 圆盘兜底 / 减少动效）在
 * design/vue/lib/theme.ts；localStorage 键为 authnexus.theme，必须与
 * index.html 的首帧预置脚本一致。
 */
import {
  applyTheme,
  currentTheme,
  themeState,
  toggleTheme as anxToggleTheme,
} from '../design/vue/lib/theme'

const LEGACY_KEY = 'keyhub-theme'

export function useTheme() {
  const theme = themeState

  /** 同步响应式状态；首次运行时把旧版键迁移到设计系统键 */
  function initTheme() {
    try {
      if (!localStorage.getItem('authnexus.theme')) {
        const legacy = localStorage.getItem(LEGACY_KEY)
        if (legacy === 'dark' || legacy === 'light') {
          applyTheme(legacy)
          return
        }
      }
    } catch {
      /* localStorage 不可用时按首帧预置值走 */
    }
    applyTheme(currentTheme())
  }

  /**
   * 切换主题。传 origin（触发按钮）才有圆形涟漪过渡；
   * 设置页这类非按钮触发的场景直接瞬时切换。
   */
  function toggleTheme(opts = {}) {
    return anxToggleTheme(opts)
  }

  return { theme, initTheme, toggleTheme }
}
