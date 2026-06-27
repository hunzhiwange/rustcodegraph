# 分发：原生 Rust 打包产物

RustCodeGraph 发布的是原生 Rust `rustcodegraph` 二进制。独立安装脚本
和 npm 安装器暴露的都是这个完全相同的公共命令；编译后的 TypeScript
运行时文件已经不再属于对外发布的运行时内容。npm 包现在是一个
cargo-dist 安装器包，会在安装时从 GitHub Releases 下载匹配的 Rust 制品。

## Bundle 中包含什么

由 CI 中的 cargo-dist 构建：

```
rustcodegraph-<triple>/
  rustcodegraph | rustcodegraph.exe
  README.md
  LICENSE
```

GitHub Release workflow 会发布 cargo-dist 生成的原生 Rust 制品，例如
`rustcodegraph-x86_64-unknown-linux-gnu.tar.xz`。cargo-dist 同时也会根据这些制品
生成 Homebrew formula 和 `rustcodegraph` npm 安装器包。

## 安装渠道

1. **`curl | sh`**（[`install.sh`](install.sh)）会检测 OS/架构，从 GitHub Releases 下载匹配的 Rust 压缩包，并把 `rustcodegraph` 链接到 PATH。
2. **npm**（`rustcodegraph`）会安装一个很小的启动器，它会下载匹配的原生 CLI。
3. **Homebrew**（`hunzhiwange/tap/rustcodegraph`）安装由 cargo-dist 生成的 formula。
4. **Windows**（[`install.ps1`](install.ps1)）会下载匹配的 `.zip`，把 `rustcodegraph.exe` 放到 `current\bin` 下，并把该目录加入 PATH。

## 发布流水线

[`.github/workflows/release.yml`](.github/workflows/release.yml) 让 cargo-dist
负责构建并托管 GitHub Release 制品。workflow 会用 Rust 侧的
`rustcodegraph extract-release-notes` 命令从 `CHANGELOG.md` 中提取 GitHub Release
说明，然后发布 cargo-dist 生成的 Homebrew formula 和 npm 安装器包。
如果要在本地做一次打包 dry run，请运行：

```bash
dist build --artifacts=global
```

发布动作由 workflow 统一负责；不要在本地任务里手动执行 `npm publish`、`git push` 或创建 tag。

仍待完成：
- macOS Gatekeeper 和 Windows Authenticode 的代码签名。
- 指向 Release 压缩包的 Scoop 包。
