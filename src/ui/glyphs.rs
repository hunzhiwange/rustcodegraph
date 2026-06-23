//! Glyph selection for CLI output.
//!
//! CLI 输出共用这一组符号表，避免进度条、树形输出和状态消息各自判断终端能力。
//! Windows 默认走 ASCII，是为了避开旧控制台 raw write 下的 UTF-8 乱码问题。

use std::env;
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyphs {
    pub ok: &'static str,
    pub err: &'static str,
    pub info: &'static str,
    pub warn: &'static str,
    pub spinner: &'static [&'static str],
    pub bar_filled: &'static str,
    pub bar_empty: &'static str,
    pub rail: &'static str,
    pub phase_done: &'static str,
    pub dash: &'static str,
    pub h_line: &'static str,
    pub tree_branch: &'static str,
    pub tree_last: &'static str,
    pub tree_pipe: &'static str,
}

/// Unicode 版本优先可读性；所有字符都集中在这里，便于快照测试逐项检查。
pub static UNICODE_GLYPHS: Glyphs = Glyphs {
    ok: "\u{2713}",
    err: "\u{2717}",
    info: "\u{2139}",
    warn: "\u{26a0}",
    spinner: &[
        "\u{00b7}", "\u{2722}", "\u{2733}", "\u{2736}", "\u{273b}", "\u{273d}",
    ],
    bar_filled: "\u{2588}",
    bar_empty: "\u{2591}",
    rail: "\u{2502}",
    phase_done: "\u{25c6}",
    dash: "\u{2014}",
    h_line: "\u{2500}",
    tree_branch: "\u{251c}\u{2500}\u{2500} ",
    tree_last: "\u{2514}\u{2500}\u{2500} ",
    tree_pipe: "\u{2502}   ",
};

/// ASCII 版本保证 7-bit 输出，适合 Windows、Linux console 和受限 CI 终端。
pub static ASCII_GLYPHS: Glyphs = Glyphs {
    ok: "[OK]",
    err: "[ERR]",
    info: "[i]",
    warn: "[!]",
    spinner: &[".", "*", "+", "x", "o", "O"],
    bar_filled: "#",
    bar_empty: "-",
    rail: "|",
    phase_done: "*",
    dash: "-",
    h_line: "-",
    tree_branch: "|-- ",
    tree_last: "`-- ",
    tree_pipe: "|   ",
};

static CACHED: OnceLock<RwLock<Option<Glyphs>>> = OnceLock::new();

#[cfg(test)]
static PLATFORM_OVERRIDE: OnceLock<RwLock<Option<&'static str>>> = OnceLock::new();

fn current_platform() -> &'static str {
    #[cfg(test)]
    {
        let cell = PLATFORM_OVERRIDE.get_or_init(|| RwLock::new(None));
        if let Some(platform) = *cell.read().expect("platform override poisoned") {
            return platform;
        }
    }

    if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        env::consts::OS
    }
}

pub fn supports_unicode() -> bool {
    // 显式环境变量用于测试、CI 和用户手动修正终端探测结果。
    if env::var("RUSTCODEGRAPH_ASCII").ok().as_deref() == Some("1") {
        return false;
    }
    if env::var("RUSTCODEGRAPH_UNICODE").ok().as_deref() == Some("1") {
        return true;
    }
    if current_platform() == "win32" {
        return false;
    }
    // Linux 虚拟控制台通常不保证这些绘图字符，普通终端则默认允许 Unicode。
    env::var("TERM").ok().as_deref() != Some("linux")
}

pub fn get_glyphs() -> Glyphs {
    // 终端能力在进程生命周期内基本稳定，缓存可避免频繁读取环境变量。
    let cell = CACHED.get_or_init(|| RwLock::new(None));
    {
        let cached = cell.read().expect("glyph cache poisoned");
        if let Some(glyphs) = cached.clone() {
            return glyphs;
        }
    }
    let glyphs = if supports_unicode() {
        UNICODE_GLYPHS.clone()
    } else {
        ASCII_GLYPHS.clone()
    };
    *cell.write().expect("glyph cache poisoned") = Some(glyphs.clone());
    glyphs
}

pub fn reset_glyphs_cache() {
    if let Some(cell) = CACHED.get() {
        *cell.write().expect("glyph cache poisoned") = None;
    }
}

#[cfg(test)]
pub fn set_platform_for_test(platform: &'static str) {
    let cell = PLATFORM_OVERRIDE.get_or_init(|| RwLock::new(None));
    *cell.write().expect("platform override poisoned") = Some(platform);
}

#[cfg(test)]
pub fn reset_platform_for_test() {
    if let Some(cell) = PLATFORM_OVERRIDE.get() {
        *cell.write().expect("platform override poisoned") = None;
    }
}
