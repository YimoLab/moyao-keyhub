/**
 * 契约检查：确认前端 api.js 里调用的每个命令名，都在 Rust 侧注册表里。
 * 双向检查，任何一边多出/拼错都会报出来。
 *
 * 用法（node 22）：
 *   node scripts/check_api_contract.mjs
 */
import { readFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')

const apiSrc = readFileSync(resolve(root, 'src/lib/api.js'), 'utf8')
const libSrc = readFileSync(resolve(root, 'src-tauri/src/lib.rs'), 'utf8')

// 1. Rust 侧：generate_handler![ ... ] 里的命令名
const block = libSrc.match(/generate_handler!\[([\s\S]*?)\]/)
if (!block) {
  console.error('✗ 未能在 lib.rs 中定位 generate_handler! 块')
  process.exit(1)
}
const rustCmds = new Set(
  block[1]
    // 去掉行注释：注册表里每个分组前有 `// 环境` 这类注释，
    // 与首个命令共享同一个逗号分段，不剥离会漏掉每组第一条
    .replace(/\/\/[^\n]*/g, '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => /^[a-z][a-z0-9_]*$/.test(s)),
)

// 2. 前端侧：call('cmd' / call("cmd"
const feCmds = new Set()
for (const m of apiSrc.matchAll(/\bcall\(\s*['"]([a-z][a-z0-9_]*)['"]/g)) {
  feCmds.add(m[1])
}

const onlyFe = [...feCmds].filter((c) => !rustCmds.has(c)).sort()
const onlyRust = [...rustCmds].filter((c) => !feCmds.has(c)).sort()

console.log(`Rust 注册命令：${rustCmds.size}`)
console.log(`前端调用命令：${feCmds.size}`)
console.log('')

if (onlyFe.length) {
  console.log('✗ 前端调用了未注册的命令（会运行时报错）：')
  for (const c of onlyFe) console.log('   - ' + c)
  console.log('')
}
if (onlyRust.length) {
  console.log('· 后端已注册但前端未封装（不算错，仅提示）：')
  for (const c of onlyRust) console.log('   - ' + c)
  console.log('')
}
if (!onlyFe.length && !onlyRust.length) {
  console.log('✓ 双向完全一致')
} else if (!onlyFe.length) {
  console.log('✓ 前端调用的命令后端全部存在（契约安全）')
}

process.exit(onlyFe.length ? 1 : 0)
