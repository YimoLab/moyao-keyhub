<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="src/assets/brand/logo-dark.svg">
    <img src="src/assets/brand/logo.svg" width="112" height="112" alt="墨钥 KeyHub">
  </picture>
</p>

<h1 align="center">墨钥 KeyHub</h1>

<p align="center">
  多平台 SSH 密钥管理桌面程序<br>
  <sub>零命令行 · 防错兜底 · 安全合规 · 全流程覆盖</sub>
</p>

<p align="center">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white">
  <img alt="Vue 3" src="https://img.shields.io/badge/Vue-3-42B883?logo=vue.js&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.77+-000000?logo=rust&logoColor=white">
  <img alt="Windows" src="https://img.shields.io/badge/Windows-10%20%2F%2011-0078D4?logo=windows&logoColor=white">
</p>

<p align="center">
  <a href="#功能">功能</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="#安全与隐私">安全与隐私</a> ·
  <a href="#架构">架构</a> ·
  <a href="#发布与自动更新">发布</a> ·
  <a href="#常见问题">常见问题</a>
</p>

---

**墨钥 KeyHub** 把「密钥生成 → 公钥部署 → `~/.ssh/config` 编辑 → 权限校准 → 连通诊断」这条原本只能在终端里完成的链路，变成一套可视化、可回滚、可审计的桌面流程。基于 **Tauri 2 + Vue 3 + Rust**。

> **私钥始终只存在于你的 `~/.ssh`。** 程序不保存副本、不上传任何数据，每次写入前自动备份。默认不发起网络请求；唯一的可选联网行为是更新检查（默认关闭，开启后也只读取 GitHub 的版本文件）。

## 功能

### 环境自检

打开应用先确认环境是否就绪，状态常驻侧栏。

- 检测系统 OpenSSH（`ssh` / `ssh-keygen` / `ssh-add`）与 Git 的可用性和版本
- 检查 `~/.ssh` 目录及敏感文件权限（Unix `700/600`，Windows owner-only ACL）
- Windows 支持一键启用系统 OpenSSH 组件，首次启动进入引导向导
- 独立的「环境自检」页面可随时重新体检

### 密钥库

`~/.ssh` 的图形界面，但不会替你保管私钥。

- 扫描全部密钥：算法、位数、SHA256 指纹、注释、创建时间、权限、口令状态
- 生成 ED25519 / RSA（2048/3072/4096），可选口令保护与强度提示
- 公钥一键复制；私钥查看需二次确认且默认脱敏
- 删除需手动键入文件名；超过 365 天的密钥给出轮换提醒
- 内置 GitHub / GitLab / Linux 服务器 / NAS 部署指引

### SSH Config 可视化编辑

左侧 Host 列表 + 中部编辑器 + 右侧实时校验，三栏联动。

- **Form / Raw 双模式**：结构化编辑，或直接改全文
- **只动自己托管的块**：重写范围限定在 `# ─── KeyHub managed block begin/end ───` 内，块外的注释、缩进、`ProxyJump`、`Match` 原样保留
- **保存前校验**：Host 重复、端口越界、`IdentityFile` 不存在、行尾注释等问题会指出具体 Host 与改法
- **自动备份**：每次写入前生成备份，可一键回滚

### 权限修复

权限不合规时，不用去记 `icacls` 或 `chmod` 的参数。

- 体检目录、私钥、`config`、`known_hosts`、`authorized_keys`
- 不合规项优先展示，可单项或一键全部修复
- Windows 用 `icacls` 关闭继承并仅授权当前用户，Unix 用 `chmod`

### 连通测试与诊断

执行一次真实的 SSH 握手，失败时告诉你下一步做什么。

- 支持「按 Host 别名」和「直连 hostname:port」两种模式
- 执行前展示完整 `ssh` 命令（不联网预览）
- 实时终端输出 + 耗时 + 退出码
- 覆盖超时、拒绝连接、DNS 失败、公钥被拒、agent 未运行、权限过宽、主机指纹变更等十类故障，逐条给出修复动作

### 安全中心

把权限、口令、agent、备份与审计放在一个页面。

- 0–100 安全评分 + 权限 / 口令 / agent / 备份四维进度
- ssh-agent 启停、开机自启、密钥装载与移除
- AES-256-GCM + Argon2id 加密全量备份（`.khbak`），支持检视、恢复、删除
- JSONL 审计日志，可按类别/结果筛选、导出带 BOM 的 CSV、按保留期清理
- 数据不可读取时明确显示「无法评估」，绝不伪造分数

### 设置

- 亮/暗主题、界面密度、标题栏风格、密钥默认值、安全策略、审计保留期
- 标题栏：跟随系统 / Windows（右上角按钮）/ macOS（左上角交通灯），切换后确认重启
- 应用更新：自动检查默认关闭，可手动检查；更新包需通过签名校验
- 本机数据位置一览，可复制路径
- 全局搜索 `Ctrl/Cmd + K`，弹窗支持 `Esc` 关闭

## 快速开始

### 环境要求

| 依赖 | 版本 / 说明 |
|---|---|
| Node.js | ≥ 18（推荐 20 / 22） |
| Rust | ≥ 1.77（推荐 stable） |
| C 工具链 | Windows：MinGW-w64（UCRT）或 MSVC Build Tools；macOS：`xcode-select --install`；Linux：`build-essential` + `libwebkit2gtk-4.1-dev` 等 |
| OpenSSH | 运行期必需，应用会调用系统原生 `ssh` / `ssh-keygen` / `ssh-add` |

### 运行

```bash
npm install

npm run app:dev   # 桌面应用（热重载）
npm run dev       # 仅前端浏览器预览（无后端能力）
```

> 浏览器预览模式下所有 Tauri 命令不可用，界面会明确提示「后端未连接」，不会用假数据伪装成功。

### 打包

```bash
# Windows：只出 NSIS（无需预装打包器）
npm run app:build -- --bundles nsis

# macOS / Linux：自动产出当前平台可用的包
npm run app:build
```

产物在 `src-tauri/target/release/bundle/`。实测 release 可执行文件 **4.31 MB**、NSIS 安装包 **1.65 MB**。

<details>
<summary><b>Windows 构建注意事项</b></summary>

- 不要直接跑 `npm run app:build`：`bundle.targets` 含 `msi`，需要额外安装 WiX Toolset，未安装会在漫长的编译结束后才失败。
- 使用 GNU 工具链时，`Cargo.toml` 的 `crate-type` 必须是 `["rlib"]`，不要改回 Tauri 模板的三件套（会触发 PE 导出序号溢出）。
- `WebView2Loader.dll` 是 GNU 工具链下 exe 的运行时依赖，已由 `build.rs` + `bundle.resources` 自动打进安装包，这两处不要删。
- `productName` 必须是纯 ASCII（当前为 `KeyHub`）：Tauri 用它拼安装目录、卸载注册表键和安装包名。窗口标题和界面里的中文品牌名由 `app.windows[].title` 与前端文案承载。改动 `productName` 会改变安装路径，必须同步维护 `src-tauri/nsis/legacy-migration.nsh`（见「安装路径迁移」）。

</details>

### 校验

```bash
# 前后端命令契约（71 个命令双向比对）
node scripts/check_api_contract.mjs

# UI 回归（需另开终端跑 vite preview）
npm run build
npx vite preview --port 5199 --strictPort
node scripts/dom_check.mjs http://localhost:5199/#/key-vault
node scripts/interact_check.mjs http://localhost:5199/#/key-vault
```

> 完整原理与踩坑记录保留在本地 `docs/` 目录，不随仓库分发。

## 安全与隐私

**九条硬约束：**

1. **私钥零副本**：不缓存私钥内容，不写临时文件。
2. **明文读取需显式确认**：查看私钥前必须二次确认，每次调用写 `warn` 级审计。
3. **口令不落命令行**：带口令的密钥拒绝自动加入 `ssh-add`（参数会被其它进程读到）。
4. **路径穿越防御**：文件名白名单 + 拒绝 `.` 开头与 `..`，绝对路径只在 Rust 侧计算。
5. **命令注入防御**：`core.sshCommand` 拒绝 `;` `&` `|` `` ` `` `$` 等元字符。
6. **原子写**：配置文件先写 `.tmp` 再 `rename`，避免半截文件。
7. **强随机数强制开启**：`strictCsprng` 恒为 `true`，UI 上锁定不可关闭。
8. **最小权限声明**：`capabilities/default.json` 不注册任何通配文件系统命令。
9. **失败不伪装**：读取失败时评分、统计与环境状态显示「无法评估 / 未知」，绝不渲染为 0 或「合规」。

<details>
<summary><b>已知边界</b></summary>

- 私钥明文需要跨 IPC 传到 WebView 供用户查看，Rust 侧未引入 `zeroize`。这是有意识的产品取舍：私钥明文最终必然进入 WebView 的 JS 堆，单在 Rust 侧清零收益有限。更彻底的方案是不在前端展示明文，改为「复制到剪贴板」或「在系统终端中打开」。
- 原生 `<select>` 展开的选项弹层由系统 WebView 渲染，圆角与阴影无法统一定制，这是平台限制；项目中的下拉已统一改用自绘 `SelectMenu`。

</details>

## 数据存放

| 内容 | 位置 | 说明 |
|---|---|---|
| 私钥 / 公钥 | `~/.ssh/` | 唯一真实存放位置，程序不复制 |
| `config` | `~/.ssh/config` | 只重写墨钥托管的注释块 |
| `known_hosts` | `~/.ssh/known_hosts` | 可查看 / 移除条目 |
| 用户偏好 | `<应用数据目录>/KeyHub/prefs.json` | Windows：`%APPDATA%\KeyHub\` |
| 审计日志 | `<应用数据目录>/KeyHub/audit.log` | JSONL，一行一条 |
| Config 备份 | `<应用数据目录>/KeyHub/config-backups/` | 每次写入前生成 |
| 加密备份 | `<应用数据目录>/KeyHub/backups/` | `.khbak` 格式 |

<details>
<summary><b><code>.khbak</code> 文件布局</b></summary>

```text
┌────────┬──────────┬───────────┬─────────────────────────┐
│ magic  │   salt   │   nonce   │      ciphertext         │
│  8 B   │   16 B   │    12 B   │   AES-256-GCM 密文      │
│KHBAK1\0│          │           │  （内含 deflate 压缩包） │
└────────┴──────────┴───────────┴─────────────────────────┘
```

密钥由口令经 **Argon2id** 派生。备份包含 config、公钥、私钥、审计日志与偏好，**排除** `known_hosts` 与 `authorized_keys`。

</details>

## 架构

```text
┌──────────────────────────────────────────────────────────┐
│ 表现层  src/views/*.vue + src/components/*.vue           │
│         Vue 3，纯本地渲染，不含业务规则                   │
├──────────────────────────────────────────────────────────┤
│ 桥接层  src/lib/api.js                                   │
│         invoke 封装；把 "CODE:msg" 拆成 e.code/e.message │
├──────────────────────────────────────────────────────────┤
│ 命令层  src-tauri/src/lib.rs                             │
│         71 个 #[tauri::command]：参数校验 + 审计 + 领域调用 │
├──────────────────────────────────────────────────────────┤
│ 领域层  env · keys · config · conn_test · agent · git    │
│         backup · audit · prefs                           │
├──────────────────────────────────────────────────────────┤
│ 基础层  paths · perms · error · time_util                │
├──────────────────────────────────────────────────────────┤
│ 系统层  OpenSSH · icacls / chmod · PowerShell / launchd  │
└──────────────────────────────────────────────────────────┘
```

**依赖方向严格单向向下。** `paths` 是唯一的路径权威——前端只传文件名，绝对路径全部在 Rust 侧计算，从结构上杜绝路径穿越。

| 模块 | 职责 |
|---|---|
| `env` | 环境检测、安装引导、平台识别 |
| `keys` | 密钥扫描 / 生成 / 读取 / 删除 / 口令管理 |
| `config` | SSH Config 解析、校验、托管块重写、备份回滚 |
| `conn_test` | 连通测试执行与十类故障诊断 |
| `agent` | ssh-agent 状态、启停、密钥装载、自启 |
| `git` | Git 全局配置与按 Host 绑定密钥 |
| `backup` | AES-256-GCM 加密备份与恢复 |
| `audit` | JSONL 审计、统计、CSV 导出、清理 |
| `prefs` | 用户偏好持久化与取值规整 |
| `paths` / `perms` / `error` / `time_util` | 路径、跨平台权限、统一错误、时间格式 |

### 技术栈

| 层 | 选型 |
|---|---|
| 桌面框架 | **Tauri 2.x**（Rust 后端 + 系统 WebView，无边框窗口 + 自绘标题栏） |
| 前端 | Vue 3 Composition API + Vue Router 4（hash 路由）+ Vite 6 |
| 后端 | Rust（edition 2021） |
| 加密 | `aes-gcm`（AES-256-GCM）+ `argon2`（Argon2id） |
| 更新 | Tauri updater（GitHub Releases + minisign 签名校验） |
| 打包 | NSIS（Windows，当前 Release 目标；MSI/macOS/Linux 待后续） |

选择 Tauri 而非 Electron：安装包小一个数量级、内存占用低，Rust 侧可以精确控制子进程与文件操作边界，这正是本项目安全模型的前提。

## 项目结构

<details>
<summary><b>展开目录树</b></summary>

```text
KeyHub/
├── .github/workflows/
│   ├── release.yml              tag 触发：构建 NSIS + 生成 latest.json（草稿 Release）
│   └── patch-latest.yml         为已有 Release 重生成 latest.json
├── release-notes/               各版本发行说明，同时用于 Release 正文与 latest.json
├── scripts/
│   ├── check_api_contract.mjs   前后端命令契约校验
│   ├── dom_check.mjs            UI 回归：断言无弹窗遮罩常驻
│   ├── interact_check.mjs       UI 回归：弹窗开/关闭环
│   └── gen_icons.py             应用图标生成器
├── site/                        官网（静态单页，零构建）
├── src/
│   ├── main.js                  入口 + 路由表 + 样式引入
│   ├── App.vue                  应用外壳（玻璃侧栏 / 顶栏 / 主区 / 浮层宿主）
│   ├── design/                  鉴枢设计系统资产（css · fonts · vue）
│   ├── styles/                  app.css（项目自有布局与部件）
│   ├── components/              AppModal · AppDrawer · AppErrorState
│   │                            WindowControls · UpdateDialog · PageSkeleton
│   ├── lib/                     api · store · theme · toast · updater · releaseNotes
│   └── views/                   Onboarding · EnvCheck · KeyVault
│                                ConfigEditor · Diagnostics
│                                SecurityCenter · Settings
└── src-tauri/
    ├── src/                     Rust 后端（15 个模块）
    ├── nsis/                    安装钩子（旧版路径迁移）
    ├── capabilities/            最小权限声明
    ├── icons/                   应用图标
    └── tauri.conf.json          窗口 / 打包 / CSP
```

</details>

> 高保真设计稿保留在本地 `ui/` 目录，不随本仓库分发；界面样式的真值源是下方「设计系统」里的资产副本。

## 设计系统

界面使用 **鉴枢设计系统（AuthNexus Design）v0.1.0**，资产已复制到 `src/design/`
（复制于 2026-11-20；上游仓库 `yimolab/authnexus-design`，三平台同名）。

| 路径 | 职责 |
|---|---|
| `src/design/css/tokens.css` | 设计令牌、明暗主题、玻璃降级 |
| `src/design/css/brand.css` | 品牌覆盖（`--acid` / logo 双色 / 字体族）——换品牌只改这里 |
| `src/design/css/base.css` | 元素重置、排版、按钮、表单、表格、标签、提示 |
| `src/design/css/components.css` | 浮层、下拉、分段、分页、Toast |
| `src/design/css/motion.css` | 动效与「减少动效」压缩 |
| `src/design/css/shell.css` | 玻璃侧栏 + 浮层顶栏外壳 |
| `src/design/vue/` | 组件实现（Icon / PageHead / PanelBlock / SelectMenu 等） |
| `src/styles/app.css` | 墨钥自有布局与业务部件，引入在资产之后 |

<details>
<summary><b>设计系统约定</b></summary>

- **只使用 `anx-` 资产类**；业务布局用 `kh-` 前缀，状态类用 `is-*`，不重写资产组件。
- 引入顺序固定：`fonts → tokens → brand → base → components → motion → shell → app.css`，见 `src/main.js`。
- 浮层统一挂到 `#overlay-layer`（`.anx-overlay-layer`），通知挂到 `#toast-layer`（`.anx-toast-layer`）。
- 主题键为 `authnexus.theme`；`index.html` 首帧预置脚本与 `design/vue/lib/theme.ts` 必须同键。
- 下拉一律使用 `SelectMenu`，不使用原生 `<select>`。
- 图标集在 `design/vue/lib/icons.ts` 副本上按项目增补（minimize / maximize / download / upload / rotate / unlock / branch），升级资产时需保留。
- 升级时对照上游 `CHANGELOG.md` 与 `contracts/`，不要只同步部分文件。

</details>

## 官网

线上地址：<https://keyhub.casuainess.cn/>

`site/` 是产品官网的静态单页，介绍程序作用、功能与安全设计，零构建、无框架运行时：

```bash
python -m http.server 8137 --directory site
# 打开 http://127.0.0.1:8137/
```

官网与桌面端共用鉴枢设计系统资产，截图取自实际运行的应用；版本号、安装包体积与下载链接会从 GitHub 最新 Release 自动同步。部署到 Cloudflare 时使用仓库根的 `wrangler.jsonc`（把 `site/` 作为静态资产目录），构建命令留空；完整步骤、下载量统计与访问统计的配置见 `site/README.md`。

## 三端支持

| 关注点 | Windows | macOS | Linux |
|---|---|---|---|
| OpenSSH | 系统可选功能，可一键启用 | 自带 | 通常自带 |
| 权限模型 | NTFS ACL（`icacls`） | `chmod 700/600` | `chmod 700/600` |
| ssh-agent | Windows 服务 | Keychain 集成 | 独立进程 + `SSH_AUTH_SOCK` |
| agent 自启 | `Set-Service` | `launchd` | `systemd --user` |
| 子进程 | `CREATE_NO_WINDOW` 避免黑框 | 无 | 无 |
| 打包 | NSIS + MSI | DMG | deb + AppImage |

当前仅在 **Windows + GNU 工具链**下完成完整构建与安装验证；macOS / Linux 尚未实机回归。

## 发布与自动更新

更新链路：GitHub Release 托管 NSIS 安装包、`.sig` 与 `latest.json`，客户端用 Tauri updater 读取并校验签名。

### 发版步骤

1. 同步版本号（`Cargo.lock` 会随构建更新）：
   - `package.json`
   - `src-tauri/tauri.conf.json`
   - `src-tauri/Cargo.toml`
2. 写发行说明 `release-notes/<tag>.md`。它会同时用于 GitHub Release 正文和 `latest.json` 的 `notes`，客户端更新弹窗直接渲染；文件不存在时回退到默认说明。
3. 本地跑一次 `npm run build` 和 `cargo check`。
4. 提交并推送 `main`。
5. 打 tag 并推送：

   ```bash
   git tag v0.2.6
   git push origin v0.2.6
   ```

6. 等待 GitHub Actions 的 `Release` 工作流完成（Windows NSIS）。
7. 打开 Releases，核对草稿中的三件套：
   - `KeyHub_<version>_x64-setup.exe`
   - `...setup.exe.sig`
   - `latest.json`
8. 手动 **Publish release**。只有发布后，客户端才能读到 `latest.json`。

### 工作流做了什么

`.github/workflows/release.yml`

1. `npm ci` + Rust stable（Windows MSVC）。
2. **校验签名密钥**：Secret 缺失或格式错误会在构建前直接失败。
3. **清理同 tag 旧草稿**：避免重跑时报 `ReleaseAsset already_exists`。
4. **读取发行说明**：写入 Release 正文与 `latest.json` 的 `notes`。
5. `tauri-action` 构建 NSIS、上传安装包与 `.sig`（`includeUpdaterJson: false`）。
6. **自行生成 `latest.json`**：签名取自 `*.exe.sig`，下载地址构造为正式 tag URL，用 `gh release upload --clobber` 上传。
7. 创建**草稿 Release**，等待人工发布。

> 不要用草稿 Release 的 `browser_download_url` 作为更新地址：草稿阶段它是 `untagged-...` 临时链接，发布后返回 404。

### 更新端点

```text
https://github.com/YimoLab/moyao-keyhub/releases/latest/download/latest.json
```

### 签名密钥（必须保管好）

- 公钥已写入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。
- 私钥与密码只在本机 / CI Secrets，**绝不提交**。
- GitHub 仓库 Settings → Secrets and variables → Actions 添加：
  - `TAURI_SIGNING_PRIVATE_KEY`（私钥文件完整内容）
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（私钥密码）
- 本机私钥：`~/.tauri/moyao-keyhub.key`，密码在同目录 `moyao-keyhub.password.txt`。
- 私钥或密码丢失后无法再签发更新，只能换密钥并让用户重新安装。

### 隐私约定

- 自动检查更新默认关闭，首次进入主界面提示一次；可在设置 → 应用更新中开启。
- 开启后启动 5 秒检查一次，6 小时内不重复。
- 只 GET `latest.json`，不上传任何信息，不包含遥测。
- 更新包签名校验失败会被拒绝安装。

### 验证发布

```bash
# 1. latest.json 可访问，version 正确
curl -L https://github.com/YimoLab/moyao-keyhub/releases/latest/download/latest.json

# 2. platforms.windows-x86_64.url 必须是 /releases/download/vX.Y.Z/... 的正式地址
#    不能是 untagged-...

# 3. 安装包可下载（HTTP 200）
curl -IL https://github.com/YimoLab/moyao-keyhub/releases/download/v0.2.6/KeyHub_0.2.6_x64-setup.exe

# 4. latest.json 中的 signature 与 Release 里的 .sig 内容完全一致
```

<details>
<summary><b>排障记录（0.2.0 首次发版）</b></summary>

| 现象 | 原因 | 修复 |
|---|---|---|
| CI 前端构建找不到 `./design/vue/ui/Icon.vue` | `.gitignore` 的 `ui/` 误伤了 `src/design/vue/ui/` | 改为 `/ui/`（只忽略根目录设计稿），补齐 21 个组件 |
| CI `tauri build` 在干净检出中失败 | `bundle.resources` 引用了 `binaries/WebView2Loader.dll`，但 `src-tauri/binaries/` 被忽略 | 把 DLL 纳入仓库 |
| 重跑 tag 报 `ReleaseAsset already_exists` | 上一次运行已把同名安装包上传到草稿 | 构建前自动删除同 tag 草稿（只删 draft） |
| 草稿里没有 `latest.json` | `tauri-action` 的 JSON 生成在 NSIS 下未触发；参数名也曾写错 | 关闭 action 的 JSON 生成，改为构建后自行生成并上传 |
| `latest.json` 的安装包 URL 是 `untagged-...`，发布后 404 | 草稿阶段的 `browser_download_url` 是临时地址 | 用 tag + asset 名构造正式下载 URL |
| 已发布 Release 的 `latest.json` 覆盖失败 | 同名 asset 覆盖在 `gh release upload` 下不稳定 | 在 Release 编辑页删除旧 `latest.json` 后再上传 |
| 终端里中文 `notes` 显示成 `īԿ` | Windows 控制台代码页的显示问题，文件本身是正确的 UTF-8 | 用浏览器的 `latest.json` 或 `ensure_ascii=True` 确认 |

</details>

<details>
<summary><b>安装路径迁移（productName 改为英文）</b></summary>

早期版本的 `productName` 是中文「墨钥 KeyHub」，安装目录随之带中文。中文路径在部分系统与子进程工具里会被错误解码，因此改为纯英文：

| 项目 | 旧（≤ 0.2.x） | 新 |
|---|---|---|
| 安装目录 | `%LOCALAPPDATA%\墨钥 KeyHub` | `%LOCALAPPDATA%\KeyHub` |
| 主程序 | `keyhub.exe` | `KeyHub.exe` |
| 安装包 | `墨钥 KeyHub_<ver>_x64-setup.exe` | `KeyHub_<ver>_x64-setup.exe` |
| 开始菜单 / 卸载项显示名 | 墨钥 KeyHub | KeyHub |

窗口标题、界面文案和应用数据位置（`~/.ssh`）都不变。

NSIS 按 `Software\<manufacturer>\<productName>` 判断是否已安装，产品名一改就找不到旧版本。`src-tauri/nsis/legacy-migration.nsh` 会在拷贝新文件前读取旧卸载表、静默卸载旧版，并在安装完成后清理残留目录，保证只剩一份安装；旧版本不存在时该钩子是空操作。

验证迁移时注意：

- 旧版卸载器会把自身复制到临时目录后返回，旧目录可能在新版装完前还没删净，钩子在 `NSIS_HOOK_POSTINSTALL` 里补了一次 `RMDir /r`。
- 旧的中文快捷方式由旧卸载器删除，新版会按新名字重建（静默更新的安装器也会创建桌面快捷方式）。

</details>

<details>
<summary><b>首次版本注意</b></summary>

`0.1.0` 发布时还没有 updater 代码，所以“旧版自动升级到 0.2.0”无法验证；从 `0.2.x` 开始的下一次版本才能真正端到端验证自动更新。

</details>

## 开发约定

- **设计系统是视觉真值源**：改 UI 前先看 `src/design/` 与上方「设计系统」约定，上游参考实现见本机 `E:/CasuaIness/Personal/authnexus-design`。
- **不硬编码颜色**：一律走 `var(--token)`；唯一例外是终端恒深底色（`--kh-term-*`），它在两种主题下都不反转。
- **不重写设计系统组件**：迁移与升级只复制文件、改 import；业务差异放在 `src/styles/app.css` 或视图自身。
- **错误必须显式呈现**：不允许 `catch {}` 静默吞掉；失败要有 toast 或行内状态。
- **未知态不等于失败态**：读不到数据时显示「未知 / 无法评估」，不要折叠成 `0`、`false` 或「未安装」。
- **危险操作三重奏**：提示风险 → 列出影响 → 要求确认。
- **审计留痕**：新增敏感操作时，在命令层补 `audit::log(...)`。
- **文档与代码同步**：功能变更后同步更新 README。

<details>
<summary><b>自定义组件契约</b></summary>

- `AppModal` / `AppDrawer` 的显隐由 `v-model` 控制；内部在 `#overlay-layer` 渲染鉴枢结构（`.anx-scrim` + `.anx-modal` / `.anx-drawer`），负责滚动锁、`Esc`、焦点陷阱与离场动画。
- 底部按钮区使用 `<template #footer>`；`dismissable=false` 只禁止遮罩 / `Esc` 关闭，右上角 X 始终可关。
- 路由组件必须是**单根元素**：页面用 `RouteFade` 包裹，多根组件会让离场与滚动复位行为不稳定。

</details>

## 常见问题

**Q：浏览器预览里为什么什么都点不动？**
A：浏览器模式没有 Rust 后端，系统能力不可用。请用 `npm run app:dev` 启动桌面应用；界面会明确提示「后端未连接」，这是刻意设计。

**Q：Windows 上 SSH 报「Permissions are too open」？**
A：进「安全中心」一键修复权限。Windows 下墨钥用 `icacls` 关闭 ACL 继承，只保留当前用户的完全控制。

**Q：程序会不会把密钥传到网上？**
A：不会。默认零网络请求，唯一联网动作是你在「连接测试」里显式触发的 SSH 握手。

**Q：如何退出对某个 Host 的管理？**
A：手动删除 `config` 中 `# ─── KeyHub managed block begin ───` 到 `end` 之间的内容即可，块外配置不受影响。

## 品牌资产

Logo 为「墨印 · 钥孔」：墨黑圆角方印、白色钥匙孔、右下酸橙绿点。

| 资产 | 位置 | 说明 |
|---|---|---|
| 主 Logo | `src/assets/brand/logo.svg` | 浅色背景用（黑印白孔） |
| 深色版 | `src/assets/brand/logo-dark.svg` | 深色背景用（白印黑孔） |
| UI 标记 | `src/components/BrandMark.vue` | 随鉴枢令牌自动反转，侧栏 30px、标题栏 20px |
| 应用图标 | `src-tauri/icons/` | `.ico` / `.icns` / PNG 全套，由 Logo 生成 |
| favicon | `public/favicon.svg` | 浏览器标签页 |

修改 Logo 的流程：

```bash
# 1. 编辑 src/assets/brand/logo.svg
# 2. 重新生成应用图标（推荐，支持 SVG）
npx tauri icon src/assets/brand/logo.svg -o src-tauri/icons
# 3. 需要离线/自定义尺寸时，也可运行 Pillow 脚本
python scripts/gen_icons.py
```

图标生成后需要重新 `npm run app:build`，任务栏、开始菜单与安装包才会使用新图标。

## 文档与许可

本仓库以 README 作为唯一文档。产品需求与开发记录保留在本地 `docs/` 目录，不随仓库分发。

私有项目，未公开发布。
