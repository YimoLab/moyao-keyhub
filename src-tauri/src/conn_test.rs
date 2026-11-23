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
//!
//! 远端策略分两类：
//! - 普通主机：远端执行只读探测 `echo KEYHUB_OK && uname -a`，以退出码 + 标记判定成功
//! - Git 托管平台（GitHub / GitLab / Gitee / 华为云 CodeArts 等）不提供 shell，
//!   不接受任意命令。对它们只做 `ssh -T` 认证探测，并以服务端欢迎语判定成功，
//!   不能依赖退出码（GitHub 返回 1、CodeArts 返回 128）

use std::io::Read;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config;
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
    /// 远端类型：`shell`（普通主机）或 `git-hosting`（Git 托管平台）
    #[serde(default)]
    pub remote_kind: String,
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

/// 远端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteKind {
    /// 普通 shell 主机，可执行远端命令
    Shell,
    /// Git 托管平台，只提供 git 协议，不提供 shell
    GitHosting,
}

impl RemoteKind {
    fn as_str(self) -> &'static str {
        match self {
            RemoteKind::Shell => "shell",
            RemoteKind::GitHosting => "git-hosting",
        }
    }

    fn is_git(self) -> bool {
        matches!(self, RemoteKind::GitHosting)
    }
}

/// 别名 / 直连参数解析后的实际连接身份。
///
/// 测试表单走别名模式时只传别名，HostName / User 都在 config 里；
/// 没有这两个值就无法判断远端是普通主机还是 Git 托管平台，
/// 也无法在诊断里指出「User 填错了」。
#[derive(Debug, Clone, Default)]
struct EffectiveTarget {
    hostname: String,
    user: String,
}

fn resolve_target(opts: &TestOpts) -> EffectiveTarget {
    let alias = opts.alias.trim();
    if !alias.is_empty() {
        if let Ok(doc) = config::read_config() {
            if let Some(h) = doc
                .hosts
                .iter()
                .find(|h| h.alias.trim().eq_ignore_ascii_case(alias))
            {
                return EffectiveTarget {
                    // 未配置 HostName 时，OpenSSH 会把别名本身当作域名
                    hostname: if h.hostname.trim().is_empty() {
                        alias.to_string()
                    } else {
                        h.hostname.trim().to_string()
                    },
                    user: h.user.trim().to_string(),
                };
            }
        }
        // 读不到 config 或没有该 Host：仍按别名当域名处理
        return EffectiveTarget {
            hostname: alias.to_string(),
            user: String::new(),
        };
    }
    EffectiveTarget {
        hostname: opts.hostname.trim().to_string(),
        user: opts.user.trim().to_string(),
    }
}

/// 判断远端是否为 Git 托管平台。
///
/// 两条线索：
/// - 用户名固定为 `git` 是业界通行约定（GitHub / GitLab / Gitee / CodeArts 都是）
/// - 域名属于已知托管服务，或带有 `gitlab` / `gitea` / `codehub` 等自建服务特征
fn classify_remote(t: &EffectiveTarget) -> RemoteKind {
    let host = t
        .hostname
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let user = t.user.trim().to_ascii_lowercase();

    if user == "git" {
        return RemoteKind::GitHosting;
    }
    if host.is_empty() {
        return RemoteKind::Shell;
    }

    const GIT_DOMAINS: &[&str] = &[
        "github.com",
        "gitlab.com",
        "gitee.com",
        "codeberg.org",
        "bitbucket.org",
        "gitcode.com",
        "git.sr.ht",
        "ssh.dev.azure.com",
    ];
    if GIT_DOMAINS.iter().any(|d| {
        host == *d
            || host
                .strip_suffix(d)
                .map(|prefix| prefix.ends_with('.'))
                .unwrap_or(false)
    }) {
        return RemoteKind::GitHosting;
    }

    // 自建 GitLab / Gitea / 华为云 CodeArts 等：域名里带服务特征
    const GIT_KEYWORDS: &[&str] = &["gitlab", "gitea", "gogs", "codehub", "codearts"];
    if GIT_KEYWORDS.iter().any(|k| host.contains(k)) || host.starts_with("git.") {
        return RemoteKind::GitHosting;
    }

    RemoteKind::Shell
}

/// Git 托管平台认证成功的判定。
///
/// 这些服务的 `ssh -T` 普遍以非 0 退出：GitHub 返回 1，华为云 CodeArts 返回 128。
/// 退出码只说明「没有 shell 会话」，不代表失败，必须看服务端欢迎语。
/// 反过来说，ssh 在认证失败时不会返回 0，所以退出码 0 同样可以判定成功。
fn git_auth_succeeded(output: &str, exit_code: i32) -> bool {
    let low = output.to_ascii_lowercase();
    const OK_MARKERS: &[&str] = &[
        "successfully authenticated",
        "does not provide shell access",
        "shell access is disabled",
        "welcome to gitlab",
        "welcome to codearts repo",
        "welcome to gitee",
        "authenticated via ssh key",
    ];
    if OK_MARKERS.iter().any(|m| low.contains(m)) {
        return true;
    }
    exit_code == 0
}

/// User 字段是否被填成了地址 / 路径的一部分。
///
/// 典型误填：把 `git@host:org/repo.git` 里的 `org/repo` 当作 User。
/// 这种值不可能是合法登录名，认证必然失败。
fn user_looks_like_path(user: &str) -> bool {
    let u = user.trim();
    !u.is_empty()
        && (u.contains('/')
            || u.contains('\\')
            || u.contains(':')
            || u.contains(char::is_whitespace))
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
    let effective = resolve_target(&opts);
    let kind = classify_remote(&effective);

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

    if kind.is_git() {
        // Git 托管平台不提供 shell：只做认证探测，不发送任何远端命令
        args.push("-T".into());
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

    // 普通主机：远端执行只读探测命令确认链路可用。
    // Git 托管平台没有 shell，发送任何命令都会被拒绝（如 CodeArts 返回
    // `invalid action`），所以只靠 `-T` 完成握手与认证即可。
    if !kind.is_git() {
        args.push("echo KEYHUB_OK && uname -a".into());
    }

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

    let raw_output = format!("{}\n{}", stdout, stderr);
    let success = !timed_out
        && match kind {
            RemoteKind::Shell => exit_code == 0 && stdout.contains("KEYHUB_OK"),
            RemoteKind::GitHosting => git_auth_succeeded(&raw_output, exit_code),
        };

    // 脱敏：把 home 路径换成 ~
    let stdout = sanitize(&stdout);
    let stderr = sanitize(&stderr);
    // Git 平台的欢迎语走 stderr（形如 `remote: Welcome to ...`）；
    // 普通主机只看 stdout，避免把失败时的报错当成远端标识
    let banner_src = if kind.is_git() {
        format!("{}\n{}", stdout, stderr)
    } else {
        stdout.clone()
    };
    let remote_banner = if success {
        let candidates: Vec<&str> = banner_src
            .lines()
            .map(|s| s.trim())
            .map(|s| s.strip_prefix("remote:").unwrap_or(s).trim())
            .filter(|s| !s.is_empty() && !s.contains("KEYHUB_OK"))
            .collect();
        let pick = if kind.is_git() {
            // 详细模式下 stderr 前几行是 OpenSSH 的调试信息，
            // 优先挑出平台真正的欢迎语
            candidates
                .iter()
                .find(|s| {
                    let low = s.to_ascii_lowercase();
                    low.contains("welcome")
                        || low.contains("authenticated")
                        || low.contains("shell access")
                })
                .or_else(|| candidates.first())
        } else {
            candidates.first()
        };
        pick.map(|s| s.to_string())
    } else {
        // 失败时 stderr 里是报错信息，不应冒充「远端标识」
        None
    };

    let diagnoses = diagnose(
        &opts,
        &effective,
        kind,
        timed_out,
        exit_code,
        &stderr,
        success,
    );
    let summary = one_line_summary(success, kind, &diagnoses, timed_out);

    let result = TestResult {
        target: target.clone(),
        command: display_cmd,
        remote_kind: kind.as_str().into(),
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
    target: &EffectiveTarget,
    kind: RemoteKind,
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
            title: if kind.is_git() {
                "认证成功".into()
            } else {
                "链路正常".into()
            },
            detail: if kind.is_git() {
                "已完成 SSH 握手并通过公钥认证。Git 托管平台不提供 shell，退出码非 0 属正常现象"
                    .into()
            } else {
                "已完成 SSH 握手并在远端执行了只读探测命令".into()
            },
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
        let host = if target.hostname.trim().is_empty() {
            opts.alias.trim().to_string()
        } else {
            target.hostname.trim().to_string()
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
        let alias = opts.alias.trim();
        let user = target.user.trim();

        if user_looks_like_path(user) {
            // 最常见的新手误填：把 clone 地址里的仓库路径当成了用户名。
            // 此时再提示「公钥没写入 authorized_keys」只会把排查引向错误方向。
            d.push(Diagnosis {
                level: "error".into(),
                title: "User 字段填的不是用户名".into(),
                detail: format!(
                    "当前 User 为 `{}`，其中包含路径/地址分隔符，ssh 会把它当作登录名发起认证",
                    user
                ),
                fix: if alias.is_empty() {
                    "把「登录用户」改成真正的用户名：Git 托管平台通常固定为 git，仓库路径（如 org/repo）只属于 clone 地址".into()
                } else {
                    format!(
                        "到「SSH Config」把 Host `{}` 的 User 改成真正的登录名：Git 托管平台通常固定为 git",
                        alias
                    )
                },
                action: "open-config".into(),
            });
        } else if kind.is_git() && user.is_empty() {
            d.push(Diagnosis {
                level: "error".into(),
                title: "Git 托管平台需要显式指定 User".into(),
                detail: "当前 Host 没有配置 User，ssh 会使用本机系统用户名；Git 托管平台只按 git 用户匹配公钥"
                    .into(),
                fix: "在 config 中为该 Host 增加 `User git`".into(),
                action: "open-config".into(),
            });
        } else if kind.is_git() && !user.eq_ignore_ascii_case("git") {
            d.push(Diagnosis {
                level: "error".into(),
                title: "Git 托管平台的 SSH 用户名通常是 git".into(),
                detail: format!(
                    "当前 User 为 `{}`。GitHub / GitLab / Gitee / CodeArts 等服务只用 `git` 用户匹配公钥，填账号名会导致 publickey 被拒",
                    user
                ),
                fix: "把该 Host 的 User 改为 git 后重试".into(),
                action: "open-config".into(),
            });
        } else if idf.is_empty() && alias.is_empty() {
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
            } else if !alias.is_empty() {
                // 别名模式下 IdentityFile / User 都由 config 决定，不能只怪 authorized_keys
                d.push(Diagnosis {
                    level: "error".into(),
                    title: "公钥未被服务端接受".into(),
                    detail: format!("服务端拒绝了 config 别名 `{}` 使用的密钥", alias),
                    fix: "检查该 Host 的 User、IdentityFile 是否正确；确认对应公钥已添加到代码托管平台或服务器的 authorized_keys"
                        .into(),
                    action: "open-config".into(),
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
        // 已经有明确的 error 结论时不再补口令提示，避免把排查方向带偏
        let has = d.iter().any(|x| x.level == "error" || x.action == "start-agent");
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

fn one_line_summary(success: bool, kind: RemoteKind, d: &[Diagnosis], timed_out: bool) -> String {
    if success {
        return if kind.is_git() {
            "认证成功（Git 托管平台不提供 shell）".into()
        } else {
            "连接成功，链路可用".into()
        };
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
    let effective = resolve_target(&opts);
    let kind = classify_remote(&effective);
    if kind.is_git() {
        // 与真实执行保持一致：Git 托管平台只做认证探测
        args.push("-T".into());
    }
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
    if !kind.is_git() {
        args.push("echo KEYHUB_OK && uname -a".into());
    }
    format!("{} {}", ssh, shell_join(&args))
}

/// 操作系统平台标识（诊断页「平台专属入口」用）
pub fn platform() -> String {
    std::env::consts::OS.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(host: &str, user: &str) -> RemoteKind {
        classify_remote(&EffectiveTarget {
            hostname: host.into(),
            user: user.into(),
        })
    }

    /// 已知 Git 托管域名不依赖 User 也能识别
    #[test]
    fn known_git_hosts_are_classified() {
        assert_eq!(kind_of("github.com", ""), RemoteKind::GitHosting);
        assert_eq!(kind_of("gitee.com", "YimoLab"), RemoteKind::GitHosting);
        assert_eq!(
            kind_of("codehub.devcloud.cn-north-4.huaweicloud.com", "git"),
            RemoteKind::GitHosting
        );
    }

    /// 自建 GitLab / Gitea 没有固定域名，靠 HostName 特征识别
    #[test]
    fn self_hosted_git_services_are_classified() {
        assert_eq!(
            kind_of("gitlab.zyys.tech", "leijing"),
            RemoteKind::GitHosting
        );
        assert_eq!(kind_of("git.example.com", "root"), RemoteKind::GitHosting);
        assert_eq!(kind_of("192.168.1.10", "root"), RemoteKind::Shell);
    }

    /// Git 服务的欢迎语必须能覆盖非 0 退出码（CodeArts 为 128、GitHub 为 1）
    #[test]
    fn git_auth_markers_override_nonzero_exit() {
        assert!(git_auth_succeeded(
            "remote: Welcome to CodeArts Repo, a601e08238e4497a88450a0e217c4c77",
            128
        ));
        assert!(git_auth_succeeded(
            "Hi alice! You've successfully authenticated, but GitHub does not provide shell access.",
            1
        ));
        assert!(git_auth_succeeded(
            "Welcome to GitLab, @alice!",
            0
        ));
    }

    /// 认证失败时即使输出里带 password 等词也不能误判成功
    #[test]
    fn publickey_denied_is_not_success() {
        assert!(!git_auth_succeeded(
            "git@github.com: Permission denied (publickey).",
            255
        ));
    }

    /// 把 clone 地址里的 org/repo 当 User 是本次实际踩到的坑
    #[test]
    fn repository_path_in_user_is_detected() {
        assert!(user_looks_like_path("ftms_dms_devops/DMSsrv-TCCN-Dev-leijing"));
        assert!(user_looks_like_path("host:repo"));
        assert!(!user_looks_like_path("git"));
        assert!(!user_looks_like_path("leijing"));
    }

    /// 别名模式下解析出的 User / HostName 要用于平台判定
    #[test]
    fn classify_uses_effective_target() {
        let t = EffectiveTarget {
            hostname: "codehub.devcloud.cn-north-4.huaweicloud.com".into(),
            user: "ftms_dms_devops/DMSsrv-TCCN-Dev-leijing".into(),
        };
        // 域名命中 CodeArts，User 虽然填错也仍按 Git 平台诊断
        assert_eq!(classify_remote(&t), RemoteKind::GitHosting);
        assert!(user_looks_like_path(&t.user));
    }
}
