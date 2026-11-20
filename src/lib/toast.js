/**
 * 轻量 Toast 服务：全局挂载 $toast，无第三方依赖。
 * 规格取自 design-spec：bg --fg / r12 / 驻留 2.4s / rise 200ms。
 */
import { ref, h } from 'vue'
import ToastHost from '../components/ToastHost.vue'

const items = ref([])
let seq = 0

function push(message, variant = 'ok') {
  const id = ++seq
  items.value.push({ id, message, variant })
  setTimeout(() => {
    const i = items.value.findIndex((t) => t.id === id)
    if (i > -1) items.value.splice(i, 1)
  }, 2400)
}

export function installToast(app) {
  app.component('ToastHost', ToastHost)
  // 全局注入到所有模板：this.$toast(...) / const { $toast } = ...
  app.config.globalProperties.$toast = push
  app.provide('toast', push)
}

/** 组合式用法：在 <script setup> 中 const toast = useToast() */
export function useToast() {
  return push
}

export function useToastState() {
  return { items }
}

export const toast = push
