//! `rustcodegraph upgrade` decision logic translated to Rust.
//!
//! 这个模块把“识别当前安装方式”和“生成/执行升级动作”放在一起，便于 CLI
//! 在 bundle、npm、npx、source checkout 等布局之间保持一致行为。

use std::process::Command;

pub const REPO: &str = "hunzhiwange/rustcodegraph";
pub const NPM_PACKAGE: &str = "rustcodegraph";
pub const INSTALL_SH_URL: &str =
    "https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMethod {
    /// cargo-dist/native bundle。Unix 和 Windows 的版本目录布局不同，需要分开处理。
    Bundle {
        os: BundleOs,
        bundle_root: String,
        install_dir: Option<String>,
    },
    Npm {
        scope: NpmScope,
    },
    Npx,
    /// 源码检出只能给出人工命令，避免在用户仓库里擅自 git pull 或重建。
    Source {
        root: String,
    },
    Unknown {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleOs {
    Unix,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpmScope {
    Global,
    Local,
}

pub struct DetectInput<'a, F>
where
    F: Fn(&str) -> bool,
{
    pub filename: &'a str,
    pub platform: &'a str,
    pub cwd: &'a str,
    pub exists: F,
}

pub fn derive_install_dir(
    bundle_root: &str,
    os: BundleOs,
    _exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let path = normalize_target_path(bundle_root);
    match os {
        // Windows installer 使用 <install_dir>/current 作为当前 bundle 根。
        BundleOs::Windows => basename(&path)
            .is_some_and(|name| name.eq_ignore_ascii_case("current"))
            .then(|| dirname(&path)),
        BundleOs::Unix => {
            // Unix cargo-dist 布局通常是 <install_dir>/versions/<version>。
            let parent = dirname(&path);
            if basename(&parent) == Some("versions") {
                Some(dirname(&parent))
            } else {
                None
            }
        }
    }
}

pub fn detect_install_method<F>(input: DetectInput<'_, F>) -> InstallMethod
where
    F: Fn(&str) -> bool,
{
    let is_win = input.platform == "win32";
    let filename = normalize_target_path(input.filename);
    let bin_dir = dirname(&filename);

    let binary_name = if is_win {
        "rustcodegraph.exe"
    } else {
        "rustcodegraph"
    };
    let rust_bundle_root = dirname(&bin_dir);
    let rust_binary = join_target_path(&join_target_path(&rust_bundle_root, "bin"), binary_name);
    // 先识别 native bundle，避免它内部的 package.json 被误判成 npm 安装。
    if filename == rust_binary
        && (input.exists)(&rust_binary)
        && (input.exists)(&join_target_path(&rust_bundle_root, "package.json"))
    {
        let os = if is_win {
            BundleOs::Windows
        } else {
            BundleOs::Unix
        };
        return InstallMethod::Bundle {
            os,
            install_dir: derive_install_dir(&rust_bundle_root, os, &input.exists),
            bundle_root: rust_bundle_root,
        };
    }

    if filename.contains("/_npx/") && is_rustcodegraph_npm_binary(&filename) {
        return InstallMethod::Npx;
    }
    if filename.contains("/node_modules/") && is_rustcodegraph_npm_binary(&filename) {
        let cwd = normalize_target_path(input.cwd);
        // cwd 下的 node_modules 视为本地依赖，否则按全局 npm 安装处理。
        return InstallMethod::Npm {
            scope: if filename.starts_with(&(cwd + "/")) {
                NpmScope::Local
            } else {
                NpmScope::Global
            },
        };
    }

    if let Some(repo_root) = find_source_checkout(&filename, input.cwd, &input.exists) {
        return InstallMethod::Source { root: repo_root };
    }

    let repo_root = resolve_target_path(&bin_dir, &["..", ".."]);
    if (input.exists)(&join_target_path(&repo_root, "package.json"))
        && (input.exists)(&join_target_path(&repo_root, ".git"))
    {
        return InstallMethod::Source { root: repo_root };
    }
    InstallMethod::Unknown {
        reason: format!("unrecognized install layout at {}", input.filename),
    }
}

fn is_rustcodegraph_npm_binary(filename: &str) -> bool {
    // 兼容旧 npm 包布局和 cargo-dist 新布局；升级入口需要能处理两代安装。
    let legacy_package_path = filename.contains("/node_modules/rustcodegraph/")
        || filename.contains("/node_modules/rustcodegraph-");
    let legacy_binary_path = filename.ends_with("/bin/rustcodegraph")
        || filename.ends_with("/bin/rustcodegraph.exe")
        || filename.ends_with("/bin/rustcodegraph.cmd");
    let cargo_dist_binary_path = filename
        .contains("/node_modules/rustcodegraph/node_modules/.bin_real/rustcodegraph")
        || filename
            .contains("/node_modules/rustcodegraph/node_modules/.bin_real/rustcodegraph.exe");
    (legacy_package_path && legacy_binary_path) || cargo_dist_binary_path
}

fn find_source_checkout<F>(filename: &str, cwd: &str, exists: &F) -> Option<String>
where
    F: Fn(&str) -> bool,
{
    let mut candidates = Vec::new();
    candidates.push(normalize_target_path(cwd));
    let mut dir = dirname(filename);
    // 从 binary 向上找有限层级，避免异常路径导致无限向根目录探测。
    for _ in 0..8 {
        candidates.push(dir.clone());
        let next = dirname(&dir);
        if next == dir {
            break;
        }
        dir = next;
    }
    candidates.into_iter().find(|candidate| {
        exists(&join_target_path(candidate, "Cargo.toml"))
            && exists(&join_target_path(candidate, ".git"))
    })
}

fn normalize_target_path(path: &str) -> String {
    // 所有检测逻辑统一用 slash path，Windows drive letter 仍由 join/resolve 保留。
    normalize_slash_path(&path.replace('\\', "/"))
}

fn resolve_target_path(base: &str, parts: &[&str]) -> String {
    let mut path = normalize_target_path(base);
    for part in parts {
        path = join_target_path(&path, part);
    }
    normalize_slash_path(&path)
}

fn normalize_slash_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            // 只做词法归一化，不访问文件系统；安装检测经常面对尚不存在的目标目录。
            if stack.last().is_some_and(|last| *last != "..") {
                stack.pop();
            } else if !absolute {
                stack.push(part);
            }
            continue;
        }
        stack.push(part);
    }

    let mut normalized = String::new();
    if absolute {
        normalized.push('/');
    }
    normalized.push_str(&stack.join("/"));
    if normalized.is_empty() {
        ".".to_owned()
    } else {
        normalized
    }
}

fn join_target_path(base: &str, child: &str) -> String {
    if child.is_empty() {
        return normalize_target_path(base);
    }
    if child.starts_with('/') || child.as_bytes().get(1) == Some(&b':') {
        return normalize_target_path(child);
    }
    normalize_slash_path(&format!("{}/{}", base.trim_end_matches('/'), child))
}

fn dirname(path: &str) -> String {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(0) => "/".to_owned(),
        Some(idx) => path[..idx].to_owned(),
        None => ".".to_owned(),
    }
}

fn basename(path: &str) -> Option<&str> {
    path.trim_end_matches('/').rsplit('/').next()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Semver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

pub fn parse_semver(version: &str) -> Option<Semver> {
    // 只解析发布标签需要的 major.minor.patch[-pre]，不实现完整 semver build metadata。
    let trimmed = version.trim().trim_start_matches('v');
    let (core, pre) = trimmed
        .split_once('-')
        .map(|(core, pre)| (core, Some(pre.to_owned())))
        .unwrap_or((trimmed, None));
    let mut parts = core.split('.');
    Some(Semver {
        major: parts.next()?.parse().ok()?,
        minor: parts.next()?.parse().ok()?,
        patch: parts.next()?.parse().ok()?,
        pre,
    })
}

pub fn compare_versions(a: &str, b: &str) -> Result<i8, String> {
    let a = parse_semver(a).ok_or_else(|| format!("cannot parse version: {a}"))?;
    let b = parse_semver(b).ok_or_else(|| format!("cannot parse version: {b}"))?;
    for (left, right) in [(a.major, b.major), (a.minor, b.minor), (a.patch, b.patch)] {
        if left != right {
            return Ok(if left > right { 1 } else { -1 });
        }
    }
    match (a.pre, b.pre) {
        // 预发布版本按字符串比较已足够覆盖当前发布标签；正式版高于同号预发布。
        (Some(left), Some(right)) => Ok(match left.cmp(&right) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }),
        (Some(_), None) => Ok(-1),
        (None, Some(_)) => Ok(1),
        (None, None) => Ok(0),
    }
}

pub fn is_update_available(current: &str, latest: &str) -> bool {
    compare_versions(latest, current)
        .map(|ordering| ordering > 0)
        .unwrap_or_else(|_| normalize_version(current) != normalize_version(latest))
}

pub fn normalize_version(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('v') {
        trimmed.to_owned()
    } else {
        format!("v{trimmed}")
    }
}

pub fn strip_v(value: &str) -> String {
    value.trim().trim_start_matches('v').to_owned()
}

pub fn parse_latest_tag_from_location(location: Option<&str>) -> Option<String> {
    let location = location?;
    let marker = "/releases/tag/";
    let start = location.find(marker)? + marker.len();
    let rest = &location[start..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

pub fn resolve_latest_version() -> Result<String, String> {
    let latest_url = format!("https://github.com/{REPO}/releases/latest");
    // 首选 GitHub latest 重定向，失败时再走 API；两者都用 curl 以保持 CLI 依赖简单。
    if let Ok(output) = Command::new("curl")
        .args([
            "-fsSLI",
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
            &latest_url,
        ])
        .output()
        && output.status.success()
    {
        let location = String::from_utf8_lossy(&output.stdout);
        if let Some(tag) = parse_latest_tag_from_location(Some(location.trim())) {
            return Ok(normalize_version(&tag));
        }
    }

    let api_url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    if let Ok(output) = Command::new("curl").args(["-fsSL", &api_url]).output()
        && output.status.success()
    {
        let body = String::from_utf8_lossy(&output.stdout);
        if let Some(tag) = parse_tag_name(&body) {
            return Ok(normalize_version(&tag));
        }
    }

    Err("could not resolve the latest version from GitHub. Check your network, or pin a version: `rustcodegraph upgrade <version>`.".to_owned())
}

fn parse_tag_name(body: &str) -> Option<String> {
    let marker = "\"tag_name\"";
    let start = body.find(marker)? + marker.len();
    let after_key = &body[start..];
    let colon = after_key.find(':')? + 1;
    let after_colon = after_key[colon..].trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let end = after_quote.find('"')?;
    Some(after_quote[..end].to_owned())
}

#[derive(Debug, Clone, Default)]
pub struct UpgradeOptions {
    pub version: Option<String>,
    pub check: bool,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradePlan {
    Check {
        current: String,
        latest: String,
        update_available: bool,
    },
    Run {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        advisory: String,
    },
    NothingToDo(String),
    Manual(String),
    Error(String),
}

type CommandRunner<'a> = dyn FnMut(&str, &[String], &[(String, String)]) -> i32 + 'a;

pub struct UpgradeDeps<'a> {
    /// 依赖注入让测试能验证命令、日志和网络解析，而不真的升级本机安装。
    pub current_version: String,
    pub method: InstallMethod,
    pub resolve_latest: Box<dyn FnMut() -> Result<String, String> + 'a>,
    pub run: Box<CommandRunner<'a>>,
    pub has_command: Box<dyn Fn(&str) -> bool + 'a>,
    pub log: Box<dyn FnMut(&str) + 'a>,
    pub warn: Box<dyn FnMut(&str) + 'a>,
    pub error: Box<dyn FnMut(&str) + 'a>,
    pub platform: String,
}

pub fn reindex_advisory() -> String {
    [
        "Your existing project indexes keep working, but were built by the previous version.",
        "To pick up this version's extraction improvements, refresh each project:",
        "  rustcodegraph sync        # incremental, fast",
        "  rustcodegraph index -f    # full rebuild",
        "(`rustcodegraph status` flags any index that predates the engine you're running.)",
    ]
    .join("\n")
}

pub fn run_upgrade(opts: UpgradeOptions, mut deps: UpgradeDeps<'_>) -> i32 {
    let latest = if let Some(version) = opts.version.as_deref() {
        normalize_version(version)
    } else {
        match (deps.resolve_latest)() {
            Ok(version) => normalize_version(&version),
            Err(error) => {
                (deps.error)(&error);
                return 1;
            }
        }
    };

    let current = normalize_version(&deps.current_version);
    let current_version = deps.current_version.clone();
    let update_available = is_update_available(&current_version, &latest);
    let label = if opts.version.is_some() {
        "target"
    } else {
        "latest"
    };
    (deps.log)(&format!("CodeGraph  current {current}  {label} {latest}"));

    if opts.check {
        if update_available {
            (deps.log)(&format!("An update is available: {current} -> {latest}"));
            (deps.log)("Run `rustcodegraph upgrade` to install it.");
        } else {
            (deps.log)(&format!("You're on the latest version ({current})."));
        }
        return 0;
    }
    if !update_available && !opts.force && opts.version.is_none() {
        (deps.log)(&format!("Already up to date ({current})."));
        (deps.log)(
            "Use `--force` to reinstall, or `rustcodegraph upgrade <version>` to change versions.",
        );
        return 0;
    }
    let method = deps.method.clone();
    // 运行路径和 plan_upgrade 保持同一套分支语义，只是这里会实际调用外部命令。
    match method {
        InstallMethod::Bundle {
            os: BundleOs::Unix,
            install_dir,
            ..
        } => upgrade_unix_bundle(
            install_dir,
            opts.version.as_ref().map(|_| latest),
            &mut deps,
        ),
        InstallMethod::Bundle {
            os: BundleOs::Windows,
            bundle_root,
            ..
        } => upgrade_windows_bundle(&bundle_root, &latest, &mut deps),
        InstallMethod::Npm { scope } => {
            let version_spec = if opts.version.is_some() {
                strip_v(&latest)
            } else {
                "latest".to_owned()
            };
            upgrade_npm(scope, &version_spec, &mut deps)
        }
        InstallMethod::Npx => {
            (deps.log)("npx always runs the latest version on demand - nothing to upgrade.");
            (deps.log)(&format!(
                "Force a fresh fetch with: npx {NPM_PACKAGE}@latest"
            ));
            0
        }
        InstallMethod::Source { root } => {
            (deps.warn)(&format!("Running from a source checkout at {root}."));
            (deps.log)("Upgrade it with: git pull && cargo build --release");
            0
        }
        InstallMethod::Unknown { reason } => {
            (deps.error)(&format!(
                "Couldn't determine how RustCodeGraph was installed ({reason})."
            ));
            (deps.log)(&format!(
                "Reinstall manually - see https://github.com/{REPO}#install"
            ));
            1
        }
    }
}

pub fn plan_upgrade(
    opts: UpgradeOptions,
    current_version: &str,
    latest_version: &str,
    method: InstallMethod,
) -> UpgradePlan {
    let latest = normalize_version(opts.version.as_deref().unwrap_or(latest_version));
    let current = normalize_version(current_version);
    let update_available = is_update_available(current_version, &latest);
    if opts.check {
        return UpgradePlan::Check {
            current,
            latest,
            update_available,
        };
    }
    if !update_available && !opts.force && opts.version.is_none() {
        return UpgradePlan::NothingToDo(format!("Already up to date ({current})."));
    }
    // 纯计划函数用于测试和未来 dry-run，不能触网、不能探测 PATH、不能执行命令。
    match method {
        InstallMethod::Bundle {
            os: BundleOs::Unix,
            install_dir,
            ..
        } => {
            let mut env = Vec::new();
            if let Some(dir) = install_dir {
                env.push(("RUSTCODEGRAPH_INSTALL_DIR".to_owned(), dir));
            }
            if opts.version.is_some() {
                env.push(("RUSTCODEGRAPH_VERSION".to_owned(), latest.clone()));
            }
            UpgradePlan::Run {
                command: "sh".to_owned(),
                args: vec!["-c".to_owned(), format!("curl -fsSL {INSTALL_SH_URL} | sh")],
                env,
                advisory: reindex_advisory(),
            }
        }
        InstallMethod::Bundle {
            os: BundleOs::Windows,
            bundle_root,
            ..
        } => UpgradePlan::Run {
            command: "powershell.exe".to_owned(),
            args: vec![
                "-NoProfile".to_owned(),
                "-ExecutionPolicy".to_owned(),
                "Bypass".to_owned(),
                "-EncodedCommand".to_owned(),
                encode_powershell_command(&build_windows_upgrade_script(
                    &bundle_root,
                    &latest,
                    "x64",
                )),
            ],
            env: Vec::new(),
            advisory: reindex_advisory(),
        },
        InstallMethod::Npm { scope } => {
            let version_spec = if opts.version.is_some() {
                strip_v(&latest)
            } else {
                "latest".to_owned()
            };
            let args = match scope {
                NpmScope::Global => vec![
                    "install".to_owned(),
                    "-g".to_owned(),
                    format!("{NPM_PACKAGE}@{version_spec}"),
                ],
                NpmScope::Local => vec![
                    "install".to_owned(),
                    format!("{NPM_PACKAGE}@{version_spec}"),
                ],
            };
            UpgradePlan::Run {
                command: "npm".to_owned(),
                args,
                env: Vec::new(),
                advisory: reindex_advisory(),
            }
        }
        InstallMethod::Npx => UpgradePlan::NothingToDo(
            "npx always runs the latest version on demand - nothing to upgrade.".to_owned(),
        ),
        InstallMethod::Source { root } => {
            UpgradePlan::Manual(format!("Running from a source checkout at {root}."))
        }
        InstallMethod::Unknown { reason } => UpgradePlan::Error(format!(
            "Couldn't determine how RustCodeGraph was installed ({reason})."
        )),
    }
}

fn upgrade_unix_bundle(
    install_dir: Option<String>,
    pinned: Option<String>,
    deps: &mut UpgradeDeps<'_>,
) -> i32 {
    // Unix bundle 复用 install.sh，这样 PATH 修正、版本 pin 和目录选择只维护一处。
    let downloader = if (deps.has_command)("curl") {
        Some(format!("curl -fsSL {INSTALL_SH_URL}"))
    } else if (deps.has_command)("wget") {
        Some(format!("wget -qO- {INSTALL_SH_URL}"))
    } else {
        None
    };

    let Some(downloader) = downloader else {
        (deps.error)("Neither curl nor wget is available to download the installer.");
        (deps.log)(&format!(
            "Install curl, or run manually:  {INSTALL_SH_URL} | sh"
        ));
        return 1;
    };

    let mut env = Vec::new();
    if let Some(dir) = install_dir {
        env.push(("RUSTCODEGRAPH_INSTALL_DIR".to_owned(), dir));
    }
    if let Some(version) = pinned {
        env.push(("RUSTCODEGRAPH_VERSION".to_owned(), version));
    }

    (deps.log)(&format!("Running the installer ({downloader} | sh)..."));
    let args = vec!["-c".to_owned(), format!("{downloader} | sh")];
    let code = (deps.run)("sh", &args, &env);
    if code != 0 {
        (deps.error)(&format!("Installer exited with code {code}."));
        return 1;
    }
    (deps.log)("");
    (deps.log)(
        "Upgrade complete. Open a new terminal if the version looks unchanged (PATH cache).",
    );
    (deps.log)(&reindex_advisory());
    0
}

fn upgrade_windows_bundle(bundle_root: &str, latest: &str, deps: &mut UpgradeDeps<'_>) -> i32 {
    let arch = if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        "x64"
    };
    let script = build_windows_upgrade_script(bundle_root, latest, arch);
    let encoded = encode_powershell_command(&script);
    (deps.log)(&format!("Downloading and installing {latest}..."));
    let args = vec![
        "-NoProfile".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-EncodedCommand".to_owned(),
        encoded,
    ];
    let code = (deps.run)("powershell.exe", &args, &[]);
    if code != 0 {
        (deps.error)(&format!("Installer exited with code {code}."));
        return 1;
    }
    (deps.log)("");
    (deps.log)("Upgrade complete. Open a new terminal to be safe (PATH/version cache).");
    (deps.log)(&reindex_advisory());
    0
}

fn upgrade_npm(scope: NpmScope, version_spec: &str, deps: &mut UpgradeDeps<'_>) -> i32 {
    // Windows npm shim 是 npm.cmd；直接调用 npm 在 std::process::Command 下可能找不到。
    let npm = if deps.platform == "win32" {
        "npm.cmd"
    } else {
        "npm"
    };
    let args = match scope {
        NpmScope::Global => vec![
            "install".to_owned(),
            "-g".to_owned(),
            format!("{NPM_PACKAGE}@{version_spec}"),
        ],
        NpmScope::Local => vec![
            "install".to_owned(),
            format!("{NPM_PACKAGE}@{version_spec}"),
        ],
    };
    (deps.log)(&format!("Running: {npm} {}", args.join(" ")));
    let code = (deps.run)(npm, &args, &[]);
    if code != 0 {
        (deps.error)(&format!("npm exited with code {code}."));
        if scope == NpmScope::Global {
            (deps.log)(
                "If this is a permissions error (EACCES), your global prefix needs sudo, or use a",
            );
            (deps.log)("Node version manager (nvm/fnm) so global installs don't require root.");
        }
        return 1;
    }
    (deps.log)("");
    (deps.log)("Upgrade complete.");
    (deps.log)(&reindex_advisory());
    0
}

pub fn build_windows_upgrade_script(bundle_root: &str, version: &str, arch: &str) -> String {
    let target = if arch == "arm64" {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let url =
        format!("https://github.com/{REPO}/releases/download/{version}/rustcodegraph-{target}.zip");
    [
        "$ErrorActionPreference='Stop'".to_owned(),
        format!("$dest='{bundle_root}'"),
        format!("$url='{url}'"),
        "Write-Host \"Downloading $url\"".to_owned(),
        "$tmp=Join-Path $env:TEMP ('cg-up-'+[guid]::NewGuid().ToString('N'))".to_owned(),
        "New-Item -ItemType Directory -Force -Path $tmp | Out-Null".to_owned(),
        "$zip=Join-Path $tmp 'cg.zip'".to_owned(),
        "Invoke-WebRequest -Uri $url -OutFile $zip".to_owned(),
        "$stage=Join-Path $tmp 'stage'".to_owned(),
        "Expand-Archive -Path $zip -DestinationPath $stage -Force".to_owned(),
        format!("$src=Join-Path $stage 'rustcodegraph-{target}'"),
        "if(-not (Test-Path $src)){throw 'rustcodegraph: archive missing rustcodegraph target directory'}".to_owned(),
        "$newExe=Join-Path $src 'bin\\rustcodegraph.exe'".to_owned(),
        "if(-not (Test-Path $newExe)){throw 'rustcodegraph: downloaded archive did not contain bin\\rustcodegraph.exe'}".to_owned(),
        "$binDir=Join-Path $dest 'bin'".to_owned(),
        "New-Item -ItemType Directory -Force -Path $binDir | Out-Null".to_owned(),
        "$exe=Join-Path $dest 'bin\\rustcodegraph.exe'".to_owned(),
        // Windows 不能覆盖正在运行的 exe，先重命名旧文件再复制新 bundle。
        "if(Test-Path $exe){Rename-Item -Path $exe -NewName ('rustcodegraph.exe.old-'+[guid]::NewGuid().ToString('N')) -Force}".to_owned(),
        "Copy-Item -Path (Join-Path $src '*') -Destination $dest -Recurse -Force".to_owned(),
        "$pkg=Join-Path $dest 'package.json'".to_owned(),
        format!("if(-not (Test-Path $pkg)){{'{{\"name\":\"rustcodegraph\",\"version\":\"{}\"}}' | Set-Content -Encoding UTF8 -Path $pkg}}", strip_v(version)),
        "Get-ChildItem -Path (Join-Path $dest 'bin') -Filter 'rustcodegraph.exe.old-*' -ErrorAction SilentlyContinue | ForEach-Object { try { Remove-Item $_.FullName -Force -ErrorAction Stop } catch {} }".to_owned(),
        "Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue".to_owned(),
        format!("Write-Host \"Installed RustCodeGraph {version} to $dest\""),
    ]
    .join(";")
}

pub fn encode_powershell_command(script: &str) -> String {
    // PowerShell -EncodedCommand 要求 UTF-16LE 后再 Base64。
    let bytes = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect::<Vec<_>>();
    base64_encode(&bytes)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn has_command(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(cmd);
        if candidate.is_file() {
            return true;
        }
        if cfg!(windows) {
            ["exe", "cmd", "bat", "com"]
                .iter()
                .any(|ext| dir.join(format!("{cmd}.{ext}")).is_file())
        } else {
            false
        }
    })
}

pub fn default_run(cmd: &str, args: &[String], env: &[(String, String)]) -> i32 {
    let mut command = Command::new(cmd);
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => -1,
    }
}
