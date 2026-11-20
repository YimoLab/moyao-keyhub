/**
 * 通知适配层：把墨钥原有 toast.push(message, variant) 的调用形式
 * 接到鉴枢设计系统的 toast() 上（视觉、动效、关闭按钮统一走 .anx-toast）。
 *
 * 原 variant → 鉴枢 tone：
 *   success / ok → ok；danger → danger；warn / info → info（鉴枢没有独立的 warn 色调）
 */
import { toast as anxToast } from '../design/vue/overlay/toast'

const TONE = { success: 'ok', ok: 'ok', danger: 'danger', warn: 'info', info: 'info' }

function push(message, variant = 'ok') {
  return anxToast({
    tone: TONE[variant] || 'info',
    title: String(message ?? ''),
    duration: 3200,
  })
}

/** 兼容 `const toast = useToast(); toast.push(…)` 与 `toast(…)` 两种写法 */
push.push = push

/** 复制到剪贴板并给出通知（引导页安装指令在用） */
push.copy = async (text, label = '已复制') => {
  try {
    await navigator.clipboard.writeText(text)
    push(label, 'success')
  } catch {
    push('复制失败，请手动选中复制', 'danger')
  }
}

export function useToast() {
  return push
}

export const toast = push

/** 兼容旧入口；ToastHost 由设计系统在 App.vue 挂载 */
export function installToast(app) {
  app.config.globalProperties.$toast = push
}
