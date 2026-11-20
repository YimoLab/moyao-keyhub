<script setup>
/**
 * 品牌标记：墨印 · 钥孔（Logo A）。
 * 使用设计令牌自适应明暗：浅色为黑底白孔，深色反转为白底黑孔；酸橙绿点恒亮。
 */
</script>

<template>
  <svg viewBox="0 0 512 512" class="kh-brand-icon" aria-hidden="true">
    <rect x="16" y="16" width="480" height="480" rx="120" fill="var(--ink)" />
    <circle cx="236" cy="214" r="66" fill="var(--paper)" />
    <path d="M236 280v98" stroke="var(--paper)" stroke-width="44" stroke-linecap="round" />
    <path d="M236 326h52" stroke="var(--paper)" stroke-width="30" stroke-linecap="round" />
    <circle cx="386" cy="386" r="24" fill="var(--acid)" />
  </svg>
</template>
