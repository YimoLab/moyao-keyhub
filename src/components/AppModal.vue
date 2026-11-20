<script setup>
import { onMounted, onBeforeUnmount, watch } from 'vue'
import AppIcon from './AppIcon.vue'

/**
 * 显隐由 v-model 控制（父组件传 v-model="xxxOpen"）。
 * 必须用 v-if 真正卸载根节点，否则全屏遮罩会常驻并拦截页面点击。
 */
const open = defineModel({ type: Boolean, default: false })

const props = defineProps({
  title: { type: String, default: '' },
  wide: { type: Boolean, default: false },
  /** 点击遮罩是否关闭；危险确认类操作应设为 false */
  dismissable: { type: Boolean, default: true },
  /** 是否显示右上角关闭按钮 */
  closable: { type: Boolean, default: true },
})

const emit = defineEmits(['close'])

/** X 关闭按钮：总是可关（点 X = 取消操作，安全）。是否显示由 closable 控制 */
function close() {
  open.value = false
  // 兼容 @close 用法（如有）
  emit('close')
}

/** 遮罩 / ESC：dismissable=false 的二次确认弹窗不允许这样关闭，必须走按钮 */
function requestClose() {
  if (props.dismissable) close()
}

// 弹窗打开时锁滚动，避免背后页面跟随滚动
watch(open, (v) => {
  document.body.style.overflow = v ? 'hidden' : ''
})

function onBackdrop(e) {
  if (e.target === e.currentTarget) requestClose()
}

function onKey(e) {
  if (e.key === 'Escape' && open.value) requestClose()
}

onMounted(() => document.addEventListener('keydown', onKey))
onBeforeUnmount(() => document.removeEventListener('keydown', onKey))
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="open" class="modal-wrap" role="dialog" aria-modal="true" @click="onBackdrop">
        <div class="modal" :class="{ 'modal-wide': wide }">
          <div v-if="title || $slots.head || closable" class="modal-head">
            <span><slot name="head">{{ title }}</slot></span>
            <button v-if="closable" class="icon-btn" aria-label="关闭" @click="close">
              <AppIcon name="close" :width="1.8" />
            </button>
          </div>
          <div class="modal-body">
            <slot />
          </div>
          <div v-if="$slots.footer" class="modal-foot">
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
