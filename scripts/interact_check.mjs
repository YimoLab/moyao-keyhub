/**
 * 交互闭环验证（v2）：
 * 1) 初始无任何遮罩/弹窗
 * 2) 打开"生成密钥"弹窗 → footer 必须有「取消」「生成密钥」按钮（曾因 #footer/#foot
 *    插槽名不一致而整体消失）
 * 3) 点右上角 X → 弹窗关闭
 * 4) 再次打开 → 点遮罩关闭（dismissable=true 路径）
 */
import { spawn } from 'node:child_process'
import { writeFileSync } from 'node:fs'

const CHROME = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe'
const PORT = 9335
const URL = process.argv[2] || 'http://localhost:5199/#/key-vault'
const lines = []
const log = (s) => { lines.push(String(s)); console.log(String(s)) }
const dump = () => writeFileSync('.interactresult.txt', lines.join('\n'), 'utf8')

const child = spawn(CHROME, [
  '--headless=new', '--disable-gpu', '--no-sandbox', '--no-first-run',
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${process.cwd()}\\.chrome-cdp2`,
  URL,
], { stdio: 'ignore' })

async function getTarget() {
  for (let i = 0; i < 40; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()
      const p = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl)
      if (p) return p
    } catch {}
    await new Promise((r) => setTimeout(r, 500))
  }
  throw new Error('CDP target not found')
}

function connect(wsUrl) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl)
    let id = 0
    const pending = new Map()
    const send = (method, params = {}) =>
      new Promise((res) => { const mid = ++id; pending.set(mid, res); ws.send(JSON.stringify({ id: mid, method, params })) })
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data)
      if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id) }
    }
    ws.onerror = () => reject(new Error('ws error'))
    ws.onopen = () => resolve({ send })
  })
}

try {
  const t = await getTarget()
  const { send } = await connect(t.webSocketDebuggerUrl)
  const evalJs = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })
    return r?.result?.result?.value
  }
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
  const count = (sel) => evalJs(`document.querySelectorAll('${sel}').length`)

  await sleep(2500)

  let pass = true
  const check = (name, ok) => { if (!ok) pass = false; log(`  ${ok ? 'PASS' : 'FAIL'} ${name}`) }

  // ── 1. 初始态 ──
  log('【1】初始态')
  check('无 modal-wrap', (await count('.modal-wrap')) === 0)

  const openGen = async () => {
    await evalJs(`
      [...document.querySelectorAll('button')].find(b => b.textContent.trim().includes('生成密钥'))?.click();
    `)
    await sleep(500)
  }
  const closeByX = async () => {
    await evalJs(`document.querySelector('.modal-head .icon-btn')?.click()`)
    await sleep(500)
  }
  const closeByBackdrop = async () => {
    await evalJs(`
      (() => {
        const w = document.querySelector('.modal-wrap');
        if (!w) return;
        const r = w.getBoundingClientRect();
        w.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: r.left + 5, clientY: r.top + 5 }));
      })()
    `)
    await sleep(500)
  }

  // ── 2. footer 按钮 ──
  log('')
  log('【2】打开生成弹窗，检查 footer 按钮')
  await openGen()
  check('modal-wrap 出现', (await count('.modal-wrap')) === 1)
  const footBtns = await evalJs(`
    [...document.querySelectorAll('.modal-foot button')].map(b => b.textContent.trim())
  `)
  log('  footer 按钮: ' + JSON.stringify(footBtns))
  check('存在「取消」按钮', footBtns.some((t) => t === '取消'))
  check('存在「生成密钥」按钮', footBtns.some((t) => t.includes('生成密钥')))
  check('X 关闭按钮存在', (await count('.modal-head .icon-btn')) === 1)

  // ── 3. X 关闭 ──
  log('')
  log('【3】点右上角 X 关闭')
  await closeByX()
  check('弹窗已关闭', (await count('.modal-wrap')) === 0)

  // ── 4. 遮罩关闭 ──
  log('')
  log('【4】重新打开，点遮罩关闭')
  await openGen()
  check('弹窗再次打开', (await count('.modal-wrap')) === 1)
  await closeByBackdrop()
  check('遮罩点击已关闭', (await count('.modal-wrap')) === 0)

  log('')
  log(pass ? 'RESULT: PASS footer 按钮 / X 关闭 / 遮罩关闭 全部正常' : 'RESULT: FAIL 存在失败项')
  dump()
} catch (e) {
  log('ERROR: ' + e.message)
  dump()
} finally {
  try { child.kill() } catch {}
  setTimeout(() => process.exit(0), 300)
}
