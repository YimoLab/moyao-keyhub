<script setup>
import { onMounted, onBeforeUnmount, watch } from 'vue'
import AppIcon from './AppIcon.vue'

/**
 * 显隐由 v-model 控制（父组件传 v-model="xxxOpen"）。
 * overlay 与 drawer 都必须在 v-if 内，否则全屏遮罩常驻。
 */
const open = defineModel({ type: Boolean, default: false })

defineProps({
  title: { type: String, default: '' },
  subtitle: { type: String, default: '' },
})

const emit = defineEmits(['close'])

function requestClose() {
  open.value = false
  emit('close')
}

function onKey(e) {
  if (e.key === 'Escape') requestClose()
}

watch(open, (v) => {
  document.body.style.overflow = v ? 'hidden' : ''
})

onMounted(() => document.addEventListener('keydown', onKey))
onBeforeUnmount(() => document.removeEventListener('keydown', onKey))
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="open" class="overlay" @click="requestClose" />
    </Transition>
    <Transition name="drawer-slide">
      <aside v-if="open" class="drawer" role="dialog" aria-modal="true">
        <div class="drawer-head">
          <span class="t" :title="title">{{ title }}</span>
          <button class="icon-btn" aria-label="关闭" @click="requestClose">
            <AppIcon name="close" :width="1.8" />
          </button>
        </div>
        <div class="drawer-body">
          <slot />
        </div>
        <div v-if="$slots.footer" class="drawer-foot">
          <slot name="footer" />
        </div>
      </aside>
    </Transition>
  </Teleport>
</template>
