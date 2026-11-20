//! ssh-agent 管理（PRD 3.1.4 后半 / 安全中心）
//!
//! 职责：
//! - 探测 agent 是否运行、已加载哪些密钥
//! - 启动 agent（Windows: ssh-agent 服务；Unix: 常驻 ssh-agent 进程）
//! - 把密钥加入 / 移除 agent
//! - 配置 agent 开机自启
//!
//! 安全说明：本模块不接触私钥明文，只把文件路径交给系统 ssh-add，
//! 由 OpenSSH 自己解密并托管到内存。

use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::env;
use crate::error::{KhError, KhResult};
use crate::paths;

/// agent 中一把密钥的摘要（ssh-add -l 的输出行）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentKey {
    /// 位长
    pub bits: u32,
    /// 指纹
    pub fingerprint: String,
    /// 注释（通常是路径或邮箱）
    pub comment: String,
    /// 算法（SSH-2.0 之前的显示名，如 RSA / ED25519）
    pub algorithm: String,
}

/// agent 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    /// 是否运行
    pub running: bool,
    /// 已加载密钥数
    pub count: usize,
    /// 密钥列表
    pub keys: Vec<AgentKey>,
    /// 状态描述（中文，供 UI 直接展示）
    pub label: String,
    /// 语义类：`ok` | `idle` | `error`
    pub tone: String,
    /// ssh-add 可执行文件路径
    pub ssh_add: Option<String>,
    /// 当前 SSH_AUTH_SOCK / 或 Windows 管道名
    pub sock: Option<String>,
    /// 是否已配置自启
    pub autostart: bool,
    /// 平台
    pub platform: String,
}

// ─────────────────────── 探测 ───────────────────────

/// 读取 agent 状态
pub fn status() -> AgentStatus {
    let ssh_add = env::find_ssh_add();
    let platform = std::env::consts::OS.to_string();
    let sock = detect_sock();

    let Some(ssh_add) = ssh_add else {
        return AgentStatus {
            running: false,
            count: 0,
            keys: Vec::new(),
            label: "未找到 ssh-add，无法管理 agent".into(),
            tone: "error".into(),
            ssh_add: None,
            sock,
            autostart: is_autostart_enabled(),
            platform,
        };
    };

    match list_keys() {
        Ok(keys) => {
            let running = true;
            let count = keys.len();
            let label = if count == 0 {
                "agent 已运行，尚未加载密钥".to_string()
            } else {
                format!("agent 已运行，已加载 {} 把密钥", count)
            };
            AgentStatus {
                running,
                count,
                keys,
                label,
                tone: if count > 0 { "ok".into() } else { "idle".into() },
                ssh_add: Some(ssh_add.to_string_lossy().to_string()),
                sock,
                autostart: is_autostart_enabled(),
                platform,
            }
        }
        Err(e) => {
            // 区分「agent 没运行」与「其它错误」
            let msg = e.to_string();
            let not_running = msg.contains("Could not open a connection")
                || msg.contains("connection to your authentication agent")
                || msg.contains("error connecting to agent")
                || msg.to_lowercase().contains("no such file");
            AgentStatus {
                running: !not_running,
                count: 0,
                keys: Vec::new(),
                label: if not_running {
                    "agent 未运行".into()
                } else {
                    format!("agent 状态异常：{}", msg)
                },
                tone: if not_running { "idle".into() } else { "error".into() },
                ssh_add: Some(ssh_add.to_string_lossy().to_string()),
                sock,
                autostart: is_autostart_enabled(),
                platform,
            }
        }
    }
}

/// 解析 `ssh-add -l` 输出
pub fn list_keys() -> KhResult<Vec<AgentKey>> {
    let ssh_add = env::find_ssh_add()
        .ok_or_else(|| KhError::SshMissing("未找到 ssh-add".into()))?;

    let out = run_capture(&ssh_add, &["-l"])?;
    let (code, stdout, stderr) = out;

    if code == 1 {
        // 1 = agent 运行但没有密钥
        return Ok(Vec::new());
    }
    if code == 2 {
        // 2 = 无法连接 agent
        return Err(KhError::CommandFailed(if stderr.trim().is_empty() {
            "无法连接 ssh-agent".into()
        } else {
            stderr.trim().to_string()
        }));
    }
    if code != 0 {
        return Err(KhError::CommandFailed(if stderr.trim().is_empty() {
            format!("ssh-add -l 退出码 {}", code)
        } else {
            stderr.trim().to_string()
        }));
    }

    // 行格式：`4096 SHA256:xxx comment... (RSA)`
    let mut keys = Vec::new();
    for line in stdout.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let mut it = t.splitn(3, ' ');
        let bits = it.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
        let fp = it.next().unwrap_or("").to_string();
        let rest = it.next().unwrap_or("").to_string();
        // 尾部 `(RSA)` 是算法
        let (comment, algorithm) = match (rest.rfind('('), rest.ends_with(')')) {
            (Some(i), true) => (
                rest[..i].trim().to_string(),
                rest[i + 1..rest.len() - 1].to_string(),
            ),
            _ => (rest.trim().to_string(), String::new()),
        };
        keys.push(AgentKey {
            bits,
            fingerprint: fp,
            comment,
            algorithm,
        });
    }
    Ok(keys)
}

/// 探测 SSH_AUTH_SOCK / Windows 命名管道
fn detect_sock() -> Option<String> {
    if let Ok(v) = std::env::var("SSH_AUTH_SOCK") {
        if !v.trim().is_empty() {
            return Some(v);
        }
    }
    None
}

// ─────────────────────── 启停 ───────────────────────

/// 启动 agent
pub fn start_agent() -> KhResult<AgentStatus> {
    // 已经是运行状态，直接返回（幂等）
    let cur = status();
    if cur.running {
        return Ok(cur);
    }

    #[cfg(windows)]
    {
        start_agent_windows()?;
    }

    #[cfg(not(windows))]
    {
        // Unix：无法从子进程长期持有 agent 会话（进程组隔离），
        // 因此这里只做「检测 + 引导」：尝试 eval ssh-agent 的常见做法不可行，
        // 改为启动一个 detached 的 ssh-agent 并写回环境文件供后续调用读取。
        start_agent_unix()?;
    }

    // 给系统一点时间让服务就绪
    std::thread::sleep(std::time::Duration::from_millis(500));

    let st = status();
    crate::audit::log(
        crate::audit::CAT_AGENT,
        "agent.start",
        if st.running { "启动 ssh-agent" } else { "尝试启动 ssh-agent" },
        if st.running { "ok" } else { "fail" },
        "",
        &st.label,
    );
    Ok(st)
}

#[cfg(windows)]
fn start_agent_windows() -> KhResult<()> {
    // Windows 上的 ssh-agent 是一个服务，标准做法是用管理员权限设置为自动启动并启动
    let script = "Set-Service -Name ssh-agent -StartupType Automatic -ErrorAction SilentlyContinue; \
                  Start-Service -Name ssh-agent -ErrorAction SilentlyContinue; \
                  (Get-Service -Name ssh-agent).Status";
    let out = run_capture(
        std::path::Path::new("powershell.exe"),
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )?;
    let (_code, stdout, stderr) = out;
    let s = stdout.trim();
    if s.eq_ignore_ascii_case("Running") {
        return Ok(());
    }
    Err(KhError::Permission(if stderr.trim().is_empty() {
        "启动 ssh-agent 服务失败：可能需要以管理员身份运行墨钥（Windows 服务操作需要提权）".into()
    } else {
        format!("启动 ssh-agent 失败：{}", stderr.trim())
    }))
}

#[cfg(not(windows))]
fn start_agent_unix() -> KhResult<()> {
    // 检查是否已有 agent 可用
    if let Ok(cur) = list_keys() {
        let _ = cur;
        return Ok(());
    }
    // 启动一个 detached ssh-agent，并把环境写回文件，后续调用读取
    let out = run_capture(std::path::Path::new("ssh-agent"), &["-s"])?;
    let (code, stdout, stderr) = out;
    if code != 0 {
        return Err(KhError::CommandFailed(format!(
            "启动 ssh-agent 失败：{}",
            if stderr.trim().is_empty() { "未知错误" } else { stderr.trim() }
        )));
    }
    // 解析 SSH_AUTH_SOCK=...; export SSH_AUTH_SOCK;
    let sock = stdout
        .lines()
        .find_map(|l| l.strip_prefix("SSH_AUTH_SOCK="))
        .map(|v| v.trim().trim_end_matches(';').to_string())
        .unwrap_or_default();
    if !sock.is_empty() {
        let p = paths::app_data_dir()?.join("agent.env");
        let _ = std::fs::write(&p, format!("SSH_AUTH_SOCK={}\n", sock));
        // 同进程内设置，让本次会话后续命令也能用
        std::env::set_var("SSH_AUTH_SOCK", &sock);
    }
    Ok(())
}

/// 停止 agent（Windows 停服务；Unix 杀进程组）
pub fn stop_agent() -> KhResult<AgentStatus> {
    #[cfg(windows)]
    {
        let out = run_capture(
            std::path::Path::new("powershell.exe"),
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Stop-Service -Name ssh-agent -Force -ErrorAction SilentlyContinue; \
                 (Get-Service -Name ssh-agent).Status",
            ],
        )?;
        let (_c, stdout, stderr) = out;
        if stdout.trim().eq_ignore_ascii_case("Stopped") {
            crate::audit::log(
                crate::audit::CAT_AGENT,
                "agent.stop",
                "停止 ssh-agent 服务",
                "ok",
                "",
                "",
            );
            return Ok(status());
        }
        return Err(KhError::Permission(if stderr.trim().is_empty() {
            "停止 ssh-agent 需要管理员权限".into()
        } else {
            format!("停止失败：{}", stderr.trim())
        }));
    }
    #[cfg(not(windows))]
    {
        let _ = run_capture(std::path::Path::new("ssh-agent"), &["-k"])?;
        crate::audit::log(
            crate::audit::CAT_AGENT,
            "agent.stop",
            "停止 ssh-agent",
            "ok",
            "",
            "",
        );
        Ok(status())
    }
}

// ─────────────────────── 增删密钥 ───────────────────────

/// 把密钥加入 agent。
///
/// `lifetime` 为 0 表示不过期。
/// 注意：若密钥带口令，ssh-add 会需要口令输入。
/// 出于「不把口令落到命令行/日志」的安全考虑，
/// 这里仅在无口令时自动加入，带口令时返回明确错误引导用户处理。
pub fn add_key(filename: &str, lifetime: u32) -> KhResult<AgentStatus> {
    let (path, meta) = resolve_key(filename)?;

    if meta.has_passphrase {
        return Err(KhError::InvalidInput(format!(
            "密钥 {} 带口令保护。为避免口令出现在命令参数或日志中，墨钥不会代为输入口令；\
             请在系统终端手动执行 ssh-add 完成加载。",
            filename
        )));
    }

    let ssh_add = env::find_ssh_add()
        .ok_or_else(|| KhError::SshMissing("未找到 ssh-add".into()))?;

    let mut args: Vec<String> = Vec::new();
    if lifetime > 0 {
        args.push("-t".into());
        args.push(lifetime.to_string());
    }
    args.push(path.to_string_lossy().to_string());

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (code, _stdout, stderr) = run_capture(&ssh_add, &arg_refs)?;

    if code != 0 {
        let msg = stderr.trim().to_string();
        crate::audit::log(
            crate::audit::CAT_AGENT,
            "agent.add",
            &format!("加入 agent 失败：{}", filename),
            "fail",
            filename,
            &msg,
        );
        // 常见错误翻译
        let friendly = if msg.contains("Could not open a connection") {
            "ssh-agent 未运行，请先启动 agent".to_string()
        } else if msg.contains("Bad passphrase") {
            "口令错误或密钥需要口令，请手动用 ssh-add 加载".to_string()
        } else if msg.contains("Permission denied") {
            "私钥权限过宽，请先修复权限".to_string()
        } else if msg.is_empty() {
            format!("ssh-add 退出码 {}", code)
        } else {
            msg
        };
        return Err(KhError::CommandFailed(friendly));
    }

    crate::audit::log(
        crate::audit::CAT_AGENT,
        "agent.add",
        &format!("把 {} 加入 ssh-agent", filename),
        "ok",
        filename,
        "",
    );
    Ok(status())
}

/// 从 agent 移除密钥（按文件路径匹配）
pub fn remove_key(filename: &str) -> KhResult<AgentStatus> {
    let (path, _) = resolve_key(filename)?;
    let ssh_add = env::find_ssh_add()
        .ok_or_else(|| KhError::SshMissing("未找到 ssh-add".into()))?;

    let (code, _o, stderr) = run_capture(&ssh_add, &["-d", &path.to_string_lossy()])?;
    if code != 0 {
        let msg = stderr.trim();
        return Err(KhError::CommandFailed(if msg.is_empty() {
            "移除失败：agent 中可能没有该密钥".into()
        } else {
            msg.to_string()
        }));
    }
    crate::audit::log(
        crate::audit::CAT_AGENT,
        "agent.remove",
        &format!("从 ssh-agent 移除 {}", filename),
        "ok",
        filename,
        "",
    );
    Ok(status())
}

/// 清空 agent 中全部密钥
pub fn remove_all() -> KhResult<AgentStatus> {
    let ssh_add = env::find_ssh_add()
        .ok_or_else(|| KhError::SshMissing("未找到 ssh-add".into()))?;
    let (code, _o, stderr) = run_capture(&ssh_add, &["-D"])?;
    if code != 0 {
        return Err(KhError::CommandFailed(if stderr.trim().is_empty() {
            "清空失败".into()
        } else {
            stderr.trim().to_string()
        }));
    }
    crate::audit::log(
        crate::audit::CAT_AGENT,
        "agent.clear",
        "清空 ssh-agent 中的全部密钥",
        "warn",
        "",
        "",
    );
    Ok(status())
}

fn resolve_key(filename: &str) -> KhResult<(std::path::PathBuf, crate::keys::KeyMeta)> {
    paths::validate_filename(filename)?;
    let dir = paths::ssh_dir()?;
    let path = dir.join(filename);
    if !path.is_file() {
        return Err(KhError::NotFound(format!("密钥不存在：{}", filename)));
    }
    let meta = crate::keys::describe_key(&path)?;
    Ok((path, meta))
}

// ─────────────────────── 开机自启 ───────────────────────

/// 是否已启用 agent 自启
pub fn is_autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        let out = run_capture(
            std::path::Path::new("powershell.exe"),
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-Service -Name ssh-agent -ErrorAction SilentlyContinue).StartType",
            ],
        );
        match out {
            Ok((_, stdout, _)) => stdout.trim().eq_ignore_ascii_case("Automatic"),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        // Unix 默认由桌面会话或 shell profile 管理，这里读我们的偏好文件
        crate::prefs::read_prefs()
            .map(|p| p.agent_autostart)
            .unwrap_or(false)
    }
}

/// 设置 agent 开机自启（PRD 3.1.4）
pub fn set_autostart(enabled: bool) -> KhResult<AgentStatus> {
    #[cfg(windows)]
    {
        let st = if enabled { "Automatic" } else { "Manual" };
        let script = format!(
            "Set-Service -Name ssh-agent -StartupType {} -ErrorAction Stop; \
             (Get-Service -Name ssh-agent).StartType",
            st
        );
        let out = run_capture(
            std::path::Path::new("powershell.exe"),
            &["-NoProfile", "-NonInteractive", "-Command", &script],
        )?;
        let (code, stdout, stderr) = out;
        if code != 0 {
            return Err(KhError::Permission(if stderr.trim().is_empty() {
                "修改 ssh-agent 启动类型需要管理员权限".into()
            } else {
                format!("设置失败：{}", stderr.trim())
            }));
        }
        let _ = stdout;
        // 若开启自启，顺手确保当前也在运行
        if enabled {
            let _ = start_agent();
        }
    }
    #[cfg(not(windows))]
    {
        let mut p = crate::prefs::read_prefs()?;
        p.agent_autostart = enabled;
        crate::prefs::write_prefs(&p)?;
    }

    // 同步偏好记录
    if let Ok(mut p) = crate::prefs::read_prefs() {
        p.agent_autostart = enabled;
        let _ = crate::prefs::write_prefs(&p);
    }

    crate::audit::log(
        crate::audit::CAT_AGENT,
        "agent.autostart",
        if enabled {
            "启用 ssh-agent 开机自启"
        } else {
            "关闭 ssh-agent 开机自启"
        },
        "ok",
        "",
        "",
    );
    Ok(status())
}

// ─────────────────────── 工具 ───────────────────────

/// 执行命令并捕获输出，返回 (退出码, stdout, stderr)
fn run_capture(exe: &std::path::Path, args: &[&str]) -> KhResult<(i32, String, String)> {
    let mut cmd = crate::perms::hidden_command(&exe.to_string_lossy());
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd
        .output()
        .map_err(|e| KhError::CommandFailed(format!("无法执行 {}：{}", exe.display(), e)))?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    ))
}

/// 供 UI 展示的 agent 相关命令（透明化，让用户知道程序会调用什么）
pub fn preview_cmd(action: &str) -> String {
    match action {
        "list" => "ssh-add -l".into(),
        "start" => {
            if cfg!(windows) {
                "powershell Start-Service ssh-agent".into()
            } else {
                "ssh-agent -s".into()
            }
        }
        "stop" => {
            if cfg!(windows) {
                "powershell Stop-Service ssh-agent".into()
            } else {
                "ssh-agent -k".into()
            }
        }
        "add" => "ssh-add <keyfile>".into(),
        "remove" => "ssh-add -d <keyfile>".into(),
        "clear" => "ssh-add -D".into(),
        other => other.to_string(),
    }
}
