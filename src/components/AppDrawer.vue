<script setup>
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import Icon from '../design/vue/ui/Icon.vue'
import { isTopOverlay, nextOverlayId, popOverlay, pushOverlay } from '../lib/overlayStack.js'

/**
 * 声明式抽屉（墨钥自有 API，视觉与行为对齐鉴枢设计系统）。
 * 结构落在 #overlay-layer 内：.anx-scrim + .anx-drawer（右侧贴边、仅左侧圆角）。
 */
const open = defineModel({ type: Boolean, default: false })

defineProps({
  title: { type: String, default: '' },
  subtitle: { type: String, default: '' },
})

const emit = defineEmits(['close'])

const visible = ref(open.value)
const closing = ref(false)
const rootEl = ref(null)
let closeTimer = 0
const overlayId = nextOverlayId()

watch(open, (v) => {
  clearTimeout(closeTimer)
  if (v) {
    closing.value = false
    visible.value = true
    nextTick(() => {
      const target =
        rootEl.value?.querySelector(
          '[data-autofocus], a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
        ) || rootEl.value
      try {
        target?.focus()
      } catch {
        /* 元素不可聚焦时静默 */
      }
    })
  } else if (visible.value) {
    closing.value = true
    closeTimer = setTimeout(() => {
      visible.value = false
      closing.value = false
    }, 190)
  }
})

function requestClose() {
  if (closing.value) return
  open.value = false
  emit('close')
}

function onScrim(e) {
  if (e.target === e.currentTarget) requestClose()
}

function onKey(e) {
  if (!isTopOverlay(overlayId)) return
  if (e.key === 'Escape') requestClose()
}

watch(visible, (v) => {
  if (v) {
    pushOverlay(overlayId)
    document.addEventListener('keydown', onKey)
  } else {
    popOverlay(overlayId)
    document.removeEventListener('keydown', onKey)
  }
}, { immediate: true })

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey)
  popOverlay(overlayId)
  clearTimeout(closeTimer)
})
</script>

<template>
  <Teleport to="#overlay-layer">
    <template v-if="visible">
      <div
        class="anx-scrim"
        :class="{ 'is-closing': closing }"
        data-scrim
        @click="onScrim"
      />
      <aside
        ref="rootEl"
        class="anx-drawer"
        :class="{ 'is-closing': closing }"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
      >
        <div class="anx-drawer__head">
          <div class="kh-grow">
            <h2 class="anx-drawer__title" :title="title">{{ title }}</h2>
            <p v-if="subtitle" class="anx-t-xs anx-t-faint">{{ subtitle }}</p>
          </div>
          <button type="button" class="anx-iconbtn" aria-label="关闭" @click="requestClose">
            <Icon name="close" :size="16" />
          </button>
        </div>
        <div class="anx-drawer__body">
          <slot />
        </div>
        <div v-if="$slots.footer" class="anx-drawer__foot">
          <slot name="footer" />
        </div>
      </aside>
    </template>
  </Teleport>
</template>
