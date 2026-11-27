/* ==========================================================================
   墨钥 KeyHub · 官网站点脚本
   --------------------------------------------------------------------------
   只做四件事：主题切换（含涟漪）、导航滚动态 / 当前章节、滚动进入、年份。
   不引入框架，不改变任何鉴枢资产的行为。
   ========================================================================== */
;(function () {
  'use strict'

  /* --------------------------------------------------------------------------
     主题：与桌面端共用 localStorage 键 authnexus.theme
     涟漪逻辑与 design/vue/lib/theme.ts 保持一致：
       浅 → 深：深色圆盘从按钮向外扩散，收尾才切主题；
       深 → 浅：先切主题（藏在满屏圆盘下），再让圆盘向按钮收缩。
     -------------------------------------------------------------------------- */
  var THEME_KEY = 'authnexus.theme'
  var RIPPLE_MS = 520
  var EASE = 'cubic-bezier(.4,0,.2,1)'
  var root = document.documentElement
  var themeBtn = document.getElementById('theme-toggle')
  var rippling = false

  function currentTheme() {
    return root.getAttribute('data-theme') === 'dark' ? 'dark' : 'light'
  }

  function syncThemeButton() {
    if (!themeBtn) return
    var dark = currentTheme() === 'dark'
    themeBtn.setAttribute('aria-label', dark ? '切换到浅色主题' : '切换到深色主题')
    themeBtn.setAttribute('title', dark ? '切换到浅色主题' : '切换到深色主题')
  }

  function applyTheme(theme) {
    root.setAttribute('data-theme', theme)
    try {
      localStorage.setItem(THEME_KEY, theme)
    } catch (e) {
      /* 隐私模式下写入失败不影响当前会话 */
    }
    syncThemeButton()
  }

  function reducedMotion() {
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches
  }

  function pageColor(theme) {
    var probe = document.createElement('div')
    probe.setAttribute('data-theme', theme)
    probe.style.cssText = 'position:fixed;left:-9999px;top:0;width:0;height:0'
    document.body.appendChild(probe)
    var v = getComputedStyle(probe).getPropertyValue('--paper').trim()
    probe.remove()
    return v || (theme === 'dark' ? '#1A1A1A' : '#FAFAFA')
  }

  function rippleGeometry(el) {
    var r = el.getBoundingClientRect()
    var x = Math.round(r.left + r.width / 2)
    var y = Math.round(r.top + r.height / 2)
    var far = Math.round(Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y)))
    return { x: x, y: y, far: far }
  }

  function supportsViewTransition() {
    return typeof document.startViewTransition === 'function'
  }

  function viewTransitionRipple(next, origin) {
    if (!supportsViewTransition()) return Promise.resolve(false)
    var g = rippleGeometry(origin)
    root.setAttribute('data-vt-mode', next === 'dark' ? 'expand' : 'shrink')
    try {
      var vt = document.startViewTransition(function () {
        applyTheme(next)
      })
      return vt.ready
        .then(function () {
          var target = next === 'dark' ? '::view-transition-new(root)' : '::view-transition-old(root)'
          var frames =
            next === 'dark'
              ? ['circle(0px at ' + g.x + 'px ' + g.y + 'px)', 'circle(' + g.far + 'px at ' + g.x + 'px ' + g.y + 'px)']
              : ['circle(' + g.far + 'px at ' + g.x + 'px ' + g.y + 'px)', 'circle(0px at ' + g.x + 'px ' + g.y + 'px)']
          var anim = root.animate({ clipPath: frames }, { duration: RIPPLE_MS, easing: EASE, fill: 'both', pseudoElement: target })
          return anim.finished.then(function () {
            return vt.finished.catch(function () {})
          }).then(function () {
            anim.cancel()
            return true
          })
        })
        .catch(function () {
          applyTheme(next)
          return true
        })
        .finally(function () {
          root.removeAttribute('data-vt-mode')
        })
    } catch (e) {
      root.removeAttribute('data-vt-mode')
      applyTheme(next)
      return Promise.resolve(true)
    }
  }

  function discRipple(next, origin) {
    var from = currentTheme()
    var g = rippleGeometry(origin)
    var full = 'circle(' + g.far + 'px at ' + g.x + 'px ' + g.y + 'px)'
    var point = 'circle(0px at ' + g.x + 'px ' + g.y + 'px)'

    var disc = document.createElement('div')
    disc.className = 'anx-theme-ripple'
    disc.style.background = pageColor(next === 'dark' ? 'dark' : from)
    document.body.appendChild(disc)

    if (next !== 'dark') applyTheme(next)

    var anim = disc.animate(
      next === 'dark' ? [{ clipPath: point }, { clipPath: full }] : [{ clipPath: full }, { clipPath: point }],
      { duration: RIPPLE_MS, easing: EASE, fill: 'both' }
    )
    return anim.finished
      .catch(function () {})
      .then(function () {
        if (next === 'dark') applyTheme(next)
        disc.remove()
      })
  }

  function toggleTheme(origin) {
    var next = currentTheme() === 'dark' ? 'light' : 'dark'
    if (!origin || reducedMotion() || rippling) {
      applyTheme(next)
      return
    }
    rippling = true
    viewTransitionRipple(next, origin)
      .then(function (done) {
        if (!done) return discRipple(next, origin)
      })
      .finally(function () {
        rippling = false
      })
  }

  if (themeBtn) {
    themeBtn.addEventListener('click', function (e) {
      toggleTheme(e.currentTarget)
    })
  }
  syncThemeButton()

  /* --------------------------------------------------------------------------
     导航：滚动后收成玻璃胶囊 + 当前章节高亮
     -------------------------------------------------------------------------- */
  var nav = document.getElementById('ks-nav')
  var darkSection = document.getElementById('security')
  function onScroll() {
    if (!nav) return
    nav.classList.toggle('is-stuck', window.scrollY > 8)
    /* 导航压在局部深色区块上时反色，保证文字与 logo 可读 */
    var overDark = false
    if (darkSection) {
      var r = darkSection.getBoundingClientRect()
      overDark = r.top <= 72 && r.bottom >= 0
    }
    nav.classList.toggle('is-over-dark', overDark)
  }
  onScroll()
  window.addEventListener('scroll', onScroll, { passive: true })

  var links = Array.prototype.slice.call(document.querySelectorAll('.ks-nav__links a'))
  var byId = {}
  links.forEach(function (a) {
    var id = (a.getAttribute('href') || '').replace(/^#/, '')
    if (id) byId[id] = a
  })

  var watched = Array.prototype.slice
    .call(document.querySelectorAll('main section[id]'))
    .filter(function (s) { return byId[s.id] })

  if ('IntersectionObserver' in window && watched.length) {
    var navIO = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (en) {
          var link = byId[en.target.id]
          if (en.isIntersecting) {
            links.forEach(function (a) { a.removeAttribute('aria-current') })
            if (link) link.setAttribute('aria-current', 'true')
          } else if (link && link.getAttribute('aria-current') === 'true') {
            /* 离开观察带（例如回到首屏）时清掉高亮，避免停在上一节 */
            link.removeAttribute('aria-current')
          }
        })
      },
      { rootMargin: '-45% 0px -50% 0px', threshold: 0 }
    )
    watched.forEach(function (s) { navIO.observe(s) })
  }

  /* --------------------------------------------------------------------------
     滚动进入：只做一次，减少动效时直接显示
     -------------------------------------------------------------------------- */
  var reveals = Array.prototype.slice.call(document.querySelectorAll('.ks-reveal'))
  if (!('IntersectionObserver' in window) || reducedMotion()) {
    reveals.forEach(function (el) { el.classList.add('is-in') })
  } else {
    var revealIO = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (en) {
          if (!en.isIntersecting) return
          en.target.classList.add('is-in')
          revealIO.unobserve(en.target)
        })
      },
      { rootMargin: '0px 0px -8% 0px', threshold: 0.08 }
    )
    reveals.forEach(function (el) { revealIO.observe(el) })
  }

  /* --------------------------------------------------------------------------
     版本与下载统计：读取 GitHub Releases，异步更新版本号、安装包链接、体积
     和累计下载量。HTML 里的静态值只在接口失败时兜底。
     -------------------------------------------------------------------------- */
  var RELEASES_API = 'https://api.github.com/repos/YimoLab/moyao-keyhub/releases?per_page=100'
  var RELEASE_CACHE_KEY = 'keyhub.releases.v2'
  var RELEASE_TTL = 6 * 60 * 60 * 1000

  function formatSize(bytes) {
    if (!bytes || typeof bytes !== 'number') return ''
    var mb = bytes / (1024 * 1024)
    if (mb >= 1) return (Math.round(mb * 10) / 10).toFixed(1).replace(/\.0$/, '') + ' MB'
    return Math.max(1, Math.round(bytes / 1024)) + ' KB'
  }

  function formatCount(n) {
    try {
      return new Intl.NumberFormat('zh-CN').format(n)
    } catch (e) {
      return String(n)
    }
  }

  function applyRelease(rel, totalDownloads) {
    if (!rel || !rel.tag_name) return
    var tag = String(rel.tag_name)
    var display = /^v/i.test(tag) ? tag : 'v' + tag
    var version = tag.replace(/^v/i, '')

    document.querySelectorAll('[data-kh-version]').forEach(function (el) {
      el.textContent = display
    })

    var exe = (rel.assets || []).filter(function (a) {
      return /\.exe$/i.test(a.name || '') && !/\.sig$/i.test(a.name || '')
    })[0]

    if (exe && /^https:\/\//i.test(exe.browser_download_url || '')) {
      document.querySelectorAll('[data-kh-download]').forEach(function (el) {
        el.setAttribute('href', exe.browser_download_url)
      })
      var size = formatSize(exe.size)
      if (size) {
        document.querySelectorAll('[data-kh-size]').forEach(function (el) {
          el.textContent = size
        })
      }
    }

    if (typeof totalDownloads === 'number' && isFinite(totalDownloads)) {
      document.querySelectorAll('[data-kh-downloads]').forEach(function (el) {
        el.textContent = formatCount(totalDownloads)
      })
      document.querySelectorAll('[data-kh-downloads-wrap]').forEach(function (el) {
        el.hidden = false
      })
    }

    var ld = document.getElementById('kh-jsonld')
    if (ld) {
      try {
        var json = JSON.parse(ld.textContent)
        json.softwareVersion = version
        if (exe) json.downloadUrl = exe.browser_download_url
        ld.textContent = JSON.stringify(json)
      } catch (e) {
        /* JSON-LD 结构异常时不影响页面 */
      }
    }

    root.setAttribute('data-release', display)
  }

  /** 汇总所有 Release：取最新正式版，并累加各版本 .exe 安装包的下载次数 */
  function summarizeReleases(releases) {
    if (!Array.isArray(releases)) return null
    var latest = releases.filter(function (r) {
      return r && r.tag_name && !r.draft && !r.prerelease
    })[0]
    if (!latest) return null

    var total = 0
    releases.forEach(function (r) {
      (r.assets || []).forEach(function (a) {
        if (/\.exe$/i.test(a.name || '') && !/\.sig$/i.test(a.name || '')) {
          total += Number(a.download_count) || 0
        }
      })
    })
    return { latest: latest, total: total }
  }

  function cachedRelease() {
    try {
      var raw = localStorage.getItem(RELEASE_CACHE_KEY)
      if (!raw) return null
      var box = JSON.parse(raw)
      if (!box || !box.at || Date.now() - box.at > RELEASE_TTL) return null
      return box.data
    } catch (e) {
      return null
    }
  }

  function fetchLatestRelease() {
    if (typeof fetch !== 'function') return
    var controller = typeof AbortController === 'function' ? new AbortController() : null
    var timer = controller ? setTimeout(function () { controller.abort() }, 4500) : null

    fetch(RELEASES_API, {
      headers: { Accept: 'application/vnd.github+json' },
      signal: controller ? controller.signal : undefined,
    })
      .then(function (res) {
        if (!res.ok) throw new Error('HTTP ' + res.status)
        return res.json()
      })
      .then(function (data) {
        var summary = summarizeReleases(data)
        if (!summary) throw new Error('no release')
        applyRelease(summary.latest, summary.total)
        try {
          /* 只缓存用得到的字段，避免把 release notes 整包塞进 localStorage */
          var slim = {
            tag_name: summary.latest.tag_name,
            assets: (summary.latest.assets || []).map(function (a) {
              return { name: a.name, size: a.size, browser_download_url: a.browser_download_url }
            }),
            total: summary.total,
          }
          localStorage.setItem(RELEASE_CACHE_KEY, JSON.stringify({ at: Date.now(), data: slim }))
        } catch (e) {
          /* 存储不可用不影响本次更新 */
        }
      })
      .catch(function () {
        /* 离线 / 限流 / 网络错误：保留静态兜底值，统计数字保持隐藏 */
      })
      .finally(function () {
        if (timer) clearTimeout(timer)
      })
  }

  var cached = cachedRelease()
  if (cached) {
    applyRelease(cached, cached.total)
  } else if (typeof requestIdleCallback === 'function') {
    requestIdleCallback(fetchLatestRelease, { timeout: 2500 })
  } else {
    setTimeout(fetchLatestRelease, 900)
  }

  /* --------------------------------------------------------------------------
     反馈邮箱复制：优先 Clipboard API，被拒时回退到临时 textarea。
     复制按钮默认 hidden，脚本可用时才显示，避免无 JS 时出现无效按钮。
     -------------------------------------------------------------------------- */
  var copyBtns = Array.prototype.slice.call(document.querySelectorAll('[data-kh-copy]'))

  function legacyCopy(text) {
    var area = document.createElement('textarea')
    area.value = text
    area.setAttribute('readonly', '')
    area.style.cssText = 'position:fixed;top:0;left:-9999px;opacity:0'
    document.body.appendChild(area)
    area.select()
    var ok = false
    try {
      ok = document.execCommand('copy')
    } catch (e) {
      ok = false
    }
    area.remove()
    return ok
  }

  function copyText(text) {
    if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
      try {
        return Promise.resolve(navigator.clipboard.writeText(text)).then(
          function () { return true },
          function () { return legacyCopy(text) }
        )
      } catch (e) {
        /* 某些环境调用即抛错，继续走回退 */
      }
    }
    return Promise.resolve(legacyCopy(text))
  }

  copyBtns.forEach(function (btn) {
    var label = btn.querySelector('[data-kh-copy-label]')
    var status = document.getElementById(btn.getAttribute('aria-describedby') || '')
    var idle = label ? label.textContent : ''
    var timer = null

    btn.hidden = false

    btn.addEventListener('click', function () {
      copyText(btn.getAttribute('data-kh-copy') || '').then(function (ok) {
        btn.classList.toggle('is-copied', ok)
        btn.classList.toggle('is-failed', !ok)
        if (label) label.textContent = ok ? '已复制' : '复制失败'
        if (status) status.textContent = ok ? '邮箱地址已复制到剪贴板' : '复制失败，请手动选择邮箱地址'
        if (timer) clearTimeout(timer)
        timer = setTimeout(function () {
          btn.classList.remove('is-copied', 'is-failed')
          if (label) label.textContent = idle
          if (status) status.textContent = ''
        }, 2400)
      })
    })
  })

  /* --------------------------------------------------------------------------
     页脚年份
     -------------------------------------------------------------------------- */
  var year = document.getElementById('year')
  if (year) year.textContent = String(new Date().getFullYear())
})()
