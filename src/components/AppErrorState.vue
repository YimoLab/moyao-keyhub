<script setup>
/**
 * 统一错误状态：所有页面的「读取失败 / 无法连接后端」都走这里，
 * 避免每个页面各写一套（裸居中文字 / 白卡片 / 红横幅）导致观感不一致。
 *
 * 只描述问题与下一步，不伪造任何数据。
 */
import AppIcon from './AppIcon.vue'

defineProps({
  title: { type: String, default: '读取失败' },
  detail: { type: String, default: '' },
  retryLabel: { type: String, default: '重试' },
})
defineEmits(['retry'])
</script>

<template>
  <div class="err-state" role="alert">
    <span class="err-seal"><AppIcon name="warning" :size="24" /></span>
    <strong>{{ title }}</strong>
    <p v-if="detail">{{ detail }}</p>
    <div class="err-actions">
      <button class="btn sm" @click="$emit('retry')">{{ retryLabel }}</button>
      <slot name="actions" />
    </div>
  </div>
</template>

<style scoped>
.err-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 9px;
  padding: 56px 24px;
  text-align: center;
}
.err-seal {
  width: 52px;
  height: 52px;
  border-radius: 16px;
  display: grid;
  place-items: center;
  background: var(--danger-soft);
  color: var(--danger-ink);
  margin-bottom: 3px;
}
.err-state strong {
  font-size: 14px;
  font-weight: 700;
}
.err-state p {
  color: var(--muted);
  font-size: 13px;
  max-width: 54ch;
  line-height: 1.55;
}
.err-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 5px;
}
</style>
