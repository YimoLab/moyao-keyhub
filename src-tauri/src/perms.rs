//! 权限管理：分平台实现 PRD 3.1.4 的「一键权限修复」。
//!
//! - Unix / macOS：调用 chmod，目录 700、文件 600（原生语义）
//! - Windows：NTFS ACL 等效实现——禁用继承、仅当前用户可控
//!   （`icacls` 是系统自带工具，无需额外依赖，且行为可被审计）

use std::path::Path;

use crate::error::{KhError, KhResult};

/// 目录标准权限：700（仅属主可读写执行）
pub const DIR_MODE: u32 = 0o700;
/// 私钥 / config 标准权限：600（仅属主可读写）
pub const FILE_MODE: u32 = 0o600;

/// 当前权限的抽象描述，便于跨平台比对
///
/// 注意：平台实现只填自己关心的字段（Unix 填 display/compliant/...，
/// Windows 填 acl 判定），展示字段全部走 `fill()` 后补。
/// 因此这里用 `Default` 提供空值兜底，避免平台分支漏填字段。
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermInfo {
    /// 绝对路径
    #[serde(default)]
    pub path: String,
    /// 美化路径（~/...）
    #[serde(default)]
    pub pretty_path: String,
    /// 展示名（文件名）
    #[serde(default)]
    pub name: String,
    /// 是文件还是目录
    #[serde(default)]
    pub is_dir: bool,
    /// 人类可读的权限串（Unix 形如 `600`；Windows 形如 `owner-only` / `inherited`）
    pub display: String,
    /// 是否已达标准
    pub compliant: bool,
    /// 是否需要修复
    pub need_fix: bool,
    /// 是否组/其他用户可读
    pub group_or_other_readable: bool,
    /// 文件大小（字节），目录为 0
    #[serde(default)]
    pub size: u64,
    /// 修改时间 RFC3339
    #[serde(default)]
    pub modified_at: String,
}

/* ═══════════════════════════════════════════════════════════════
 * Unix 实现
 * ═══════════════════════════════════════════════════════════════ */

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    pub fn stat(path: &Path, is_dir: bool) -> KhResult<PermInfo> {
        let meta = std::fs::metadata(path)?;
        let mode = meta.permissions().mode() & 0o777;

        // 判定标准：不得对组/其他开放任何权限；且属主权限至少含读。
        // 严格比对会误伤 640 这类"更严"的组合，因此按"是否存在越权位"判定。
        let group_or_other = mode & 0o077 != 0;
        let owner_ok = mode & 0o600 == 0o600;
        let dir_x_ok = !is_dir || (mode & 0o100 == 0o100);
        let compliant = !group_or_other && owner_ok && dir_x_ok;

        Ok(PermInfo {
            display: format!("{:03o}", mode),
            compliant,
            need_fix: !compliant,
            group_or_other_readable: group_or_other,
            ..Default::default()
        })
    }

    pub fn fix(path: &Path, is_dir: bool) -> KhResult<()> {
        let want = if is_dir { DIR_MODE } else { FILE_MODE };
        let perm = std::fs::Permissions::from_mode(want);
        std::fs::set_permissions(path, perm)?;
        Ok(())
    }
}

/* ═══════════════════════════════════════════════════════════════
 * Windows 实现：NTFS ACL
 * ═══════════════════════════════════════════════════════════════
 * 等效策略：
 *  1. 关闭继承（/inheritance:r）
 *  2. 只授予当前用户完全控制（/grant:r "$USER:(F)"）
 * 这等价于 Unix 的 600 / 700：只有属主可访问。
 */

#[cfg(windows)]
mod imp {
    use super::*;
    use std::process::Command;

    /// 当前用户名（DOMAIN\user 形式）
    fn current_user() -> String {
        std::env::var("USERNAME").unwrap_or_else(|_| ".".into())
    }

    fn run_icacls(args: &[&str]) -> KhResult<String> {
        let out = hidden_command("icacls")
            .args(args)
            .output()
            .map_err(|e| KhError::CommandFailed(format!("无法调用 icacls：{e}")))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(KhError::CommandFailed(format!(
                "icacls 执行失败：{}",
                stderr.trim()
            )));
        }
        Ok(stdout)
    }

    pub fn stat(path: &Path, is_dir: bool) -> KhResult<PermInfo> {
        let _ = is_dir;
        // 读取 ACL，判断是否仍存在继承条目或 Builtin 组条目
        let p = path.to_string_lossy().to_string();
        let out = run_icacls(&[&p])?;

        // icacls 输出形如：
        //   C:\path NT AUTHORITY\SYSTEM:(I)(F)
        //           BUILTIN\Administrators:(I)(F)
        //           DESKTOP\User:(F)
        // 我们关心：是否还有 (I) 继承项，或是否存在 Users / Everyone / Authenticated Users 条目
        let mut inherited = false;
        let mut broad_grant = false;
        for line in out.lines().skip(1) {
            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if l.contains("(I)") {
                inherited = true;
            }
            let upper = l.to_uppercase();
            if upper.contains("BUILTIN\\USERS")
                || upper.contains("EVERYONE")
                || upper.contains("AUTHENTICATED USERS")
                || upper.contains("\\USERS:")
            {
                broad_grant = true;
            }
        }

        let compliant = !inherited && !broad_grant;
        Ok(PermInfo {
            is_dir,
            display: if compliant {
                "owner-only".into()
            } else if inherited {
                "inherited".into()
            } else {
                "broad-acl".into()
            },
            compliant,
            need_fix: !compliant,
            group_or_other_readable: broad_grant,
            size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            ..Default::default()
        })
    }

    pub fn fix(path: &Path, _is_dir: bool) -> KhResult<()> {
        let p = path.to_string_lossy().to_string();
        let user = format!("{}:(F)", current_user());

        // 1) 关闭继承并移除所有继承来的权限
        // 2) 重置为仅当前用户完全控制
        run_icacls(&[&p, "/inheritance:r", "/grant:r", &user, "/T", "/C", "/Q"])?;
        Ok(())
    }

    /// 创建进程时隐藏控制台窗口（避免桌面应用弹黑框）
    pub fn hidden_command(program: &str) -> Command {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut c = Command::new(program);
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }
}

/* ═══════════════════════════════════════════════════════════════
 * 统一出口
 * ═══════════════════════════════════════════════════════════════ */

/// 读取路径的权限信息
pub fn stat(path: &Path, is_dir: bool) -> KhResult<PermInfo> {
    imp::stat(path, is_dir)
}

/// 把路径权限修复为标准值
pub fn fix(path: &Path, is_dir: bool) -> KhResult<()> {
    imp::fix(path, is_dir)
}

/// 创建隐藏窗口的子进程（跨平台统一入口）
pub fn hidden_command(program: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        imp::hidden_command(program)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(program)
    }
}

/// 目录与文件的标准权限描述（用于 UI 文案）
///
/// Unix 用户会看到 `700` / `600`；Windows 下这个值不代表 mode，
/// 界面应改看 `PermInfo.compliant`，本函数仅在 Unix 语境下使用。
/// 当前由 `audit_permissions` 命令回传给前端做清单标题。
pub fn standard_label(is_dir: bool) -> &'static str {
    if is_dir {
        "700"
    } else {
        "600"
    }
}

/* ═══════════════════════════════════════════════════════════════
 * 目录树级操作（安全中心「一键修复权限」）
 * ═══════════════════════════════════════════════════════════════ */

/// 审计 `~/.ssh` 目录及其下所有敏感文件的权限。
///
/// 只遍历 `~/.ssh`，不涉及其它任何路径——这是红线的一部分。
pub fn audit_tree() -> KhResult<Vec<PermInfo>> {
    use crate::paths;

    let dir = paths::ssh_dir()?;
    let mut out = Vec::new();

    if !dir.is_dir() {
        return Ok(out);
    }

    // 1) 目录本身
    if let Ok(mut p) = stat(&dir, true) {
        fill(&mut p, &dir, true);
        out.push(p);
    }

    // 2) 目录下需要关注的条目
    let rd = std::fs::read_dir(&dir)?;
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();

        // 跳过与权限无关的噪声
        if name.starts_with('.') {
            continue;
        }

        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();

        // 只审计：目录、私钥、config、known_hosts、authorized_keys
        let interesting = is_dir
            || name == "config"
            || name == "known_hosts"
            || name.starts_with("authorized_keys")
            || crate::keys::looks_like_private_key(&path);
        if !interesting {
            continue;
        }

        if let Ok(mut p) = stat(&path, is_dir) {
            fill(&mut p, &path, is_dir);
            out.push(p);
        }
    }

    // 不达标项排前面，方便用户直接看到问题
    out.sort_by(|a, b| {
        a.need_fix
            .cmp(&b.need_fix)
            .reverse()
            .then_with(|| a.pretty_path.cmp(&b.pretty_path))
    });

    Ok(out)
}

/// 一键修复 `~/.ssh` 下所有不达标权限，返回修复项数。
pub fn fix_tree() -> KhResult<usize> {
    let items = audit_tree()?;
    let mut fixed = 0usize;
    for it in items.iter().filter(|i| i.need_fix) {
        let p = std::path::PathBuf::from(&it.path);
        if p.exists() && fix(&p, it.is_dir).is_ok() {
            fixed += 1;
        }
    }
    Ok(fixed)
}

/// 补全 PermInfo 的展示字段
fn fill(info: &mut PermInfo, path: &Path, is_dir: bool) {
    use crate::paths;
    info.path = path.to_string_lossy().to_string();
    info.pretty_path = paths::pretty(path);
    info.name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.ssh".into());
    info.is_dir = is_dir;
    if !is_dir {
        if let Ok(m) = std::fs::metadata(path) {
            info.size = m.len();
        }
    }
    info.modified_at = crate::time_util::file_time_rfc3339(path).unwrap_or_default();
}
