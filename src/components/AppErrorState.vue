<script setup>
/**
 * 统一错误状态：所有页面的「读取失败 / 无法连接后端」都走这里。
 * 视觉对齐鉴枢 .anx-empty；只描述问题与下一步，不伪造任何数据。
 */
import Icon from '../design/vue/ui/Icon.vue'

defineProps({
  title: { type: String, default: '读取失败' },
  detail: { type: String, default: '' },
  retryLabel: { type: String, default: '重试' },
})
defineEmits(['retry'])
</script>

<template>
  <div class="anx-empty kh-error-state" role="alert">
    <div class="anx-empty__icon"><Icon name="alert-triangle" :size="24" /></div>
    <p class="anx-empty__title">{{ title }}</p>
    <p v-if="detail" class="anx-empty__text">{{ detail }}</p>
    <div class="anx-empty__actions">
      <button type="button" class="anx-btn anx-btn--sm" @click="$emit('retry')">
        {{ retryLabel }}
      </button>
      <slot name="actions" />
    </div>
  </div>
</template>
