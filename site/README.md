# 墨钥 KeyHub · 官网

介绍墨钥 KeyHub 作用的单页静态站。零构建、零框架运行时，部署时上传整个 `site/` 目录即可。

## 本地预览

```bash
# 在仓库根目录
python -m http.server 8137 --directory site
# 打开 http://127.0.0.1:8137/
```

任意静态服务器均可，不需要 Node 或 Vite。

## 目录

| 路径 | 职责 |
|---|---|
| `index.html` | 页面结构、文案、SEO 与 JSON-LD |
| `assets/site.css` | 站点层样式，`ks-` 前缀，只做布局与营销区块 |
| `assets/site.js` | 主题切换（含涟漪）、导航滚动态、当前章节、入场动画、GitHub 最新版本同步、反馈邮箱复制 |
| `assets/theme-init.js` | 首帧主题预置，键为 `authnexus.theme`（与桌面端共用） |
| `assets/shots/` | 产品截图，2240 与 1120 两档 WebP，配合 `srcset` |
| `design/css/` | 鉴枢设计系统 CSS 资产副本 |
| `design/fonts/` | 自托管字体（Albert Sans / IBM Plex Mono，OFL 1.1） |

## 设计系统

- 来源：**鉴枢设计系统（AuthNexus Design）v0.1.0**，复制于 2026-11-24，取自仓库内 `src/design/`（与桌面端同一版本）。
- 引入顺序：`fonts → tokens → brand → base → components → motion → site.css`。官网不使用中后台外壳，因此未引入 `shell.css`。
- 站点层只用 `ks-` 前缀；不改写任何 `anx-` 资产组件。状态类按设计系统约定使用 `aria-current` / `aria-expanded` 等。
- 颜色、间距、圆角、动效均走令牌。唯一例外是 `--ks-term-*`：终端预览需要两种主题下都保持深底，与桌面端 `--kh-term-*` 的处理一致。
- 下载卡的按钮是站点自有按钮（`.ks-dl__btn`），因为酸橙绿卡片需要在明暗主题下表现一致，避免局部强制浅色主题而复制一套令牌。

## 版本号与下载链接（自动同步）

页面加载后会异步请求 GitHub 最新 Release：

```text
https://api.github.com/repos/YimoLab/moyao-keyhub/releases/latest
```

成功后自动更新：

- 所有 `[data-kh-version]` 的版本号（首屏、下载区、页脚）
- 所有 `[data-kh-download]` 的安装包链接（首屏与下载按钮）
- 所有 `[data-kh-size]` 的安装包体积（首屏 meta 与下载区标题）
- JSON-LD 的 `softwareVersion` 与 `downloadUrl`

结果缓存在 `localStorage`（键 `keyhub.latestRelease.v1`，6 小时），减少重复请求。请求失败、限流、离线或仓库改名时静默回退到 HTML 中的静态值，页面不会报错。

因此**发版后通常不需要改页面**。仍建议把 HTML 里的静态值保持为最新版本，作为兜底：

- 首屏 meta、下载区 meta 与页脚的 `v0.2.10`
- 两个 `data-kh-download` 的兜底链接
- 两处 `data-kh-size` 的兜底体积
- JSON-LD 的 `softwareVersion` / `downloadUrl`

如果仓库迁移或改名，同步修改 `assets/site.js` 里的 `RELEASE_API`。

> 官网会向 GitHub API 发起一次只读请求，不发送访客数据。GitHub 未认证接口限流为 60 次/小时/IP；访问量大的公开站点建议换成自己的版本端点或加一层服务端缓存。

## 统计

### 下载量

下载卡的「安装包累计下载」来自 GitHub Releases API，汇总所有正式 Release 中 `.exe` 资产（排除 `.sig`）的 `download_count`。它与版本同步共用一次请求：

- API：`/repos/YimoLab/moyao-keyhub/releases?per_page=100`
- 缓存 6 小时（`localStorage` 键 `keyhub.releases.v2`）
- 接口失败时整项隐藏，不显示 `0` 或伪造数字

这个数字包含官网按钮、GitHub 页面和客户端自动更新触发的安装包下载，所以是「安装包下载次数」，不等同于独立用户数。

### 访问量

使用 Cloudflare Web Analytics（无 Cookie、无跨站追踪）：

1. Cloudflare Dashboard → **Analytics & Logs** → **Web Analytics**。
2. **Add a site**，填写 `keyhub.casuainess.cn`。域名已通过 Cloudflare 代理，默认使用自动安装。
3. 如果 Workers 自定义域名没有自动注入，在 **Manage site** 复制 JS snippet，粘到 `index.html` 的 `</body>` 前。CSP 已允许所需域名：
   - `script-src https://static.cloudflareinsights.com`
   - `connect-src https://cloudflareinsights.com`
4. 数据在 Cloudflare Web Analytics 后台查看：访问量、路径、来源、国家、设备等，通常延迟几分钟。

说明：

- 官网的访问统计与桌面程序无关。墨钥桌面端仍然是默认零网络请求、不含遥测。
- 官网的网络请求只有两类：GitHub Releases API（版本与下载量）和 Cloudflare Web Analytics（如果启用）。
- 如果要在官网页面上直接显示访问量，需要自建 Worker + KV 计数器，且计数容易被刷新和爬虫污染；当前没有实现。

## 反馈通道

页面提供两个反馈入口，位于 FAQ 之后的「反馈」区块，导航与页脚均有锚点：

- **GitHub Issues**：`https://github.com/YimoLab/moyao-keyhub/issues`，适合可公开的 bug 报告与讨论，处理进度可追踪。
- **邮件反馈**：`suggest-keyhub@casuainess.cn`，提供 `mailto:` 链接与一键复制按钮，适合不适合公开的内容。

复制按钮由 `assets/site.js` 驱动：优先使用 Clipboard API，权限被拒时回退到 `document.execCommand('copy')`；两者都失败时按钮显示「复制失败」，邮箱文本仍可手动选择，不会假装成功。按钮初始带 `hidden`，脚本加载后才显示，因此无 JavaScript 时只呈现 `mailto:` 链接，不会出现点不动的按钮。

修改反馈邮箱时，搜索并同步以下位置：

- `index.html` 反馈区的 `mailto:` 与 `data-kh-copy`
- `index.html` 页脚的「邮件反馈」链接
- `index.html` JSON-LD 的 `contactPoint.email`

反馈区还包含一条安全提示（不要附私钥、口令或服务器凭据），修改文案时保留这一约束。

## 更新截图

截图来自实际运行的桌面应用（`src-tauri/target/release/KeyHub.exe`），不是设计稿或渲染图。步骤：

1. 启动应用，并给 WebView2 开远程调试端口：

   ```bash
   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333" ./src-tauri/target/release/KeyHub.exe
   ```

2. 通过 `http://127.0.0.1:9333/json/list` 拿到页面 WebSocket 地址，用 CDP 的 `Page.captureScreenshot` 截取各页面（哈希路由：`#/key-vault`、`#/diagnostics`、`#/security`）。
3. **替换真实数据**：截图中的密钥文件名、注释、指纹、Host 别名、Git 用户名都属于本机隐私，必须在渲染层替换为演示值后再截，或使用隔离的演示环境。不要把真实主机名写进仓库。
4. 转成 WebP，输出 2240 与 1120 两档，放到 `assets/shots/`，文件名保持 `key-vault` / `generate` / `diagnostics` / `security`。
5. 同步更新 `<img>` 的 `width` / `height` 与 `alt`。

## 部署到 Cloudflare

`site/` 是纯静态输出，没有构建步骤。**不要在 Cloudflare 里使用 `npm run build`**，那构建的是桌面端前端。

Cloudflare 现在默认创建 **Worker**（Workers Static Assets）。仓库根的 `wrangler.jsonc` 已经把 `site/` 配成静态资产目录：

```jsonc
{
  "name": "keyhub-site",
  "compatibility_date": "2026-11-24",
  "assets": {
    "directory": "./site",
    "not_found_handling": "404-page"
  }
}
```

Workers Static Assets 原生支持 `_headers` 与 `_redirects`，`not_found_handling: "404-page"` 会让不存在的路径返回 `site/404.html`。

### 当前控制台（Worker + Git）

截图里的字段这样填：

| 字段 | 值 |
|---|---|
| 项目名称 | `keyhub-site` |
| 构建命令 | **清空**（删掉默认的 `npm run build`） |
| 部署命令 | `npx wrangler deploy` |
| 预览命令 | `npx wrangler preview`（保持默认） |

点「部署」。之后每次 push `main` 会自动重新部署。自定义域名在项目 → **Settings** → **Domains & Routes** 中添加。

注意：

- 构建命令留空后，Cloudflare 可能仍会安装仓库根目录的依赖，但不会运行桌面端构建。
- `npx wrangler deploy` 读取仓库根的 `wrangler.jsonc`；确认它已提交并推送。
- `wrangler.jsonc` 的 `name` 必须和 Cloudflare 项目名一致，改名时同步修改。

### 命令行直接部署

```bash
npx wrangler login
npx wrangler deploy
```

在仓库根执行，读取 `wrangler.jsonc`。本地会生成 `.wrangler/`，已加入 `.gitignore`。

### 旧版 Pages 流程

如果账户仍使用 Pages：

| 配置项 | 值 |
|---|---|
| Project name | `keyhub-site` |
| Production branch | `main` |
| Framework preset | `None` |
| Build command | **留空** |
| Build output directory | `site` |
| Root directory | **留空** |

Pages 会自动应用 `_headers` 与 `_redirects`，并使用根目录的 `404.html`。

### 发布相关文件

| 文件 | 作用 |
|---|---|
| `_headers` | 安全头与按路径分组的缓存策略 |
| `404.html` | Worker 由 `not_found_handling` 启用；Pages 会自动使用 |
| `robots.txt` | 允许抓取并指向 sitemap |
| `sitemap.xml` | 单页 sitemap，指向正式域名 |

CSS / JS / 截图的文件名不带内容哈希，所以缓存策略设为 `must-revalidate`：浏览器每次用 ETag 验证，更新样式后能立即生效。字体文件名稳定，缓存 7 天。

**CSP 的关键一条**：`_headers` 里的 `connect-src` 必须包含 `https://api.github.com`，否则浏览器会拦截页面内的版本自动同步。修改版本端点时，同步改这里和 `assets/site.js` 的 `RELEASE_API`。

### 发布后检查

1. `https://<domain>/` 正常，浅色 / 深色都能切换；
2. 控制台无 CSP 报错；Network 中 `api.github.com/.../releases?per_page=100` 返回 200；
3. 页脚版本、安装包体积与 GitHub 最新 Release 一致；
4. 任意不存在的路径（如 `/not-a-page`）显示自定义 404；
5. 正式域名为 `https://keyhub.casuainess.cn/`；`canonical`、`og:url`、`og:image`、`twitter:image`、JSON-LD 的 `url` 与 `robots.txt` 里的 sitemap 都已指向它。换域名时同步修改这些位置，避免默认域名（`*.workers.dev` / `*.pages.dev`）与正式域名重复收录；
6. 两个下载按钮都能下载到最新安装包；
7. 下载卡的「安装包累计下载」显示数字，且与 GitHub Releases 中各 `.exe` 的下载次数之和一致；接口失败时该项隐藏是预期行为；
8. 如果启用了 Cloudflare Web Analytics，后台能看到访问数据；
9. 反馈区的 GitHub 按钮指向 Issues 列表，邮件地址可打开邮件客户端；复制按钮点击后显示「已复制」，无 JS 时复制按钮隐藏、`mailto:` 仍可用。

### 说明

- 版本号、体积与下载链接在运行时从 GitHub 自动同步；发版后不必重新部署，除非要更新截图、文案或静态兜底值。
- GitHub 未认证 API 限流为 60 次/小时/IP。访问量大时建议换成自己的版本端点，并在 `_headers` 的 `connect-src` 与 `site.js` 的 `RELEASE_API` 中同步修改。

其他静态托管（Nginx、对象存储）同样只需上传整个 `site/` 目录；保持 `design/` 与 `assets/` 的相对路径不变，并把 `_headers` 的等价配置加到托管平台。

## 浏览器支持与降级

- 主题切换：Chromium 111+ 使用 View Transitions 做圆形涟漪；不支持时回落到平色圆盘动画；`prefers-reduced-motion` 下直接切换。
- `color-mix()` 用于少量半透明混合；不支持时对应装饰会退化为透明，不影响文字可读。
- 无 JavaScript 时：内容、链接、FAQ 全部可用；主题使用首帧脚本的默认值；入场动画不播放；版本号与下载链接使用 HTML 静态兜底值。
- 版本自动同步失败时：保留静态兜底值，不弹错误、不影响其它交互。

## 验收

```bash
# 鉴枢用法静态检查（在仓库根目录）
node E:/CasuaIness/Personal/authnexus-design/tools/lint-usage.mjs site
```

`site/index.html` 与 `site/assets/site.js` 应为 PASS。CSS 的 `no-raw-color` WARN 来自 `--ks-term-*` 终端令牌定义，属于有意保留的例外；`site/design/` 下的资产自身告警是已知误报，与源仓库一致。
