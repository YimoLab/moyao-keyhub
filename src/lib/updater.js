/**
 * 更新检查与安装（Tauri updater）。
 *
 * 隐私约定：
 *   · 只有用户主动点击「检查更新」，或用户开启「启动时自动检查更新」后，
 *     才会 GET 一次 GitHub Release 的 latest.json；
 *   · 不上传任何本机信息，不包含遥测；
 *   · 更新包必须通过内置公钥的签名校验才会安装。
 */
import { markRaw, reactive } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import * as api from './api'
import { toast } from './toast'

const LAST_CHECK_KEY = 'keyhub.update.lastCheck'
/** 自动检查的最小间隔：6 小时，避免每次启动都请求 */
const AUTO_INTERVAL = 6 * 60 * 60 * 1000

export const updateState = reactive({
  open: false,
  checking: false,
  /** { version, currentVersion, date, body, raw } */
  update: null,
  downloading: false,
  installing: false,
  progress: 0,
  downloaded: 0,
  contentLength: 0,
  error: '',
})

let inflight = null

export function useUpdater() {
  return { state: updateState, checkForUpdates, downloadAndInstall, dismissUpdate }
}

/**
 * 检查更新。
 * silent=true 时不弹「已是最新 / 检查失败」提示（用于启动自动检查）。
 */
export async function checkForUpdates({ silent = false } = {}) {
  if (!api.isBackendAvailable()) {
    if (!silent) toast('浏览器预览模式无法检查更新', 'info')
    return null
  }
  if (inflight) return inflight

  updateState.checking = true
  updateState.error = ''
  inflight = (async () => {
    try {
      const update = await check()
      try {
        localStorage.setItem(LAST_CHECK_KEY, String(Date.now()))
      } catch {
        /* localStorage 不可用时忽略 */
      }
      if (!update) {
        if (!silent) toast('已是最新版本', 'success')
        return null
      }
      updateState.update = {
        version: update.version,
        currentVersion: update.currentVersion,
        date: update.date,
        body: update.body,
        raw: markRaw(update),
      }
      updateState.open = true
      return update
    } catch (e) {
      updateState.error = e?.message || String(e)
      if (!silent) toast(`检查更新失败：${updateState.error}`, 'danger')
      return null
    } finally {
      updateState.checking = false
      inflight = null
    }
  })()
  return inflight
}

/** 下载并安装；完成后重启应用 */
export async function downloadAndInstall() {
  const raw = updateState.update?.raw
  if (!raw) return
  updateState.downloading = true
  updateState.installing = false
  updateState.error = ''
  updateState.progress = 0
  updateState.downloaded = 0
  updateState.contentLength = 0

  try {
    await raw.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          updateState.contentLength = event.data?.contentLength || 0
          break
        case 'Progress':
          updateState.downloaded += event.data?.chunkLength || 0
          updateState.progress = updateState.contentLength
            ? Math.min(99, Math.round((updateState.downloaded / updateState.contentLength) * 100))
            : 0
          break
        case 'Finished':
          updateState.progress = 100
          updateState.installing = true
          break
      }
    })
    await relaunch()
  } catch (e) {
    updateState.error = e?.message || String(e)
    toast(`更新失败：${updateState.error}`, 'danger')
  } finally {
    updateState.downloading = false
    updateState.installing = false
  }
}

export function dismissUpdate() {
  if (updateState.downloading) return
  updateState.open = false
}

/** 启动后的自动检查：只在用户开启时执行，且 6 小时内不重复请求 */
export function maybeAutoCheck(prefs) {
  if (!prefs?.autoCheckUpdates) return
  let last = 0
  try {
    last = Number(localStorage.getItem(LAST_CHECK_KEY) || 0)
  } catch {
    last = 0
  }
  if (Date.now() - last < AUTO_INTERVAL) return
  setTimeout(() => {
    checkForUpdates({ silent: true })
  }, 5000)
}

// 开发期调试入口：便于在没有真实 Release 时验证更新弹窗
if (import.meta.env.DEV && typeof window !== 'undefined') {
  window.__keyhubUpdater = updateState
}
