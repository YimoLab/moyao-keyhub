//! 连通性测试与智能诊断（PRD 3.1.6）
//!
//! 文件名用 `conn_test.rs` 而非 `test.rs`：`test` 在 Rust 里是保留的
//! 属性命名空间（`#[cfg(test)]` / `#[test]`），模块名与其同名会造成
//! 阅读混淆，也容易被 `cargo test` 的语义误导。
//!
//! 强约束（PRD 7.1 / 7.3）：
//! - 只在用户显式点击「测试」后发起网络连接，程序不自发联网
//! - 测试通过系统原生 `ssh` 执行，不使用第三方 SSH 实现
//! - 命令构造使用 BatchMode 类参数避免交互卡死，
//!   若因口令导致认证失败，诊断会明确给出「加入 agent」的处置建议
//! - 所有输出脱敏后返回：绝不含私钥内容

use std::io::Read;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::env;
use crate::error::{KhError, KhResult};
use crate::paths;
use crate::perms;

/// 测试入参
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestOpts {
    /// 走 config 别名（推荐，能完整复用 config 语义）
    #[serde(default)]
    pub alias: String,
    /// 直连时的主机名 / IP
    #[serde(default)]
    pub hostname: String,
    /// 端口
    #[serde(default)]
    pub port: u32,
    /// 用户
    #[serde(default)]
    pub user: String,
    /// IdentityFile（可选）
    #[serde(default)]
    pub identity: String,
    /// 超时秒数
    #[serde(default)]
    pub timeout_secs: u32,
    /// 是否详细输出（-v）
    #[serde(default)]
    pub verbose: bool,
}

/// 单条诊断结论
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    /// `error` | `warn` | `info` | `ok`
    pub level: String,
    /// 标题，如「密钥未加载到 agent」
    pub title: String,
    /// 现象说明
    pub detail: String,
    /// 处置建议
    pub fix: String,
    /// 建议动作标识，前端据此渲染可点击项
    #[serde(default)]
    pub action: String,
}

/// 测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    /// 目标展示名
    pub target: String,
    /// 实际执行的命令（脱敏后，供用户核对）
    pub command: String,
    /// 是否成功
    pub success: bool,
    /// 退出码
    pub exit_code: i32,
    /// 耗时毫秒
    pub elapsed_ms: u64,
    /// 远程主机标识（成功时）
    pub remote_banner: Option<String>,
    /// 远程命令输出
    pub stdout: String,
    /// 错误输出（已脱敏）
    pub stderr: String,
    /// 智能诊断结论列表
    pub diagnoses: Vec<Diagnosis>,
    /// 一句话结论
    pub summary: String,
}

/// 连通测试历史记录
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TestHistoryItem {
    pub at: String,
    pub target: String,
    pub success: bool,
    pub elapsed_ms: u64,
    pub summary: String,
}

fn history_path() -> KhResult<std::path::PathBuf> {
    Ok(paths::app_data_dir()?.join("test-history.json"))
}

/// 读取连通测试历史（倒序，最新在前）
pub fn read_history(limit: u32) -> KhResult<Vec<TestHistoryItem>> {
    let p = history_path()?;
    if !p.is_file() {
        return Ok(Vec::new());
    }
    let txt = std::fs::read_to_string(&p)?;
    let mut v: Vec<TestHistoryItem> = serde_json::from_str(&txt).unwrap_or_default();
    v.reverse();
    if limit > 0 {
        v.truncate(limit as usize);
    }
    Ok(v)
}

fn push_history(item: TestHistoryItem) {
    let Ok(p) = history_path() else { return };
    let mut v: Vec<TestHistoryItem> = std::fs::read_to_string(&p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    v.push(item);
    // 只保留最近 200 条，避免无限增长
    if v.len() > 200 {
        let drop = v.len() - 200;
        v.drain(0..drop);
    }
    if let Ok(s) = serde_json::to_string(&v) {
        let _ = std::fs::write(&p, s.as_bytes());
    }
}

/// 清空历史
pub fn clear_history() -> KhResult<()> {
    let p = history_path()?;
    if p.is_file() {
        std::fs::write(&p, b"")?;
    }
    Ok(())
}

/// 执行连通测试
pub fn test_connection(opts: TestOpts) -> KhResult<TestResult> {
    let ssh = env::find_ssh()
        .ok_or_else(|| KhError::SshMissing("未找到 ssh 客户端，请先在环境自检中安装".into()))?;

    let timeout = if opts.timeout_secs == 0 {
        10
    } else {
        opts.timeout_secs
    };
    let target = build_target(&opts);

    // 构造命令：所有命令都带这些安全参数
    let mut args: Vec<String> = Vec::new();
    args.push("-o".into());
    args.push(format!("ConnectTimeout={}", timeout));
    args.push("-o".into());
    args.push("StrictHostKeyChecking=accept-new".into());
    args.push("-o".into());
    args.push("NumberOfPasswordPrompts=0".into());
    // BatchMode 保证不会卡在交互式提问上；若密钥带口令，
    // 诊断会明确告诉用户「需加入 agent」而非静默失败
    args.push("-o".into());
    args.push("BatchMode=yes".into());
    args.push("-o".into());
    args.push("LogLevel=ERROR".into());

    if opts.verbose {
        args.push("-v".into());
    }

    if !opts.alias.trim().is_empty() {
        // 走别名，完整复用 config
        args.push(opts.alias.trim().to_string());
    } else {
        if opts.port > 0 {
            args.push("-p".into());
            args.push(opts.port.to_string());
        }
        if !opts.identity.trim().is_empty() {
            args.push("-i".into());
            args.push(expand_identity(&opts.identity));
            args.push("-o".into());
            args.push("IdentitiesOnly=yes".into());
        }
        let host = opts.hostname.trim();
        if host.is_empty() {
            return Err(KhError::InvalidInput(
                "请填写主机名，或选择一个 config 别名".into(),
            ));
        }
        let user = opts.user.trim();
        let dest = if user.is_empty() {
            host.to_string()
        } else {
            format!("{}@{}", user, host)
        };
        args.push(dest);
    }

    // 远端执行只读探测命令确认链路可用
    args.push("echo KEYHUB_OK && uname -a".into());

    let display_cmd = format!("ssh {}", shell_join(&args));

    let start = Instant::now();
    let mut cmd = perms::hidden_command(&ssh.to_string_lossy());
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child: Child = cmd
        .spawn()
        .map_err(|e| KhError::CommandFailed(format!("无法启动 ssh：{}", e)))?;

    // 等待进程，超时则强杀
    let deadline = start + Duration::from_secs(timeout as u64 + 8);
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(60));
            }
            Err(e) => {
                return Err(KhError::CommandFailed(format!("ssh 执行异常：{}", e)));
            }
        }
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }
    let exit_code = child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);

    let success = !timed_out && exit_code == 0 && stdout.contains("KEYHUB_OK");

    // 脱敏：把 home 路径换成 ~
    let stdout = sanitize(&stdout);
    let stderr = sanitize(&stderr);
    let remote_banner = stdout
        .lines()
        .map(|s| s.trim())
        .find(|s| !s.is_empty() && !s.contains("KEYHUB_OK"))
        .map(|s| s.to_string());

    let diagnoses = diagnose(&opts, timed_out, exit_code, &stderr, success);
    let summary = one_line_summary(success, &diagnoses, timed_out);

    let result = TestResult {
        target: target.clone(),
        command: display_cmd,
        success,
        exit_code: if timed_out { 124 } else { exit_code },
        elapsed_ms,
        remote_banner,
        stdout,
        stderr,
        diagnoses,
        summary: summary.clone(),
    };

    push_history(TestHistoryItem {
        at: crate::time_util::now_rfc3339(),
        target,
        success,
        elapsed_ms,
        summary: summary.clone(),
    });

    crate::audit::log(
        crate::audit::CAT_CONNECT,
        "conn.test",
        &summary,
        if success { "ok" } else { "fail" },
        &result.target,
        &format!("耗时 {}ms，退出码 {}", elapsed_ms, result.exit_code),
    );

    Ok(result)
}

fn build_target(o: &TestOpts) -> String {
    if !o.alias.trim().is_empty() {
        return o.alias.trim().to_string();
    }
    let host = o.hostname.trim();
    let user = o.user.trim();
    let port = if o.port > 0 && o.port != 22 {
        format!(":{}", o.port)
    } else {
        String::new()
    };
    if user.is_empty() {
        format!("{}{}", host, port)
    } else {
        format!("{}@{}{}", user, host, port)
    }
}

fn expand_identity(id: &str) -> String {
    paths::expand(id)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| id.to_string())
}

/// 把敏感/冗长内容折叠：home 路径 → ~
fn sanitize(s: &str) -> String {
    if let Ok(home) = paths::home_dir() {
        let h = home.to_string_lossy().to_string();
        let mut out = String::with_capacity(s.len());
        for line in s.lines() {
            out.push_str(&line.replace(&h, "~"));
            out.push('\n');
        }
        if !out.trim().is_empty() {
            return out.trim_end().to_string();
        }
    }
    s.trim_end().to_string()
}

/// 智能诊断：把 ssh 报错翻译成处置建议
fn diagnose(
    opts: &TestOpts,
    timed_out: bool,
    exit_code: i32,
    stderr: &str,
    success: bool,
) -> Vec<Diagnosis> {
    let mut d: Vec<Diagnosis> = Vec::new();
    let low = stderr.to_lowercase();

    if success {
        d.push(Diagnosis {
            level: "ok".into(),
            title: "链路正常".into(),
            detail: "已完成 SSH 握手并在远端执行了只读探测命令".into(),
            fix: String::new(),
            action: String::new(),
        });
        // 成功也顺手检查权限，避免「能连但权限不合规」的隐患
        if let Ok(dir) = paths::ssh_dir() {
            if let Ok(st) = perms::stat(&dir, true) {
                if !st.compliant {
                    d.push(Diagnosis {
                        level: "warn".into(),
                        title: "~/.ssh 目录权限过宽".into(),
                        detail: format!("当前为 {}，建议收紧到 700", st.display),
                        fix: "在安全中心点击「一键修复权限」".into(),
                        action: "fix-perms".into(),
                    });
                }
            }
        }
        return d;
    }

    let timeout = if opts.timeout_secs == 0 {
        10
    } else {
        opts.timeout_secs
    };

    // 1) 超时
    if timed_out || low.contains("connection timed out") || low.contains("operation timed out") {
        d.push(Diagnosis {
            level: "error".into(),
            title: "连接超时".into(),
            detail: format!("在 {} 秒内未建立 TCP 连接，目标可能被防火墙拦截或地址不可达", timeout),
            fix: "确认服务器地址与端口正确；检查云厂商安全组是否放行该端口；本地网络是否需代理"
                .into(),
            action: "retry-longer".into(),
        });
    }

    // 2) 端口 / 主机不可达
    if low.contains("connection refused") {
        d.push(Diagnosis {
            level: "error".into(),
            title: "连接被拒绝".into(),
            detail: "目标主机可达，但对应端口没有服务在监听".into(),
            fix: "确认 sshd 正在运行，且端口与 config 中一致（默认 22）".into(),
            action: "check-port".into(),
        });
    }
    if low.contains("no route to host") || low.contains("network is unreachable") {
        d.push(Diagnosis {
            level: "error".into(),
            title: "网络不可达".into(),
            detail: "本机无法路由到目标地址".into(),
            fix: "检查本机网络、VPN 或代理状态；确认填写的不是内网地址".into(),
            action: String::new(),
        });
    }
    if low.contains("could not resolve hostname") || low.contains("name or service not known") {
        let host = if opts.hostname.trim().is_empty() {
            opts.alias.trim().to_string()
        } else {
            opts.hostname.trim().to_string()
        };
        d.push(Diagnosis {
            level: "error".into(),
            title: "域名解析失败".into(),
            detail: format!("无法解析 `{}`", host),
            fix: "检查拼写；若为内网域名，确认 DNS 或 hosts 配置".into(),
            action: String::new(),
        });
    }

    // 3) 认证类
    if low.contains("permission denied (publickey") || low.contains("no supported authentication") {
        let idf = opts.identity.trim();
        if idf.is_empty() && opts.alias.trim().is_empty() {
            d.push(Diagnosis {
                level: "error".into(),
                title: "未指定密钥，服务端拒绝了默认密钥".into(),
                detail: "直连模式下没有 -i 参数，ssh 只尝试默认密钥与 agent".into(),
                fix: "在测试表单中选择一把密钥，或改用 config 别名连接".into(),
                action: "pick-key".into(),
            });
        } else {
            let key_missing = if !idf.is_empty() {
                paths::expand(idf).map(|x| !x.is_file()).unwrap_or(false)
            } else {
                false
            };
            if key_missing {
                d.push(Diagnosis {
                    level: "error".into(),
                    title: "指定的密钥文件不存在".into(),
                    detail: format!("找不到 {}", idf),
                    fix: "在密钥库中确认文件名，或重新生成".into(),
                    action: "open-vault".into(),
                });
            } else {
                d.push(Diagnosis {
                    level: "error".into(),
                    title: "公钥未被服务端接受".into(),
                    detail: "服务端拒绝了提供的公钥，通常意味着公钥尚未写入目标机的 authorized_keys"
                        .into(),
                    fix: "复制该密钥的公钥并追加到服务器的 ~/.ssh/authorized_keys，或使用平台部署指引"
                        .into(),
                    action: "copy-public".into(),
                });
            }
        }
    }
    if low.contains("agent admitted failure")
        || low.contains("could not open a connection to your authentication agent")
    {
        d.push(Diagnosis {
            level: "error".into(),
            title: "ssh-agent 未运行".into(),
            detail: "带口令的密钥需要 agent 托管，否则非交互模式下无法使用".into(),
            fix: "在安全中心启动 ssh-agent，并把密钥加入 agent".into(),
            action: "start-agent".into(),
        });
    }
    if low.contains("invalid format") {
        d.push(Diagnosis {
            level: "error".into(),
            title: "密钥文件格式异常".into(),
            detail: "ssh 无法解析该私钥文件，可能文件损坏或并非私钥".into(),
            fix: "确认选择的是私钥而非公钥；如已损坏，从备份恢复或重新生成并重新部署公钥".into(),
            action: "open-vault".into(),
        });
    }
    if low.contains("bad permissions")
        || low.contains("permissions are too open")
        || low.contains("unprotected private key")
    {
        d.push(Diagnosis {
            level: "error".into(),
            title: "私钥权限过宽，被 ssh 主动拒绝".into(),
            detail: "OpenSSH 出于安全考虑会拒绝加载权限过于开放的私钥".into(),
            fix: "在密钥库或安全中心对该密钥执行权限修复（600）".into(),
            action: "fix-perms".into(),
        });
    }

    // 4) 主机密钥变更
    if low.contains("remote host identification has changed")
        || low.contains("host key verification failed")
    {
        d.push(Diagnosis {
            level: "error".into(),
            title: "主机指纹校验失败".into(),
            detail: "服务端指纹与 known_hosts 记录不一致：可能是服务器重装，也可能是中间人风险"
                .into(),
            fix: "先与服务器管理员核实指纹；确认无误后从 known_hosts 移除该条目再重试".into(),
            action: "show-known-hosts".into(),
        });
    }

    // 5) 非交互下无法输入口令
    if exit_code != 0 && (low.contains("batchmode") || low.contains("password")) {
        let has = d
            .iter()
            .any(|x| x.action == "start-agent" || x.title.contains("公钥未被服务端接受"));
        if !has {
            d.push(Diagnosis {
                level: "warn".into(),
                title: "认证需要口令，但测试禁止交互输入".into(),
                detail: "该密钥带口令保护，而测试运行在 BatchMode 下无法弹出口令输入".into(),
                fix: "把密钥加入 ssh-agent 后重试，这是唯一既安全又不破坏自动化的方式".into(),
                action: "start-agent".into(),
            });
        }
    }

    // 6) 未知失败：给出原始线索
    if d.is_empty() {
        d.push(Diagnosis {
            level: "error".into(),
            title: "连接失败".into(),
            detail: if stderr.trim().is_empty() {
                "ssh 未返回可用错误信息，可能是进程被中断".into()
            } else {
                "ssh 返回了未能自动识别的错误，请参考下方原始输出".into()
            },
            fix: "展开原始输出查看细节；可开启「详细模式」重试以获取更多线索".into(),
            action: "retry-verbose".into(),
        });
    }

    // agent 为空是很多认证失败的根因，补一条提示
    if matches!(env::detect_agent(), info if info.running && info.identities.is_empty()) {
        d.push(Diagnosis {
            level: "info".into(),
            title: "ssh-agent 正在运行但没有加载任何密钥".into(),
            detail: "agent 为空时不会参与认证".into(),
            fix: "把本次使用的密钥加入 agent，可同时解决口令与环境变量继承问题".into(),
            action: "add-agent".into(),
        });
    }

    d
}

fn one_line_summary(success: bool, d: &[Diagnosis], timed_out: bool) -> String {
    if success {
        return "连接成功，链路可用".into();
    }
    if timed_out {
        return "连接超时，未能建立会话".into();
    }
    if let Some(first) = d.iter().find(|x| x.level == "error") {
        return format!("失败：{}", first.title);
    }
    "连接失败".into()
}

/// 把参数数组拼成可读命令串（仅展示，不执行）
fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.contains(' ') || a.contains('"') {
                format!("\"{}\"", a.replace('"', "\\\""))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 校验 ssh 客户端可用性（供前端在测试前调用）
pub fn ssh_available() -> bool {
    env::find_ssh().is_some()
}

/// 读取 known_hosts 中与某主机相关的行（诊断用，只保留主机名与算法，不泄漏密钥）
pub fn known_hosts_entries(host: &str) -> Vec<String> {
    let Ok(dir) = paths::ssh_dir() else {
        return Vec::new();
    };
    let Ok(txt) = std::fs::read_to_string(dir.join("known_hosts")) else {
        return Vec::new();
    };
    let h = host.trim();
    if h.is_empty() {
        return Vec::new();
    }
    txt.lines()
        .filter(|l| l.contains(h))
        .map(|l| {
            let mut it = l.split_whitespace();
            let first = it.next().unwrap_or("");
            let algo = it.next().unwrap_or("");
            format!("{} {}", first, algo)
        })
        .collect()
}

/// 从 known_hosts 移除某主机的所有条目（主机指纹变更后的标准处置）
pub fn remove_known_host(host: &str) -> KhResult<usize> {
    let dir = paths::ssh_dir()?;
    let p = dir.join("known_hosts");
    if !p.is_file() {
        return Err(KhError::NotFound("known_hosts 不存在".into()));
    }
    let h = host.trim();
    if h.is_empty() {
        return Err(KhError::InvalidInput("主机名不能为空".into()));
    }
    // 备份后再改
    let bak = dir.join(format!(
        "known_hosts.keyhub-{}.bak",
        crate::time_util::file_stamp()
    ));
    std::fs::copy(&p, &bak)?;

    let txt = std::fs::read_to_string(&p)?;
    let mut kept = Vec::new();
    let mut removed = 0usize;
    for line in txt.lines() {
        let hit = line
            .split_whitespace()
            .next()
            .map(|first| {
                first == h
                    || first
                        .split(',')
                        .any(|part| part == h || part == &format!("[{}]:22", h))
            })
            .unwrap_or(false);
        if hit {
            removed += 1;
        } else {
            kept.push(line);
        }
    }
    if removed > 0 {
        let mut s = kept.join("\n");
        if !s.is_empty() {
            s.push('\n');
        }
        std::fs::write(&p, s.as_bytes())?;
        crate::audit::log(
            crate::audit::CAT_SECURITY,
            "knownhosts.remove",
            &format!("移除 known_hosts 中 {} 的 {} 条记录", h, removed),
            "warn",
            h,
            &paths::pretty(&bak),
        );
    }
    Ok(removed)
}

/// 供 UI 展示的命令预览（不执行、不联网），让用户清楚将要发生什么
pub fn preview_command(opts: TestOpts, timeout_secs: u32) -> String {
    let ssh = env::find_ssh()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "ssh".into());
    let timeout = if timeout_secs == 0 { 10 } else { timeout_secs };
    let mut args: Vec<String> = vec![
        "-o".into(),
        format!("ConnectTimeout={}", timeout),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "BatchMode=yes".into(),
    ];
    if !opts.alias.trim().is_empty() {
        args.push(opts.alias.trim().into());
    } else {
        if opts.port > 0 {
            args.push("-p".into());
            args.push(opts.port.to_string());
        }
        if !opts.identity.trim().is_empty() {
            args.push("-i".into());
            args.push(opts.identity.trim().into());
        }
        let host = opts.hostname.trim();
        let user = opts.user.trim();
        args.push(if user.is_empty() {
            host.to_string()
        } else {
            format!("{}@{}", user, host)
        });
    }
    args.push("echo KEYHUB_OK && uname -a".into());
    format!("{} {}", ssh, shell_join(&args))
}

/// 操作系统平台标识（诊断页「平台专属入口」用）
pub fn platform() -> String {
    std::env::consts::OS.to_string()
}
