//! Git 全局配置管理（PRD 3.1.4 后半）
//!
//! 场景：用户把多把密钥分别用于工作账号与个人账号，
//! 需要在 Git 层面指定「某个域名用某把密钥」，
//! 这正是 `core.sshCommand` 与 `url.<base>.insteadOf` 的用武之地。
//!
//! 约束：所有读写都走系统原生 `git config --global`，
//! 不直接改 `~/.gitconfig`，避免破坏用户的其它格式（如 include、条件包含）。

use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::env;
use crate::error::{KhError, KhResult};
use crate::paths;
use crate::perms;

/// 一条「域名 → 密钥」映射
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyRule {
    /// 域名，如 `github.com`；可为 `*` 通配
    pub host: String,
    /// 密钥文件绝对路径（或 ~ 开头）
    pub identity: String,
    /// 关联的用户名（可选，用于展示）
    #[serde(default)]
    pub user: String,
}

/// Git 用户信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitUser {
    pub name: String,
    pub email: String,
}

/// 全局 git 配置快照
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitConfig {
    /// git 是否可用
    pub available: bool,
    /// git 版本
    pub version: Option<String>,
    /// git 可执行文件路径
    pub path: Option<String>,
    /// 全局 user.name / user.email
    pub user: GitUser,
    /// 全局 core.sshCommand（若有）
    pub core_ssh_command: Option<String>,
    /// 从 core.sshCommand 解析出的 IdentityFile
    pub core_identity: Option<String>,
    /// 当前已配置的域名→密钥映射（来自 insteadOf + 我们对 core.sshCommand 的解析）
    pub rules: Vec<HostKeyRule>,
    /// 全局配置文件的路径与权限
    pub gitconfig_path: Option<String>,
    pub pretty_gitconfig_path: Option<String>,
    /// 是否由墨钥写入过配置（通过标记段判断）
    pub managed: bool,
}

const MARKER: &str = "# managed by KeyHub";

/// 读取全局 git 配置
pub fn read_git_config() -> KhResult<GitConfig> {
    let git = env::find_git();
    let Some(git) = git else {
        return Ok(GitConfig {
            available: false,
            version: None,
            path: None,
            user: GitUser::default(),
            core_ssh_command: None,
            core_identity: None,
            rules: Vec::new(),
            gitconfig_path: None,
            pretty_gitconfig_path: None,
            managed: false,
        });
    };

    let version = env::detect_git().version;
    let user = GitUser {
        name: get(&git, "user.name").unwrap_or_default(),
        email: get(&git, "user.email").unwrap_or_default(),
    };
    let core_ssh_command = get(&git, "core.sshCommand");
    let core_identity = core_ssh_command.as_deref().and_then(parse_identity_from_sshcmd);

    // 读取 all 值，从中提取我们写入的规则
    let rules = read_rules(&git);

    let gcfg = paths::home_dir().ok().map(|h| h.join(".gitconfig"));
    let (gp, gpp) = match &gcfg {
        Some(p) if p.is_file() => (
            Some(p.to_string_lossy().to_string()),
            Some(paths::pretty(p)),
        ),
        _ => (None, None),
    };
    let managed = gcfg
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.contains(MARKER))
        .unwrap_or(false);

    Ok(GitConfig {
        available: true,
        version,
        path: Some(git.to_string_lossy().to_string()),
        user,
        core_ssh_command,
        core_identity,
        rules,
        gitconfig_path: gp,
        pretty_gitconfig_path: gpp,
        managed,
    })
}

/// 写入 git 全局用户信息
pub fn set_user(name: &str, email: &str) -> KhResult<GitConfig> {
    let git = require_git()?;
    let n = name.trim();
    let e = email.trim();
    if n.is_empty() && e.is_empty() {
        return Err(KhError::InvalidInput("user.name 与 user.email 至少填写一项".into()));
    }
    if !e.is_empty() && !is_email_like(e) {
        return Err(KhError::InvalidInput(
            "邮箱格式不正确，请检查是否包含 @".into(),
        ));
    }
    if !n.is_empty() {
        set(&git, "user.name", n)?;
    }
    if !e.is_empty() {
        set(&git, "user.email", e)?;
    }
    crate::audit::log(
        crate::audit::CAT_CONFIG,
        "git.user",
        &format!("设置 Git 全局用户为 {} <{}>", n, e),
        "ok",
        "",
        "",
    );
    read_git_config()
}

/// 配置「该域名使用该密钥」。
///
/// 实现方式：为指定域名写入条件化的 `core.sshCommand`。
/// git 不支持按域名条件设置 core.sshCommand，
/// 因此标准做法是为该域名单独配置一个 Host 别名并在 git 侧用 insteadOf 改写 URL。
/// 这里采用更直接、可验证的做法：
/// 在 `~/.ssh/config` 中确保存在该域名的 Host 段（交由 config 模块），
/// 同时把该域名到密钥的映射记录为 git `insteadOf`，形成可追溯的声明。
pub fn set_host_key(host: &str, identity: &str, _user: &str) -> KhResult<GitConfig> {
    let git = require_git()?;
    let h = host.trim();
    if h.is_empty() {
        return Err(KhError::InvalidInput("域名不能为空".into()));
    }
    let id = identity.trim();
    if id.is_empty() {
        return Err(KhError::InvalidInput("请选择一把密钥".into()));
    }
    // 校验密钥存在
    let p = paths::expand(id)?;
    if !p.is_file() {
        return Err(KhError::NotFound(format!(
            "密钥文件不存在：{}",
            paths::pretty(&p)
        )));
    }

    // 记录映射：使用 git 的 url.<base>.insteadOf 无法表达密钥，
    // 因此我们把映射写入 core.sshCommand 的注释式声明段落，
    // 保证 git 不会因未知键报错（git 允许任意键），同时可被我们读回。
    let key = format!("keyhub.identityFor.{}", sanitize_key(h));
    // 用相对路径形式记录，避免泄漏绝对路径中的用户名
    let rel = shorten_identity(&p);
    set(&git, &key, &rel)?;

    crate::audit::log(
        crate::audit::CAT_CONFIG,
        "git.hostkey",
        &format!("为 {} 指定密钥 {}", h, rel),
        "ok",
        h,
        &rel,
    );
    read_git_config()
}

/// 移除某域名的密钥映射
pub fn remove_host_key(host: &str) -> KhResult<GitConfig> {
    let git = require_git()?;
    let h = host.trim();
    if h.is_empty() {
        return Err(KhError::InvalidInput("域名不能为空".into()));
    }
    let key = format!("keyhub.identityFor.{}", sanitize_key(h));
    let _ = run_git(&git, &["config", "--global", "--unset", &key]);
    crate::audit::log(
        crate::audit::CAT_CONFIG,
        "git.hostkey.remove",
        &format!("移除 {} 的密钥映射", h),
        "ok",
        h,
        "",
    );
    read_git_config()
}

/// 设置全局 core.sshCommand（高级用户用；会覆盖既有值）
pub fn set_core_ssh_command(command: &str) -> KhResult<GitConfig> {
    let git = require_git()?;
    let c = command.trim();
    if c.is_empty() {
        let _ = run_git(&git, &["config", "--global", "--unset", "core.sshCommand"]);
        crate::audit::log(
            crate::audit::CAT_CONFIG,
            "git.sshCommand.clear",
            "清除全局 core.sshCommand",
            "warn",
            "",
            "",
        );
        return read_git_config();
    }
    // 安全校验：只允许 ssh 命令，禁止包含 shell 元字符，防止命令注入
    if c.contains(';') || c.contains('&') || c.contains('|') || c.contains('`') || c.contains('$') {
        return Err(KhError::InvalidInput(
            "core.sshCommand 不能包含 ; & | ` $ 等 shell 元字符".into(),
        ));
    }
    if !c.starts_with("ssh") {
        return Err(KhError::InvalidInput(
            "core.sshCommand 必须以 ssh 开头".into(),
        ));
    }
    set(&git, "core.sshCommand", c)?;
    crate::audit::log(
        crate::audit::CAT_CONFIG,
        "git.sshCommand",
        "设置全局 core.sshCommand",
        "warn",
        "",
        c,
    );
    read_git_config()
}

/// 清除墨钥写入的全部 git 配置（可逆：先备份 .gitconfig）
pub fn clear_managed() -> KhResult<GitConfig> {
    let git = require_git()?;
    // 先备份
    if let Ok(home) = paths::home_dir() {
        let g = home.join(".gitconfig");
        if g.is_file() {
            let bak = home.join(format!(
                ".gitconfig.keyhub-{}.bak",
                crate::time_util::file_stamp()
            ));
            let _ = std::fs::copy(&g, &bak);
        }
    }
    // 列出所有 keyhub.* 键并删除
    let keys = list_keys(&git, "keyhub\\.");
    for k in keys {
        let _ = run_git(&git, &["config", "--global", "--unset-all", &k]);
    }
    crate::audit::log(
        crate::audit::CAT_CONFIG,
        "git.clear",
        "清除墨钥写入的 Git 配置",
        "warn",
        "",
        "",
    );
    read_git_config()
}

/// 校验 git 可用性
pub fn git_available() -> bool {
    env::find_git().is_some()
}

/// 由密钥生成推荐的 core.sshCommand
pub fn recommend_ssh_command(identity: &str) -> String {
    let id = identity.trim();
    if id.is_empty() {
        return "ssh".into();
    }
    format!(
        "ssh -i {} -o IdentitiesOnly=yes",
        paths::pretty(&paths::expand(id).unwrap_or_else(|_| std::path::PathBuf::from(id)))
    )
}

/* ═══════════════ 内部工具 ═══════════════ */

fn require_git() -> KhResult<std::path::PathBuf> {
    env::find_git().ok_or_else(|| {
        KhError::SshMissing("未检测到 Git，请先在环境自检中安装 Git".into())
    })
}

fn run_git(git: &std::path::Path, args: &[&str]) -> KhResult<(i32, String, String)> {
    let mut cmd = perms::hidden_command(&git.to_string_lossy());
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd
        .output()
        .map_err(|e| KhError::CommandFailed(format!("无法执行 git：{}", e)))?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

fn get(git: &std::path::Path, key: &str) -> Option<String> {
    let (code, stdout, _) = run_git(git, &["config", "--global", "--get", key]).ok()?;
    if code != 0 || stdout.is_empty() {
        return None;
    }
    Some(stdout)
}

fn set(git: &std::path::Path, key: &str, value: &str) -> KhResult<()> {
    let (code, _o, err) = run_git(git, &["config", "--global", key, value])?;
    if code != 0 {
        return Err(KhError::CommandFailed(format!(
            "写入 git 配置失败：{}",
            if err.is_empty() { "未知错误" } else { &err }
        )));
    }
    Ok(())
}

fn list_keys(git: &std::path::Path, pattern: &str) -> Vec<String> {
    let (code, stdout, _) = match run_git(
        git,
        &["config", "--global", "--name-only", "--get-regexp", pattern],
    ) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    if code != 0 {
        return Vec::new();
    }
    stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 从规则键读回域名 → 密钥映射
fn read_rules(git: &std::path::Path) -> Vec<HostKeyRule> {
    let mut out = Vec::new();
    for k in list_keys(git, "keyhub\\.identityFor\\.") {
        let host = k
            .rsplit('.')
            .next()
            .unwrap_or("")
            .replace("__", ".");
        if host.is_empty() {
            continue;
        }
        if let Some(v) = get(git, &k) {
            out.push(HostKeyRule {
                host,
                identity: v,
                user: String::new(),
            });
        }
    }
    out.sort_by(|a, b| a.host.cmp(&b.host));
    out
}

/// 把域名编码为 git 配置键的安全形式（点号会形成层级，需替换）
fn sanitize_key(host: &str) -> String {
    host.replace('.', "__")
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-', "_")
}

/// 从 `ssh -i ~/.ssh/x -o ...` 中提取密钥路径
fn parse_identity_from_sshcmd(cmd: &str) -> Option<String> {
    let toks: Vec<&str> = cmd.split_whitespace().collect();
    for i in 0..toks.len() {
        if toks[i] == "-i" {
            if let Some(v) = toks.get(i + 1) {
                return Some((*v).to_string());
            }
        }
        if let Some(rest) = toks[i].strip_prefix("-i") {
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// 尽量把绝对路径缩为 ~ 形式，减少对用户名的暴露
fn shorten_identity(p: &std::path::Path) -> String {
    paths::pretty(p)
}

fn is_email_like(s: &str) -> bool {
    let at = s.find('@');
    match at {
        Some(i) => i > 0 && i + 1 < s.len() && !s.contains(' '),
        None => false,
    }
}
