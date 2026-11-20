<script setup>
/**
 * 图标组件：全部路径取自 ui/screens/*.html，保证与设计稿逐笔一致。
 * 统一 24×24 viewBox、stroke currentColor、fill none。
 */
defineProps({
  name: { type: String, required: true },
  size: { type: [Number, String], default: 18 },
  width: { type: [Number, String], default: 1.6 },
})

const PATHS = {
  /* ── 导航 ── */
  key: '<circle cx="8" cy="12" r="3.5"/><path d="M11.5 12H20M17 12v3.5"/>',
  config: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M10 13l-2 2 2 2M14 13l2 2-2 2"/>',
  pulse: '<path d="M3 12h4l2-7 4 14 2-7h6"/>',
  shield: '<path d="M12 3l7 3v5c0 4.5-3 8.5-7 10-4-1.5-7-5.5-7-10V6z"/><path d="M9.5 12l2 2 3.5-4"/>',
  settings:
    '<circle cx="12" cy="12" r="3"/><path d="M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M18.5 5.5l-1.7 1.7M7.2 16.8l-1.7 1.7"/>',

  /* ── 操作 ── */
  plus: '<path d="M12 5v14M5 12h14"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  minimize: '<path d="M6 12h12"/>',
  maximize: '<rect x="6" y="6" width="12" height="12" rx="1.5"/>',
  copy: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
  external: '<path d="M14 5h5v5M19 5l-8 8M19 14v5H5V5h5"/>',
  trash: '<path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13"/>',
  rotate:
    '<path d="M20 11A8 8 0 0 0 5.6 6.6L4 8"/><path d="M4 4v4h4"/><path d="M4 13a8 8 0 0 0 14.4 4.4L20 16"/><path d="M20 20v-4h-4"/>',
  refresh: '<path d="M20 11a8 8 0 1 0-2.3 6.3M20 5v6h-6"/>',
  play: '<path d="M7 5l12 7-12 7z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>',
  check: '<path d="M5 13l4 4 10-11"/>',
  warning: '<path d="M12 4l9 16H3z"/><path d="M12 10v4M12 17.5v.5"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 8v4M12 15.5v.5"/>',
  download: '<path d="M12 4v10M8 10l4 4 4-4M5 19h14"/>',
  upload: '<path d="M12 20V10M8 14l4-4 4 4M5 5h14"/>',
  lock: '<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>',
  unlock: '<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 7-2.6"/>',
  eye: '<path d="M2.5 12S6 6 12 6s9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6z"/><circle cx="12" cy="12" r="2.6"/>',
  eyeOff: '<path d="M4 4l16 16"/><path d="M9.6 9.7A2.6 2.6 0 0 0 12 14.6c.7 0 1.3-.2 1.8-.7"/><path d="M6.2 6.5C3.8 8.2 2.5 12 2.5 12s3.5 6 9.5 6c1.6 0 3-.4 4.2-1M9.7 6.3A9 9 0 0 1 12 6c6 0 9.5 6 9.5 6s-.7 1.2-1.9 2.6"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9l3 3-3 3M13 15h4"/>',
  server:
    '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/>',
  monitor: '<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M4 9h16M9 4v5"/>',
  branch: '<circle cx="7" cy="6" r="2.5"/><circle cx="7" cy="18" r="2.5"/><circle cx="17" cy="12" r="2.5"/><path d="M7 8.5v7M9.5 6.8l5 3.7"/>',
  moon: '<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5z"/>',
  sun: '<circle cx="12" cy="12" r="4.5"/><path d="M12 2.5v2.2M12 19.3v2.2M2.5 12h2.2M19.3 12h2.2M4.8 4.8l1.6 1.6M17.6 17.6l1.6 1.6M19.2 4.8l-1.6 1.6M6.4 17.6l-1.6 1.6"/>',
  arrowRight: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  chevronRight: '<path d="M9 6l6 6-6 6"/>',
  filter: '<path d="M4 6h16M4 12h16M4 18h10"/>',
  layers: '<path d="M12 3l9 5-9 5-9-5z"/><path d="M3 13l9 5 9-5"/>',
  history: '<path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1M3 4v4h4"/><path d="M12 8v4.5l3 2"/>',
  shieldOff: '<path d="M12 3l7 3v5c0 4.5-3 8.5-7 10-4-1.5-7-5.5-7-10V6z"/><path d="M9.5 9.5l5 5"/>',
}

const FILLED = new Set(['arrowRight'])
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    :fill="FILLED.has(name) ? 'none' : 'none'"
    stroke="currentColor"
    :stroke-width="width"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    v-html="PATHS[name] || ''"
  />
</template>
