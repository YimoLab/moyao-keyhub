//! 环境自检（PRD 3.1.1）：双源检测系统原生 OpenSSH 与 Git 内置 OpenSSH，
//! 并检测 Git、`~/.ssh` 目录、ssh-agent 服务。
//!
//! 设计原则：只读取版本信息与 `~/.ssh` 目录，不访问其他路径。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{KhError, KhResult};
use crate::paths;
use crate::perms::{self, DIR_MODE};

/// 单个 SSH 客户端的探测结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshClient {
    /// `system` | `git` | `path`
    pub kind: String,
    /// 展示名：系统原生 OpenSSH / Git 内置 OpenSSH
    pub label: String,
    /// 可执行文件绝对路径
    pub ssh_path: Option<String>,
    /// 对应 ssh-keygen 路径
    pub keygen_path: Option<String>,
    /// 对应 ssh-add 路径
    pub add_path: Option<String>,
    /// 版本串，如 OpenSSH_9.5p2
    pub version: Option<String>,
    /// 是否可用
    pub available: bool,
    /// 探测说明（展示在检测项描述里）
    pub detail: String,
}

/// Git 探测结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub available: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
}

/// `~/.ssh` 目录状态
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshDirInfo {
    pub path: String,
    pub pretty_path: String,
    pub exists: bool,
    /// 本次检测中是否新建了目录
    pub created: bool,
    pub perm_display: Option<String>,
    pub perm_compliant: bool,
    /// 目录内发现的密钥文件数
    pub key_count: usize,
    pub has_config: bool,
}

/// ssh-agent 状态
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub running: bool,
    /// 已加载的密钥指纹列表
    pub identities: Vec<String>,
    /// ssh-add 路径
    pub add_path: Option<String>,
    pub detail: String,
}

/// 全量环境自检结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvReport {
    /// `windows` | `macos` | `linux`
    pub platform: String,
    /// 平台友好名
    pub platform_label: String,
    /// 全部探测到的 SSH 客户端（含不可用的，便于展示检测过程）
    pub ssh_clients: Vec<SshClient>,
    /// 墨钥将优先使用的客户端
    pub preferred: Option<SshClient>,
    pub git: GitInfo,
    pub ssh_dir: SshDirInfo,
    pub agent: AgentInfo,
    /// 是否需要安装引导
    pub needs_install_guide: bool,
}

/* ═══════════════════════════════════════════════════════════════
 * 工具函数
 * ═══════════════════════════════════════════════════════════════ */

/// 执行命令并返回 stdout（首行）
fn run_capture(program: &Path, args: &[&str]) -> Option<String> {
    let out = perms::hidden_command(&program.to_string_lossy())
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // ssh -V 的输出走 stderr，所以两边都取
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    if text.trim().is_empty() {
        text = err;
    }
    Some(text.trim().to_string())
}

/// 探测某个 ssh 可执行文件的版本
fn probe_version(ssh: &Path) -> Option<String> {
    let raw = run_capture(ssh, &["-V"])?;
    // 形如 "OpenSSH_9.5p2, LibreSSL 3.8.2" 或 "OpenSSH_for_Windows_9.5p2, ..."
    let first = raw.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        None
    } else {
        Some(first.to_string())
    }
}

/// 从 ssh 路径推导同目录下的 ssh-keygen / ssh-add
fn sibling(ssh: &Path, name: &str) -> Option<PathBuf> {
    let cand = ssh.parent()?.join(exe_name(name));
    if cand.is_file() {
        Some(cand)
    } else {
        None
    }
}

#[cfg(windows)]
fn exe_name(n: &str) -> String {
    format!("{n}.exe")
}
#[cfg(not(windows))]
fn exe_name(n: &str) -> String {
    n.to_string()
}

/// 在 PATH 中查找可执行文件
fn which(name: &str) -> Option<PathBuf> {
    let probe = exe_name(name);

    // 1) 优先用 where / command -v（尊重用户 PATH 顺序）
    #[cfg(windows)]
    let out = perms::hidden_command("where").arg(&probe).output().ok();
    #[cfg(not(windows))]
    let out = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .ok();

    if let Some(o) = out {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout);
            if let Some(first) = s.lines().find(|l| !l.trim().is_empty()) {
                let p = PathBuf::from(first.trim());
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    // 2) 兜底：手工遍历 PATH
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(&probe);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Windows 系统 OpenSSH 的固定安装位置
#[cfg(windows)]
fn system_openssh_candidates() -> Vec<PathBuf> {
    let mut v = vec![];
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    v.push(PathBuf::from(&windir).join("System32").join("OpenSSH").join("ssh.exe"));
    // 32 位视图
    v.push(
        PathBuf::from(&windir)
            .join("Sysnative")
            .join("OpenSSH")
            .join("ssh.exe"),
    );
    v
}

/// Windows Git 内置 OpenSSH 的常见位置
#[cfg(windows)]
fn git_openssh_candidates() -> Vec<PathBuf> {
    let mut v = vec![];
    for base in [
        "C:\\Program Files\\Git",
        "C:\\Program Files (x86)\\Git",
    ] {
        v.push(PathBuf::from(base).join("usr").join("bin").join("ssh.exe"));
    }
    // 从 git.exe 位置反推
    if let Some(git) = which("git") {
        // git.exe 通常在 <root>\cmd\git.exe 或 <root>\bin\git.exe
        if let Some(root) = git.parent().and_then(|p| p.parent()) {
            v.push(root.join("usr").join("bin").join("ssh.exe"));
        }
    }
    v
}

/// Unix 系统 OpenSSH 常见位置
#[cfg(not(windows))]
fn unix_openssh_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/bin/ssh"),
        PathBuf::from("/usr/local/bin/ssh"),
        PathBuf::from("/bin/ssh"),
        PathBuf::from("/opt/homebrew/bin/ssh"),
    ]
}

/// 由 ssh 路径构造完整的 SshClient 描述
fn make_client(kind: &str, label: &str, ssh: PathBuf) -> SshClient {
    let version = probe_version(&ssh);
    let keygen = sibling(&ssh, "ssh-keygen");
    let add = sibling(&ssh, "ssh-add");
    let dir = ssh
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let available = version.is_some();
    let detail = if available {
        format!(
            "{} · {}",
            version.clone().unwrap_or_default(),
            dir
        )
    } else {
        format!("找到 {dir} 下的 ssh，但无法获取版本信息")
    };

    SshClient {
        kind: kind.to_string(),
        label: label.to_string(),
        ssh_path: Some(ssh.to_string_lossy().to_string()),
        keygen_path: keygen.map(|p| p.to_string_lossy().to_string()),
        add_path: add.map(|p| p.to_string_lossy().to_string()),
        version,
        available,
        detail,
    }
}

/* ═══════════════════════════════════════════════════════════════
 * 各项探测
 * ═══════════════════════════════════════════════════════════════ */

/// 探测系统原生 OpenSSH
fn detect_system_ssh() -> Option<SshClient> {
    #[cfg(windows)]
    {
        for c in system_openssh_candidates() {
            if c.is_file() {
                return Some(make_client("system", "系统原生 OpenSSH", c));
            }
        }
        // 退化：PATH 中的 ssh 若不在 Git 目录，也视作系统 ssh
        if let Some(p) = which("ssh") {
            let s = p.to_string_lossy().to_lowercase();
            if !s.contains("\\git\\") {
                return Some(make_client("system", "系统原生 OpenSSH", p));
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        for c in unix_openssh_candidates() {
            if c.is_file() {
                return Some(make_client("system", "系统原生 OpenSSH", c));
            }
        }
        if let Some(p) = which("ssh") {
            return Some(make_client("system", "系统原生 OpenSSH", p));
        }
        None
    }
}

/// 探测 Git 内置 OpenSSH
fn detect_git_ssh() -> Option<SshClient> {
    #[cfg(windows)]
    {
        for c in git_openssh_candidates() {
            if c.is_file() {
                return Some(make_client("git", "Git 内置 OpenSSH", c));
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        // macOS / Linux 的 Git 与系统共用同一 OpenSSH，直接复用系统结果
        detect_system_ssh().map(|mut c| {
            c.kind = "path".into();
            c
        })
    }
}

/// 探测 Git 与全局用户配置
pub fn detect_git() -> GitInfo {
    let Some(git) = which("git") else {
        return GitInfo {
            available: false,
            version: None,
            path: None,
            user_name: None,
            user_email: None,
        };
    };

    let raw = run_capture(&git, &["--version"]);
    let version = raw.map(|v| {
        // "git version 2.51.0.windows.1" → "2.51.0.windows.1"
        v.trim_start_matches("git version ").trim().to_string()
    });

    let user_name = run_capture(&git, &["config", "--global", "user.name"])
        .filter(|s| !s.trim().is_empty());
    let user_email = run_capture(&git, &["config", "--global", "user.email"])
        .filter(|s| !s.trim().is_empty());

    GitInfo {
        available: true,
        version,
        path: Some(git.to_string_lossy().to_string()),
        user_name,
        user_email,
    }
}

/// 探测 `~/.ssh` 目录；不存在时创建并设 700（PRD 3.1.1）
fn detect_ssh_dir(create_if_missing: bool) -> KhResult<SshDirInfo> {
    let dir = paths::ssh_dir()?;
    let mut created = false;

    if !dir.exists() {
        if create_if_missing {
            paths::ensure_dir(&dir)?;
            // 新建目录立即设标准权限
            let _ = perms::fix(&dir, true);
            created = true;
        }
    }

    let exists = dir.is_dir();
    let (perm_display, perm_compliant) = if exists {
        match perms::stat(&dir, true) {
            Ok(p) => (Some(p.display), p.compliant),
            Err(_) => (None, false),
        }
    } else {
        (None, false)
    };

    // 统计密钥与 config
    let mut key_count = 0usize;
    let mut has_config = false;
    if exists {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name == "config" {
                    has_config = true;
                }
                if name.starts_with('.') || name.ends_with(".pub") {
                    continue;
                }
                // 排除已知的非密钥文件
                if matches!(name.as_str(), "known_hosts" | "authorized_keys" | "config") {
                    continue;
                }
                if crate::keys::looks_like_private_key(&e.path()) {
                    key_count += 1;
                }
            }
        }
    }

    Ok(SshDirInfo {
        path: dir.to_string_lossy().to_string(),
        pretty_path: paths::pretty(&dir),
        exists,
        created,
        perm_display,
        perm_compliant,
        key_count,
        has_config,
    })
}

/// 探测 ssh-agent 状态与已加载密钥
pub fn detect_agent() -> AgentInfo {
    let add = find_ssh_add();

    let Some(add_path) = add else {
        return AgentInfo {
            running: false,
            identities: Vec::new(),
            add_path: None,
            detail: "未找到 ssh-add，无法探测 agent 状态".into(),
        };
    };

    let out = perms::hidden_command(&add_path.to_string_lossy())
        .arg("-l")
        .output();

    match out {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();

            if o.status.success() {
                let identities: Vec<String> = stdout
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string())
                    .collect();
                AgentInfo {
                    running: true,
                    detail: if identities.is_empty() {
                        "ssh-agent 运行中 · 尚未加载任何密钥".into()
                    } else {
                        format!("ssh-agent 运行中 · 已加载 {} 把密钥", identities.len())
                    },
                    identities,
                    add_path: Some(add_path.to_string_lossy().to_string()),
                }
            } else {
                // 常见 stderr：
                //  - "Could not open a connection to your authentication agent."
                //  - "The agent has no identities."
                let lower = stderr.to_lowercase();
                if lower.contains("no identities") {
                    AgentInfo {
                        running: true,
                        identities: Vec::new(),
                        detail: "ssh-agent 运行中 · 尚未加载任何密钥".into(),
                        add_path: Some(add_path.to_string_lossy().to_string()),
                    }
                } else {
                    AgentInfo {
                        running: false,
                        identities: Vec::new(),
                        detail: format!("ssh-agent 未运行：{}", stderr.trim()),
                        add_path: Some(add_path.to_string_lossy().to_string()),
                    }
                }
            }
        }
        Err(e) => AgentInfo {
            running: false,
            identities: Vec::new(),
            add_path: Some(add_path.to_string_lossy().to_string()),
            detail: format!("无法执行 ssh-add：{e}"),
        },
    }
}

/// 定位 ssh-add
pub fn find_ssh_add() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // 优先系统 OpenSSH 自带的
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let p = PathBuf::from(&windir)
            .join("System32")
            .join("OpenSSH")
            .join("ssh-add.exe");
        if p.is_file() {
            return Some(p);
        }
        for base in ["C:\\Program Files\\Git", "C:\\Program Files (x86)\\Git"] {
            let g = PathBuf::from(base).join("usr").join("bin").join("ssh-add.exe");
            if g.is_file() {
                return Some(g);
            }
        }
    }
    which("ssh-add")
}

/// 定位 ssh-keygen（供密钥模块使用）
pub fn find_ssh_keygen() -> Option<PathBuf> {
    if let Some(c) = detect_system_ssh() {
        if let Some(k) = c.keygen_path {
            return Some(PathBuf::from(k));
        }
    }
    if let Some(c) = detect_git_ssh() {
        if let Some(k) = c.keygen_path {
            return Some(PathBuf::from(k));
        }
    }
    which("ssh-keygen")
}

/// 定位 ssh（供连通测试使用）
pub fn find_ssh() -> Option<PathBuf> {
    if let Some(c) = detect_system_ssh() {
        if let Some(p) = c.ssh_path {
            return Some(PathBuf::from(p));
        }
    }
    if let Some(c) = detect_git_ssh() {
        if let Some(p) = c.ssh_path {
            return Some(PathBuf::from(p));
        }
    }
    which("ssh")
}

/// 定位 git（供 git 配置模块使用）
pub fn find_git() -> Option<PathBuf> {
    if let Some(p) = detect_git().path {
        return Some(PathBuf::from(p));
    }
    which("git")
}

/* ═══════════════════════════════════════════════════════════════
 * 对外入口
 * ═══════════════════════════════════════════════════════════════ */

/// 全量环境自检
pub fn detect_environment(create_ssh_dir: bool) -> KhResult<EnvReport> {
    let mut clients: Vec<SshClient> = Vec::new();

    if let Some(c) = detect_system_ssh() {
        clients.push(c);
    }
    if let Some(c) = detect_git_ssh() {
        // 避免系统与 Git 指向同一文件时重复列项
        let dup = clients
            .iter()
            .any(|x| x.ssh_path == c.ssh_path);
        if !dup {
            clients.push(c);
        }
    }

    let preferred = clients.iter().find(|c| c.available).cloned();
    let git = detect_git();
    let ssh_dir = detect_ssh_dir(create_ssh_dir)?;
    let agent = detect_agent();

    let platform = current_platform();
    let platform_label = platform_label(&platform).to_string();

    Ok(EnvReport {
        platform,
        platform_label,
        needs_install_guide: preferred.is_none(),
        preferred,
        git,
        ssh_dir,
        agent,
        ssh_clients: clients,
    })
}

pub fn current_platform() -> String {
    if cfg!(target_os = "windows") {
        "windows".into()
    } else if cfg!(target_os = "macos") {
        "macos".into()
    } else {
        "linux".into()
    }
}

pub fn platform_label(p: &str) -> &'static str {
    match p {
        "windows" => "Windows",
        "macos" => "macOS",
        _ => "Linux",
    }
}

/// 本机主机名（用于生成示例 config、诊断展示）
pub fn hostname() -> Option<String> {
    for key in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(v) = std::env::var(key) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    // Unix 兜底
    #[cfg(not(windows))]
    {
        if let Ok(o) = std::process::Command::new("hostname").output() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// 环境自检结果的精简摘要（侧栏状态卡用）
pub fn quick_summary() -> KhResult<EnvSummary> {
    let report = detect_environment(false)?;
    let agent = crate::agent::status();
    Ok(EnvSummary {
        ssh_label: report
            .preferred
            .as_ref()
            .map(|c| c.label.clone())
            .unwrap_or_else(|| "未检测到 ssh".into()),
        ssh_available: report.preferred.is_some(),
        ssh_version: report.preferred.as_ref().and_then(|c| c.version.clone()),
        git_version: report.git.version.clone(),
        perm_label: report
            .ssh_dir
            .perm_display
            .clone()
            .unwrap_or_else(|| "—".into()),
        perm_compliant: report.ssh_dir.perm_compliant,
        agent_label: agent.label.clone(),
        agent_running: agent.running,
        key_count: report.ssh_dir.key_count,
        has_config: report.ssh_dir.has_config,
        platform: report.platform.clone(),
    })
}

/// 侧栏 / 概览用的环境摘要
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSummary {
    pub ssh_label: String,
    pub ssh_available: bool,
    pub ssh_version: Option<String>,
    pub git_version: Option<String>,
    pub perm_label: String,
    pub perm_compliant: bool,
    pub agent_label: String,
    pub agent_running: bool,
    pub key_count: usize,
    pub has_config: bool,
    pub platform: String,
}

/// 分平台安装引导（PRD 3.1.1）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallGuide {
    pub platform: String,
    pub title: String,
    pub steps: Vec<GuideStep>,
    /// Windows 才支持一键启用
    pub one_click_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideStep {
    pub label: String,
    /// 需要复制的命令（Unix）
    pub command: Option<String>,
    pub note: String,
}

pub fn install_guide() -> InstallGuide {
    let platform = current_platform();
    match platform.as_str() {
        "windows" => InstallGuide {
            platform,
            title: "为 Windows 启用 OpenSSH 客户端".into(),
            one_click_supported: true,
            steps: vec![
                GuideStep {
                    label: "方式一：一键启用（推荐）".into(),
                    command: None,
                    note: "点击下方「一键启用」按钮，墨钥将调用系统可选功能安装 OpenSSH.Client，无需重启。".into(),
                },
                GuideStep {
                    label: "方式二：手动启用".into(),
                    command: Some(
                        "Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0".into(),
                    ),
                    note: "以管理员身份运行 PowerShell 并执行该命令。".into(),
                },
                GuideStep {
                    label: "方式三：安装 Git for Windows".into(),
                    command: None,
                    note: "Git for Windows 自带 OpenSSH，安装后墨钥会自动识别为备选环境。".into(),
                },
            ],
        },
        "macos" => InstallGuide {
            platform,
            title: "在 macOS 上准备 OpenSSH".into(),
            one_click_supported: false,
            steps: vec![
                GuideStep {
                    label: "macOS 已内置 OpenSSH".into(),
                    command: None,
                    note: "macOS 12+ 默认自带 /usr/bin/ssh。若检测失败，请确认 PATH 未被覆盖。".into(),
                },
                GuideStep {
                    label: "通过 Homebrew 安装较新版本（可选）".into(),
                    command: Some("brew install openssh".into()),
                    note: "若需要更新的 OpenSSH 特性（如 FIDO 安全密钥支持），可执行此命令。".into(),
                },
            ],
        },
        _ => InstallGuide {
            platform,
            title: "在 Linux 上安装 OpenSSH 客户端".into(),
            one_click_supported: false,
            steps: vec![
                GuideStep {
                    label: "Debian / Ubuntu".into(),
                    command: Some("sudo apt update && sudo apt install -y openssh-client".into()),
                    note: "适用于 Ubuntu、Debian、Linux Mint 等。".into(),
                },
                GuideStep {
                    label: "Fedora / RHEL / CentOS".into(),
                    command: Some("sudo dnf install -y openssh-clients".into()),
                    note: "适用于 Fedora、RHEL 8+、Rocky、AlmaLinux。".into(),
                },
                GuideStep {
                    label: "Arch / Manjaro".into(),
                    command: Some("sudo pacman -S --noconfirm openssh".into()),
                    note: "Arch 系通常已预装。".into(),
                },
                GuideStep {
                    label: "openSUSE".into(),
                    command: Some("sudo zypper install -y openssh-clients".into()),
                    note: "适用于 openSUSE Leap / Tumbleweed。".into(),
                },
            ],
        },
    }
}

/// Windows 一键启用 OpenSSH 客户端（PRD 3.1.1）
#[cfg(windows)]
pub fn enable_windows_openssh() -> KhResult<String> {
    // 先看是否已启用
    let check = perms::hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0).State",
        ])
        .output();

    if let Ok(o) = check {
        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if s.eq_ignore_ascii_case("Installed") {
            return Ok("系统原生 OpenSSH 已处于启用状态，无需重复安装".into());
        }
    }

    let out = perms::hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0 | Select-Object -ExpandProperty RestartNeeded",
        ])
        .output()
        .map_err(|e| KhError::CommandFailed(format!("无法调用 PowerShell：{e}")))?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    if out.status.success() {
        Ok(if stdout.is_empty() {
            "OpenSSH 客户端已启用".into()
        } else {
            format!("OpenSSH 客户端已启用（RestartNeeded={stdout}）")
        })
    } else {
        // 最常见原因是没有管理员权限
        Err(KhError::Permission(format!(
            "启用失败，请以管理员身份运行墨钥后重试。系统返回：{}",
            if stderr.is_empty() { &stdout } else { &stderr }
        )))
    }
}

#[cfg(not(windows))]
pub fn enable_windows_openssh() -> KhResult<String> {
    Err(KhError::Unsupported(
        "一键启用仅支持 Windows 平台".into(),
    ))
}

/// 确保 `~/.ssh` 存在且权限达标（供电商其它模块复用）
pub fn ensure_ssh_dir() -> KhResult<PathBuf> {
    let dir = paths::ssh_dir()?;
    paths::ensure_dir(&dir)?;
    let _ = perms::fix(&dir, true);
    debug_assert!(DIR_MODE == 0o700);
    Ok(dir)
}
