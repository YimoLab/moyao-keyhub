/**
 * 声明式浮层栈：AppModal / AppDrawer 共用的滚动锁与「最上层」判定。
 *
 * 为什么需要：KeyVault 等页面会在抽屉里再弹二次确认。若各组件各自
 * 设置 body.overflow，先关闭的那个会把锁提前解除，背景页面在另一个
 * 浮层还开着时就能滚动；ESC 也会同时关掉两层。
 *
 * 约定：
 *   · 打开（visible=true）→ pushOverlay(id)；关闭 → popOverlay(id)；
 *   · 只有 isTopOverlay(id) 的浮层响应 ESC；
 *   · popOverlay 幂等，组件卸载时可直接调用。
 */
const stack = []
let seq = 0

export function nextOverlayId() {
  return ++seq
}

export function pushOverlay(id) {
  if (stack.includes(id)) return
  stack.push(id)
  document.body.style.overflow = 'hidden'
}

export function popOverlay(id) {
  const i = stack.lastIndexOf(id)
  if (i < 0) return
  stack.splice(i, 1)
  if (!stack.length) document.body.style.overflow = ''
}

export function isTopOverlay(id) {
  return stack.length > 0 && stack[stack.length - 1] === id
}
