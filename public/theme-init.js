/**
 * 首屏主题预置：在任何界面渲染前把 data-theme 设为目标值。
 * 独立成文件是为了兼容 Tauri 的 CSP（script-src 'self' 不允许内联脚本）。
 * 键名必须与 design/vue/lib/theme.ts 一致。
 */
;(function () {
  try {
    var t = localStorage.getItem('authnexus.theme')
    // 旧版本（样式重构前）使用 keyhub-theme，这里做一次无缝迁移
    if (t !== 'dark' && t !== 'light') {
      var legacy = localStorage.getItem('keyhub-theme')
      if (legacy === 'dark' || legacy === 'light') {
        t = legacy
        localStorage.setItem('authnexus.theme', legacy)
      }
    }
    if (t !== 'dark' && t !== 'light') t = 'light'
    document.documentElement.setAttribute('data-theme', t)
  } catch (e) {
    document.documentElement.setAttribute('data-theme', 'light')
  }
})()
