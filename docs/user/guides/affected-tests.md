# CI 中受影响的测试

仅运行更改实际涉及的测试。

`rustcodegraph affected` 传递性地跟踪导入依赖项，以查找哪些测试文件受到一组已更改源文件的影响 - 因此 CI 只能运行相关测试。

```bash
rustcodegraph affected app/utils.py app/api.py             # pass files as arguments
git diff --name-only | rustcodegraph affected --stdin      # pipe from git diff
rustcodegraph affected app/auth.py --filter "tests/e2e/*"  # custom test-file pattern
```

## 选项

| 选项 | 描述 | 默认 |
|---|---|---|
| `--stdin` | 从 stdin 读取文件列表 | `false` |
| `-d, --depth <n>` | 最大依赖遍历深度 | `5` |
| `-f, --filter <glob>` | 自定义 glob 来识别测试文件 | 自动检测 |
| `-j, --json` | 输出为 JSON | `false` |
| `-q, --quiet` | 仅输出文件路径 | `false` |

## CI/钩子示例

```bash
#!/usr/bin/env bash
AFFECTED=$(git diff --name-only HEAD | rustcodegraph affected --stdin --quiet)
if [ -n "$AFFECTED" ]; then
  npx vitest run $AFFECTED
fi
```
