/**
 * 发行说明 Markdown 渲染
 *
 * 数据来自 latest.json（远端），属于不可信输入：先整体做 HTML 转义，再生成
 * 受控标签，链接仅允许 http(s)。
 *
 * 只覆盖发行说明常用的语法：标题、段落、列表、表格、代码块、引用、分割线、
 * 粗体、行内代码与链接。不引入第三方依赖，避免为一段说明扩大攻击面。
 */

const ESCAPE = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ESCAPE[c])
}

/** 行内语法：代码先占位，避免内部字符被后续规则改写 */
function inline(text) {
  const codes = []
  let s = esc(text)
  s = s.replace(/`([^`]+)`/g, (_, code) => {
    codes.push(code)
    return `\u0000${codes.length - 1}\u0000`
  })
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  s = s.replace(
    /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
    '<a href="$2" target="_blank" rel="noreferrer noopener">$1</a>',
  )
  s = s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${codes[Number(i)]}</code>`)
  return s
}

function splitRow(line) {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((s) => s.trim())
}

function cellAlign(cell) {
  const left = cell.startsWith(':')
  const right = cell.endsWith(':')
  if (left && right) return 'center'
  if (right) return 'right'
  if (left) return 'left'
  return ''
}

function tableCell(tag, content, align) {
  const style = align ? ` style="text-align:${align}"` : ''
  return `<${tag}${style}>${inline(content)}</${tag}>`
}

function renderTable(header, aligns, rows) {
  const head = header.map((c, i) => tableCell('th', c, aligns[i])).join('')
  const body = rows
    .map((r) => {
      const cells = [...r, ...Array(Math.max(0, header.length - r.length)).fill('')].slice(
        0,
        header.length,
      )
      return `<tr>${cells.map((c, i) => tableCell('td', c, aligns[i])).join('')}</tr>`
    })
    .join('')
  return `<div class="kh-release-notes__table"><table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table></div>`
}

export function renderReleaseNotes(md) {
  const lines = String(md || '')
    .replace(/\r\n?/g, '\n')
    .split('\n')
  const html = []
  let i = 0

  const isTableStart = (idx) => {
    const head = lines[idx] || ''
    const sep = lines[idx + 1] || ''
    return (
      /^\s*\|/.test(head) &&
      /^\s*\|?[\s:|-]+\|?\s*$/.test(sep) &&
      sep.includes('-') &&
      sep.includes('|')
    )
  }

  while (i < lines.length) {
    const t = lines[i].trim()
    if (!t) {
      i++
      continue
    }

    // 围栏代码块
    if (/^```/.test(t)) {
      const buf = []
      i++
      while (i < lines.length && !/^```/.test(lines[i].trim())) {
        buf.push(lines[i])
        i++
      }
      i++ // 关闭行
      html.push(`<pre><code>${esc(buf.join('\n'))}</code></pre>`)
      continue
    }

    // 表格
    if (isTableStart(i)) {
      const header = splitRow(lines[i])
      const aligns = splitRow(lines[i + 1]).map(cellAlign)
      i += 2
      const rows = []
      while (i < lines.length && /^\s*\|/.test(lines[i])) {
        rows.push(splitRow(lines[i]))
        i++
      }
      html.push(renderTable(header, aligns, rows))
      continue
    }

    // 标题
    const heading = t.match(/^(#{1,4})\s+(.+)$/)
    if (heading) {
      const level = heading[1].length
      html.push(`<h${level}>${inline(heading[2])}</h${level}>`)
      i++
      continue
    }

    // 分割线
    if (/^(-{3,}|\*{3,})$/.test(t)) {
      html.push('<hr>')
      i++
      continue
    }

    // 引用
    if (/^>\s?/.test(t)) {
      const buf = []
      while (i < lines.length && /^>\s?/.test(lines[i].trim())) {
        buf.push(lines[i].trim().replace(/^>\s?/, ''))
        i++
      }
      html.push(`<blockquote><p>${inline(buf.join(' '))}</p></blockquote>`)
      continue
    }

    // 列表
    if (/^([-*+]|\d+\.)\s+/.test(t)) {
      const ordered = /^\d+\.\s+/.test(t)
      const tag = ordered ? 'ol' : 'ul'
      const items = []
      while (i < lines.length) {
        const current = lines[i].trim()
        const m = current.match(ordered ? /^\d+\.\s+(.+)$/ : /^[-*+]\s+(.+)$/)
        if (!m) break
        items.push(`<li>${inline(m[1])}</li>`)
        i++
      }
      html.push(`<${tag}>${items.join('')}</${tag}>`)
      continue
    }

    // 段落：吃到空行或下一个块级标记
    const buf = [t]
    i++
    while (i < lines.length) {
      const cur = lines[i].trim()
      if (
        !cur ||
        /^(#{1,4})\s/.test(cur) ||
        /^```/.test(cur) ||
        /^([-*+]|\d+\.)\s+/.test(cur) ||
        /^(-{3,}|\*{3,})$/.test(cur) ||
        /^>\s?/.test(cur) ||
        isTableStart(i)
      ) {
        break
      }
      buf.push(cur)
      i++
    }
    html.push(`<p>${inline(buf.join(' '))}</p>`)
  }

  return html.join('\n')
}
