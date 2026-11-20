/**
 * 通过 CDP 做真实浏览器验证（不依赖任何第三方库，Node 22 自带 WebSocket）。
 * 流程：启动 headless Chrome(--remote-debugging-port) → 拉 /json/list 拿 ws url
 *      → Runtime.evaluate 检查 DOM → 输出断言。
 */
import { spawn } from 'node:child_process'
import { writeFileSync } from 'node:fs'

const CHROME = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe'
const PORT = 9333
const URL = process.argv[2] || 'http://127.0.0.1:5199/#/key-vault'
const lines = []
const log = (s) => { lines.push(s); console.log(s) }
const dump = () => writeFileSync('.cdpresult.txt', lines.join('\n'), 'utf8')

const child = spawn(CHROME, [
  '--headless=new', '--disable-gpu', '--no-sandbox', '--no-first-run',
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${process.cwd()}\\.chrome-cdp`,
  URL,
], { stdio: 'ignore', detached: false })

async function getTarget() {
  for (let i = 0; i < 40; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/list`)
      const list = await r.json()
      const page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl)
      if (page) return page
    } catch {}
    await new Promise((r) => setTimeout(r, 500))
  }
  throw new Error('CDP target not found')
}

function cdp(wsUrl) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl)
    let id = 0
    const pending = new Map()
    const send = (method, params = {}) =>
      new Promise((res) => {
        const mid = ++id
        pending.set(mid, res)
        ws.send(JSON.stringify({ id: mid, method, params }))
      })
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data)
      if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id) }
    }
    ws.onerror = (e) => reject(new Error('ws error'))
    ws.onopen = async () => {
      setTimeout(async () => {
        const res = await send('Runtime.evaluate', {
          expression: `(() => {
            const q = (s) => document.querySelectorAll(s).length;
            return JSON.stringify({
              overlayScrim: q('.anx-overlay-layer .anx-scrim'),
              modal: q('.anx-modal'),
              drawer: q('.anx-drawer'),
              appShell: q('.anx-shell'),
              bareRoot: q('.kh-bare'),
              titleOk: document.body.innerText.includes('墨钥 KeyHub'),
              navItems: q('.anx-sidenav__item'),
              bodyText: document.body.innerText.slice(0, 200)
            });
          })()`,
          returnByValue: true,
        })
        resolve({ send, ws, value: res?.result?.result?.value })
      }, 3500)
    }
  })
}

try {
  const target = await getTarget()
  log('CDP target: ' + target.url)
  const { value } = await cdp(target.webSocketDebuggerUrl)
  const d = JSON.parse(value)
  log('')
  log('.anx-scrim(常驻遮罩): ' + d.overlayScrim + '   <-- 期望 0')
  log('.anx-modal 节点:       ' + d.modal + '   <-- 期望 0')
  log('.anx-drawer 节点:      ' + d.drawer + '   <-- 期望 0')
  log('.anx-shell 外壳渲染:   ' + d.appShell)
  log('标题栏文案:           ' + d.titleOk)
  log('侧栏导航项数量:       ' + d.navItems)
  log('')
  log('页面首屏文本: ' + JSON.stringify(d.bodyText))
  const pass = d.overlayScrim === 0 && d.modal === 0 && d.drawer === 0 && d.appShell === 1
  log('')
  log(pass ? 'RESULT: PASS 弹窗默认不渲染，页面无遮罩拦截' : 'RESULT: FAIL')
  dump()
} catch (e) {
  log('ERROR: ' + e.message)
  dump()
} finally {
  try { child.kill() } catch {}
  setTimeout(() => process.exit(0), 300)
}
