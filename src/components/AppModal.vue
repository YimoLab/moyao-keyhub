<script setup>
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import Icon from '../design/vue/ui/Icon.vue'
import { isTopOverlay, nextOverlayId, popOverlay, pushOverlay } from '../lib/overlayStack.js'

/**
 * 声明式弹窗（墨钥自有 API，视觉与行为对齐鉴枢设计系统）。
 *
 * - 结构落在 #overlay-layer 内：.anx-scrim + .anx-modal
 * - 入场 / 离场沿用 --t 令牌；关闭时先播 is-closing 再卸载
 * - ESC / 点击遮罩仅在 dismissable 时生效；wide 映射为 anx-modal--lg
 */
const open = defineModel({ type: Boolean, default: false })

const props = defineProps({
  title: { type: String, default: '' },
  wide: { type: Boolean, default: false },
  /** 点击遮罩 / ESC 是否关闭；危险确认类操作应设为 false */
  dismissable: { type: Boolean, default: true },
  /** 是否显示右上角关闭按钮 */
  closable: { type: Boolean, default: true },
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

/** X 关闭按钮：总是可关（点 X = 取消操作，安全） */
function close() {
  open.value = false
  emit('close')
}

/** 遮罩 / ESC：dismissable=false 的二次确认弹窗不允许这样关闭 */
function requestClose() {
  if (props.dismissable && !closing.value) close()
}

function onScrim(e) {
  if (e.target === e.currentTarget) requestClose()
}

function onKey(e) {
  if (!isTopOverlay(overlayId)) return
  if (e.key === 'Escape') requestClose()
  if (e.key !== 'Tab' || !rootEl.value) return
  const list = Array.from(
    rootEl.value.querySelectorAll(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )
  ).filter((el) => el.offsetParent !== null)
  if (!list.length) return
  const first = list[0]
  const last = list[list.length - 1]
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault()
    first.focus()
  }
}

// 打开期间接管 ESC / Tab 并锁背景滚动；关闭时按栈归还
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
      <div
        ref="rootEl"
        class="anx-modal"
        :class="[{ 'anx-modal--lg': wide, 'is-closing': closing }]"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
      >
        <div v-if="title || $slots.head || closable" class="anx-modal__head">
          <div class="anx-modal__head-text">
            <h2 class="anx-modal__title">
              <slot name="head">{{ title }}</slot>
            </h2>
          </div>
          <button
            v-if="closable"
            type="button"
            class="anx-iconbtn anx-modal__close"
            aria-label="关闭"
            @click="close"
          >
            <Icon name="close" :size="16" />
          </button>
        </div>
        <div class="anx-modal__body">
          <slot />
        </div>
        <div v-if="$slots.footer" class="anx-modal__foot">
          <slot name="footer" />
        </div>
      </div>
    </template>
  </Teleport>
</template>
