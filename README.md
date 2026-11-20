# 墨钥 KeyHub

> 多平台 SSH 密钥管理桌面程序 —— 零命令行、防错兜底、安全合规、全流程覆盖

Windows / macOS / Linux 三端桌面应用，基于 **Tauri 2 + Vue 3 + Rust**。
把「密钥生成 → 公钥部署 → `~/.ssh/config` 编辑 → 权限校准 → 连通诊断」这条过去只能在终端里完成的链路，变成一套可视化、可回滚、可审计的桌面流程。

**核心承诺：私钥始终只存在于你的 `~/.ssh`，程序不保存副本、默认不发起任何网络请求、每次写操作前先备份。**

---

## 功能

### 环境自检
- 检测系统 OpenSSH（`ssh` / `ssh-keygen` / `ssh-add`）与 Git 的可用性和版本
- 检查 `~/.ssh` 目录及敏感文件权限（Unix `700/600`，Windows owner-only ACL）
- Windows 支持一键启用系统 OpenSSH 组件；首次启动进入引导向导
- 环境状态常驻侧栏，随时可见；独立的 `/env-check` 页面可随时重新体检

### 密钥库
- 扫描 `~/.ssh` 下全部密钥：算法、位数、SHA256 指纹、注释、创建时间、权限、口令状态
- 生成 ED25519 / RSA（2048/3072/4096），可选口令保护与强度提示
- 公钥一键复制；私钥查看需二次确认且默认脱敏
- 删除密钥需手动键入文件名；超过 365 天的密钥给出轮换提醒
- 内置 GitHub / GitLab / Linux 服务器 / NAS 部署指引

### SSH Config 可视化编辑
- 左侧 Host 列表 + 中部编辑器 + 右侧实时校验，三栏联动
- Form / Raw 双模式，完整保留注释、缩进、`ProxyJump`、`Match` 等手写内容
- 只重写托管块内的配置（`# ─── KeyHub managed block begin/end ───`）
- 保存前校验 Host 重复、端口越界、`IdentityFile` 不存在等；每次写入前自动备份，支持一键回滚

### 权限修复
- 体检目录、私钥、`config`、`known_hosts`、`authorized_keys`
- 不合规项优先展示，可单项或一键全部修复
- Windows 用 `icacls` 关闭继承并仅授权当前用户，Unix 用 `chmod`

### 连通测试与诊断
- 支持「按 Host 别名」和「直连 hostname:port」两种模式
- 执行前展示完整 `ssh` 命令（不联网预览）
- 实时终端输出 + 耗时 + 退出码
- 覆盖超时、拒绝连接、DNS 失败、公钥被拒、agent 未运行、权限过宽、主机指纹变更等十类故障，逐条给出修复动作

### 安全中心
- 0–100 安全评分 + 权限 / 口令 / agent / 备份四维进度
- ssh-agent 启停、开机自启、密钥装载与移除
- AES-256-GCM + Argon2id 加密全量备份（`.khbak`），支持检视、恢复、删除
- JSONL 审计日志，可按类别/结果筛选、导出带 BOM 的 CSV、按保留期清理
- 数据不可读取时明确显示「无法评估」，绝不伪造分数

### 设置
- 亮/暗主题、界面密度、密钥默认值、安全策略、审计保留期
- 本机数据位置一览，可复制路径
- 全局搜索 `Ctrl/Cmd + K`；弹窗支持 `Esc` 关闭

---

## 技术栈

| 层 | 选型 |
|---|---|
| 桌面框架 | **Tauri 2.x**（Rust 后端 + 系统 WebView，无边框窗口 + 自绘标题栏） |
| 前端 | Vue 3 Composition API + Vue Router 4（hash 路由）+ Vite 6 |
| 后端 | Rust（edition 2021） |
| 加密 | `aes-gcm`（AES-256-GCM）+ `argon2`（Argon2id） |
| 打包 | NSIS（Windows）/ DMG（macOS）/ deb、AppImage（Linux） |

选择 Tauri 而非 Electron：安装包小一个数量级、内存占用低，Rust 侧可以精确控制子进程与文件操作边界，这正是本项目安全模型的前提。

---

## 快速开始

### 前置要求

1. **Node.js** ≥ 18（推荐 20/22）
2. **Rust** ≥ 1.77（推荐 stable）
3. **C 工具链**
   - Windows：MinGW-w64（UCRT 版）或 MSVC Build Tools
   - macOS：`xcode-select --install`
   - Linux：`build-essential` + `libwebkit2gtk-4.1-dev` 等
4. **OpenSSH 客户端**（运行期必需）

```bash
npm install
npm run app:dev      # 桌面应用（热重载）
npm run dev          # 仅前端浏览器预览（无后端能力）
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

> ⚠️ **Windows 用户注意**
> - 不要直接跑 `npm run app:build`：`bundle.targets` 含 `msi`，需要额外安装 WiX Toolset，未安装会在漫长的编译结束后才失败。
> - 项目使用 GNU 工具链时，`Cargo.toml` 的 `crate-type` 必须是 `["rlib"]`，不要改回 Tauri 模板的三件套（会触发 PE 导出序号溢出）。
> - `WebView2Loader.dll` 是 GNU 工具链下 exe 的运行时依赖，已由 `build.rs` + `bundle.resources` 自动打进安装包，这两处不要删。

完整原理与踩坑记录保留在本地 `docs/` 目录（不随仓库分发）。

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

---

## 架构

```
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

---

## 项目结构

```
KeyHub/
├── scripts/
│   ├── check_api_contract.mjs   前后端命令契约校验
│   ├── dom_check.mjs            UI 回归：断言无弹窗遮罩常驻
│   ├── interact_check.mjs       UI 回归：弹窗开/关闭环
│   └── gen_icons.py             应用图标生成器
├── src/
│   ├── main.js                  入口 + 路由表
│   ├── App.vue                  应用外壳（标题栏 / 侧栏 / 主区）
│   ├── styles/                  tokens.css · base.css · shell.css
│   ├── components/              AppIcon · AppModal · AppDrawer
│   │                            ToastHost · AppErrorState
│   ├── lib/                     api · store · theme · toast
│   └── views/                   Onboarding · EnvCheck · KeyVault
│                                ConfigEditor · Diagnostics
│                                SecurityCenter · Settings
└── src-tauri/
    ├── src/                     Rust 后端（15 个模块）
    ├── capabilities/            最小权限声明
    ├── icons/                   应用图标
    └── tauri.conf.json          窗口 / 打包 / CSP
```

> 高保真设计稿保留在本地 `ui/` 目录，不随本仓库分发。

---

## 数据与存储

| 内容 | 位置 | 说明 |
|---|---|---|
| 私钥 / 公钥 | `~/.ssh/` | 唯一真实存放位置，程序不复制 |
| `config` | `~/.ssh/config` | 只重写墨钥托管的注释块 |
| `known_hosts` | `~/.ssh/known_hosts` | 可查看 / 移除条目 |
| 用户偏好 | `<应用数据目录>/KeyHub/prefs.json` | Windows：`%APPDATA%\KeyHub\` |
| 审计日志 | `<应用数据目录>/KeyHub/audit.log` | JSONL，一行一条 |
| Config 备份 | `<应用数据目录>/KeyHub/config-backups/` | 每次写入前生成 |
| 加密备份 | `<应用数据目录>/KeyHub/backups/` | `.khbak` 格式 |

`.khbak` 文件布局：

```
┌────────┬──────────┬───────────┬─────────────────────────┐
│ magic  │   salt   │   nonce   │      ciphertext         │
│  8 B   │   16 B   │    12 B   │   AES-256-GCM 密文      │
│KHBAK1\0│          │           │  （内含 deflate 压缩包） │
└────────┴──────────┴───────────┴─────────────────────────┘
```

密钥由口令经 **Argon2id** 派生。备份包含 config、公钥、私钥、审计日志与偏好，**排除** `known_hosts` 与 `authorized_keys`。

---

## 安全设计

1. **私钥零副本**：不缓存私钥内容，不写临时文件。
2. **明文读取需显式确认**：查看私钥前必须二次确认，每次调用写 `warn` 级审计。
3. **口令不落命令行**：带口令的密钥拒绝自动加入 `ssh-add`（参数会被其它进程读到）。
4. **路径穿越防御**：文件名白名单 + 拒绝 `.` 开头与 `..`，绝对路径只在 Rust 侧计算。
5. **命令注入防御**：`core.sshCommand` 拒绝 `;` `&` `|` `` ` `` `$` 等元字符。
6. **原子写**：配置文件先写 `.tmp` 再 `rename`，避免半截文件。
7. **强随机数强制开启**：`strictCsprng` 恒为 `true`，UI 上锁定不可关闭。
8. **最小权限声明**：`capabilities/default.json` 不注册任何通配文件系统命令。
9. **失败不伪装**：读取失败时评分、统计与环境状态显示「无法评估 / 未知」，绝不渲染为 0 或「合规」。

### 已知边界

- 私钥明文需要跨 IPC 传到 WebView 供用户查看，Rust 侧未引入 `zeroize`。这是有意识的产品取舍：私钥明文最终必然进入 WebView 的 JS 堆，单在 Rust 侧清零收益有限。更彻底的方案是不在前端展示明文，改为「复制到剪贴板」或「在系统终端中打开」。
- `<select>` 展开的选项弹层由系统 WebView 渲染，圆角与阴影无法统一定制，这是平台限制。

---

## 三端差异

| 关注点 | Windows | macOS | Linux |
|---|---|---|---|
| OpenSSH | 系统可选功能，可一键启用 | 自带 | 通常自带 |
| 权限模型 | NTFS ACL（`icacls`） | `chmod 700/600` | `chmod 700/600` |
| ssh-agent | Windows 服务 | Keychain 集成 | 独立进程 + `SSH_AUTH_SOCK` |
| agent 自启 | `Set-Service` | `launchd` | `systemd --user` |
| 子进程 | `CREATE_NO_WINDOW` 避免黑框 | 无 | 无 |
| 打包 | NSIS + MSI | DMG | deb + AppImage |

当前仅在 **Windows + GNU 工具链**下完成完整构建与安装验证；macOS / Linux 尚未实机回归。

---

## 开发约定

- **设计稿是视觉真值源**：改 UI 前先看本地 `ui/design-spec.html`。
- **不硬编码颜色**：一律走 `var(--token)`。
- **错误必须显式呈现**：不允许 `catch {}` 静默吞掉；失败要有 toast 或行内状态。
- **未知态不等于失败态**：读不到数据时显示「未知 / 无法评估」，不要折叠成 `0`、`false` 或「未安装」。
- **危险操作三重奏**：提示风险 → 列出影响 → 要求确认。
- **审计留痕**：新增敏感操作时，在命令层补 `audit::log(...)`。
- **文档与代码同步**：功能变更后同步更新 README。

### 自定义组件契约

- `AppModal` / `AppDrawer` 的显隐由 `v-model` 控制，根节点必须保留 `v-if`，否则全屏遮罩会常驻并吞掉点击。
- 底部按钮区使用 `<template #footer>`；`dismissable=false` 只禁止遮罩 / `Esc` 关闭，右上角 X 始终可关。
- 路由组件必须是**单根元素**：`App.vue` 用 `<transition mode="out-in">` 包裹，多根组件会导致离场无法结束、切页后主区空白。

---

## 文档

本仓库以 README 作为唯一文档。产品需求与开发记录保留在本地 `docs/` 目录，不随仓库分发。

---

## 常见问题

**Q：浏览器预览里为什么什么都点不动？**
A：浏览器模式没有 Rust 后端，系统能力不可用。请用 `npm run app:dev` 启动桌面应用；界面会明确提示「后端未连接」，这是刻意设计。

**Q：Windows 上 SSH 报「Permissions are too open」？**
A：进「安全中心」一键修复权限。Windows 下墨钥用 `icacls` 关闭 ACL 继承，只保留当前用户的完全控制。

**Q：程序会不会把密钥传到网上？**
A：不会。默认零网络请求，唯一联网动作是你在「连接测试」里显式触发的 SSH 握手。

**Q：如何退出对某个 Host 的管理？**
A：手动删除 `config` 中 `# ─── KeyHub managed block begin ───` 到 `end` 之间的内容即可，块外配置不受影响。

---

## 许可

私有项目，未公开发布。
