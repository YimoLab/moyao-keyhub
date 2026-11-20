// 墨钥 KeyHub · 构建脚本
//
// 在 tauri_build::build() 之外，额外做一件事：
// 把 WebView2Loader.dll 从 target/<profile>/build/webview2-com-sys-<hash>/out/<arch>/
// 复制到项目内固定路径 src-tauri/binaries/WebView2Loader.dll。
//
// 为什么需要这一步：
//   GNU 工具链（x86_64-pc-windows-gnu）下无法静态链接 WebView2，必须依赖
//   WebView2Loader.dll 在运行时动态加载（MSVC 工具链才支持静态链接）。
//   Tauri 自己的 build.rs 会把该 DLL 复制到 target/<profile>/ 供直接运行，
//   但**不会**把它交给打包器 —— 结果 NSIS 安装包只包含 keyhub.exe，
//   用户装完后启动即报「找不到 WebView2Loader.dll」。
//
//   而 DLL 的原始路径里带一个随构建变化的 hash 目录名（webview2-com-sys-<hash>），
//   无法在 tauri.conf.json 里硬编码。所以这里先把它落到固定路径，
//   再由 bundle.resources 引用该固定路径，从而正确打进安装包。
fn main() {
    // ⚠️ 顺序很重要：必须在 tauri_build::build() 之前把 DLL 归集好。
    // 因为 tauri_build::build() 会校验 tauri.conf.json 里 bundle.resources
    // 声明的路径是否存在，若此时 binaries/WebView2Loader.dll 还没生成，
    // 构建会直接失败：resource path `binaries\WebView2Loader.dll` doesn't exist
    #[cfg(windows)]
    stage_webview2_loader();

    tauri_build::build();
}

/// 把 WebView2Loader.dll 归集到 `binaries/WebView2Loader.dll`（固定路径）。
///
/// 源路径形如：
///   target/<profile>/build/webview2-com-sys-<hash>/out/x64/WebView2Loader.dll
/// hash 每次构建可能不同，故用遍历 + 目录名匹配的方式定位。
///
/// 注意：cargo 执行 build script 时的 cwd 就是 crate 根目录（src-tauri/），
/// 与 tauri.conf.json 中 resources 路径的解析基准一致，因此直接写相对路径即可。
#[cfg(windows)]
fn stage_webview2_loader() {
    use std::path::{Path, PathBuf};

    // OUT_DIR = target/<profile>/build/keyhub-<hash>/out
    // 往上三级即 target/<profile>
    let out_dir = match std::env::var("OUT_DIR") {
        Ok(v) => PathBuf::from(v),
        Err(_) => return,
    };
    let profile_dir = match out_dir.ancestors().nth(3) {
        Some(p) => p.to_path_buf(),
        None => return,
    };

    // 目标 CPU 架构目录名（WebView2 SDK 用 x64 / x86 / arm64）
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => return,
    };

    let dest_dir = Path::new("binaries");
    let dest = dest_dir.join("WebView2Loader.dll");

    // 候选源：所有 webview2-com-sys-* 目录下的 out/<arch>/WebView2Loader.dll
    let build_root = profile_dir.join("build");
    let entries = match std::fs::read_dir(&build_root) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut src: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let dir = entry.path();
        let is_wv2 = dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("webview2-com-sys-"))
            .unwrap_or(false);
        if !is_wv2 {
            continue;
        }
        let cand = dir.join("out").join(arch).join("WebView2Loader.dll");
        if cand.is_file() {
            src = Some(cand);
            break;
        }
    }

    // 清洁构建时 webview2-com-sys 可能尚未产出。
    // 若目标文件已存在（正常情况：之前的构建已归集过），直接复用，不必报错。
    let src = match src {
        Some(p) => p,
        None => {
            if dest.is_file() {
                return; // 已有归集产物，够 tauri_build 校验通过
            }
            // 既没有源也没有现成产物 —— 明确报错，而不是让 tauri_build 抛
            // 一句含义模糊的 "resource path ... doesn't exist"
            panic!(
                "未找到 WebView2Loader.dll 的构建输出（arch={}）。\n\
                 请先执行一次 `cargo build` 让 webview2-com-sys 完成编译，再打包。\n\
                 profile 目录: {}",
                arch,
                profile_dir.display()
            );
        }
    };

    if std::fs::create_dir_all(dest_dir).is_err() {
        panic!("无法创建目录 {}", dest_dir.display());
    }

    // 内容相同就不重复写，避免无谓地触发下游重新打包
    if let (Ok(a), Ok(b)) = (std::fs::read(&src), std::fs::read(&dest)) {
        if a == b {
            return;
        }
    }

    match std::fs::copy(&src, &dest) {
        Ok(_) => println!(
            "cargo:warning=已归集 WebView2Loader.dll -> {} (供 NSIS 打包使用)",
            dest.display()
        ),
        Err(e) => panic!("复制 WebView2Loader.dll 失败: {e}"),
    }
}
