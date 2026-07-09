# 疑难解答

修复了最常见的RustCodeGraph问题。

## "RustCodeGraph未初始化"

首先在项目目录中运行`rustcodegraph init -i`。

## 高级索引

检查是否排除了`node_modules`和其他大型目录（如果已gitignored ，则排除）。使用`--quiet`减少输出开销。

## MCP命中`database is locked`

当前版本不应该： Rust运行时在WAL模式下使用SQLite ，其中并发读取通常不会阻止writer。如果您仍然看到此信息，请按以下步骤操作：

- * *您仍在运行旧的CodeGraph包或二进制文件。* * RustCodeGraph是一个单独的项目，不会升级CodeGraph。单独安装RustCodeGraph — `curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh` （ macOS/Linux ）、`irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex` （ Windows ）或`npm i -g rustcodegraph` —然后确保您的MCP配置指向`rustcodegraph`。
- * * `rustcodegraph status`显示`wal`以外的`Journal:` * * —无法在此文件系统上启用WAL （在网络共享和WSL2 `/mnt`上常见） ，因此读取可以阻止写入。将项目（及其`.rustcodegraph/`文件夹）移动到本地磁盘上。

## MCP服务器未连接

确保项目已初始化/索引，验证MCP配置中的路径，并检查`rustcodegraph serve --mcp`是否从命令行工作。

## 缺少符号

MCP服务器在保存时自动同步（等待几秒钟）。如果需要，手动运行`rustcodegraph sync`。检查文件语言是否[受支持](./reference/languages.md)，并且未被`.gitignore`排除。
