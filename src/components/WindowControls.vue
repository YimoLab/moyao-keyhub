<script setup>
import Icon from '../design/vue/ui/Icon.vue'

/**
 * 窗口控制按钮，两种标题栏风格：
 *   windows — 右侧最小化 / 最大化 / 关闭，贴近 Windows 11 系统按钮
 *   mac     — 左上角交通灯（关 / 最小化 / 最大化）
 * 只负责外观与事件，窗口操作用法由 App.vue 传入。
 */
defineProps({
  variant: { type: String, default: 'windows' },
})
defineEmits(['minimize', 'maximize', 'close'])
</script>

<template>
  <div v-if="variant === 'windows'" class="kh-win-controls">
    <button
      type="button"
      class="anx-iconbtn kh-win-btn"
      aria-label="最小化"
      title="最小化"
      @click="$emit('minimize')"
    >
      <Icon name="minimize" :size="15" :stroke="1.4" />
    </button>
    <button
      type="button"
      class="anx-iconbtn kh-win-btn"
      aria-label="最大化或还原"
      title="最大化 / 还原"
      @click="$emit('maximize')"
    >
      <Icon name="maximize" :size="12" :stroke="1.4" />
    </button>
    <button
      type="button"
      class="anx-iconbtn kh-win-btn kh-win-btn--close"
      aria-label="关闭"
      title="关闭"
      @click="$emit('close')"
    >
      <Icon name="close" :size="14" :stroke="1.4" />
    </button>
  </div>

  <div v-else class="kh-traffic" role="group" aria-label="窗口控制">
    <button
      type="button"
      class="kh-traffic__dot kh-traffic__dot--close"
      aria-label="关闭"
      title="关闭"
      @click="$emit('close')"
    />
    <button
      type="button"
      class="kh-traffic__dot kh-traffic__dot--min"
      aria-label="最小化"
      title="最小化"
      @click="$emit('minimize')"
    />
    <button
      type="button"
      class="kh-traffic__dot kh-traffic__dot--max"
      aria-label="最大化或还原"
      title="最大化 / 还原"
      @click="$emit('maximize')"
    />
  </div>
</template>
