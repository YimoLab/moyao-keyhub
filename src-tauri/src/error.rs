//! 墨钥 KeyHub · 统一错误模型
//!
//! 约定：所有面向前端的能力都返回 `Result<T, KhError>`。
//! `KhError` 序列化为 `"CODE:人类可读消息"` 字符串，前端据此做分支与提示。
//! 这样前端既拿到可编程的 code，也拿到可直接展示的中文文案。

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum KhError {
    /// 环境缺少可用的 ssh / ssh-keygen
    #[error("{0}")]
    SshMissing(String),

    /// 参数不合法（文件名、端口、算法等）
    #[error("{0}")]
    InvalidInput(String),

    /// 目标文件或目录不存在
    #[error("{0}")]
    NotFound(String),

    /// 当前用户对该路径没有权限
    #[error("{0}")]
    Permission(String),

    /// 文件已存在，拒绝覆盖
    #[error("{0}")]
    AlreadyExists(String),

    /// 命令执行失败（附 stderr 摘要）
    #[error("{0}")]
    CommandFailed(String),

    /// 加密 / 解密相关错误（口令错误、数据损坏）
    #[error("{0}")]
    Crypto(String),

    /// 平台不支持的能力
    ///
    /// 注意：在 Windows 上构建时，本变体只在 `#[cfg(not(windows))]` 分支里
    /// 被构造，因此会触发 `never constructed` 警告。这是跨平台代码的正常现象，
    /// 不是死代码 —— 它保证 Unix 上调用 Windows 专属能力时能给出明确错误。
    #[allow(dead_code)]
    #[error("{0}")]
    Unsupported(String),

    /// IO 兜底
    #[error("{0}")]
    Io(String),
}

impl KhError {
    /// 稳定的错误码，前端可据此分支处理。
    pub fn code(&self) -> &'static str {
        match self {
            KhError::SshMissing(_) => "SSH_MISSING",
            KhError::InvalidInput(_) => "INVALID_INPUT",
            KhError::NotFound(_) => "NOT_FOUND",
            KhError::Permission(_) => "PERMISSION",
            KhError::AlreadyExists(_) => "EXISTS",
            KhError::CommandFailed(_) => "COMMAND_FAILED",
            KhError::Crypto(_) => "CRYPTO",
            KhError::Unsupported(_) => "UNSUPPORTED",
            KhError::Io(_) => "IO",
        }
    }
}

impl Serialize for KhError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{}:{}", self.code(), self))
    }
}

impl From<std::io::Error> for KhError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        let msg = e.to_string();
        match e.kind() {
            NotFound => KhError::NotFound(msg),
            PermissionDenied => KhError::Permission(msg),
            AlreadyExists => KhError::AlreadyExists(msg),
            _ => KhError::Io(msg),
        }
    }
}

pub type KhResult<T> = Result<T, KhError>;
