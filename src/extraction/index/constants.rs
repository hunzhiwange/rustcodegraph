// 这些阈值约束索引器的“默认安全边界”：单文件过大时跳过，IO
// 分批避免一次性占满文件句柄，嵌入式仓库探测则限制深度和目录数量。
pub(super) const FILE_IO_BATCH_SIZE: usize = 10;
pub(super) const PARSE_TIMEOUT_MS: u64 = 10_000;
pub(super) const WORKER_RECYCLE_INTERVAL: usize = 250;
pub(super) const MAX_FILE_SIZE: u64 = 1024 * 1024;
pub(super) const EMBEDDED_REPO_SEARCH_DEPTH: usize = 4;
pub(super) const EMBEDDED_REPO_SEARCH_ENTRIES: usize = 2000;

// 默认忽略列表偏向“构建产物、依赖目录和工具缓存”，让索引器聚焦源文件，
// 也避免在 monorepo 中误扫几万级别的生成文件。
pub(super) const DEFAULT_IGNORE_DIRS: &[&str] = &[
    "node_modules",
    "bower_components",
    "jspm_packages",
    "web_modules",
    ".git",
    ".yarn",
    ".pnpm-store",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".vite",
    ".parcel-cache",
    ".angular",
    ".docusaurus",
    "storybook-static",
    ".vinxi",
    ".nitro",
    "out-tsc",
    ".vercel",
    ".netlify",
    ".wrangler",
    "dist",
    "build",
    "out",
    ".output",
    "coverage",
    ".nyc_output",
    "__pycache__",
    "__pypackages__",
    ".venv",
    "venv",
    ".pixi",
    ".pdm-build",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".nox",
    ".hypothesis",
    ".ipynb_checkpoints",
    ".eggs",
    "target",
    ".gradle",
    "obj",
    "vendor",
    ".build",
    "Pods",
    "Carthage",
    "DerivedData",
    ".swiftpm",
    ".dart_tool",
    ".pub-cache",
    ".cxx",
    ".externalNativeBuild",
    "vcpkg_installed",
    ".bloop",
    ".metals",
    "lua_modules",
    ".luarocks",
    "__history",
    "__recovery",
    ".cache",
];

// 这里保留少量需要通配的目录形态；更完整的项目级规则会从 .gitignore 合并进来。
pub(super) const DEFAULT_IGNORE_PATTERNS: &[&str] = &["*.egg-info/", "cmake-build-*/", "bazel-*/"];
