//! Generated-file detection for symbol-disambiguation down-ranking.
//!
//! This is the Rust counterpart to `generated-detection.ts`. It is deliberately
//! path-only: generated files stay in the graph, but callers can rank them
//! behind hand-written implementations with the same name.
//!
//! 这里不读取文件内容，避免搜索/排序路径触发额外 I/O；判断结果只作为
//! 排序降权信号，不影响文件是否被索引。

fn basename(file_path: &str) -> &str {
    file_path
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(file_path)
}

fn has_any_suffix(file_path: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| file_path.ends_with(suffix))
}

/// Whether `file_path` looks like a tool-generated source file based on its
/// filename. This preserves the TS suffix heuristics without reading content.
pub fn is_generated_file(file_path: &str) -> bool {
    let name = basename(file_path);

    // 后缀表覆盖常见 protobuf、mock、minified 和语言特定生成物。
    // 新规则应尽量保持“明显生成”的保守边界，避免误降权用户手写源码。
    if has_any_suffix(
        file_path,
        &[
            ".pb.go",
            ".pulsar.go",
            "_grpc.pb.go",
            "_mock.go",
            "_mocks.go",
            ".generated.ts",
            ".generated.tsx",
            ".generated.js",
            ".generated.jsx",
            ".gen.ts",
            ".gen.tsx",
            ".gen.js",
            ".gen.jsx",
            ".pb.ts",
            ".pb.js",
            "_pb.ts",
            "_pb.js",
            "_grpc_pb.ts",
            "_grpc_pb.js",
            ".min.js",
            ".min.mjs",
            "_pb2.py",
            "_pb2_grpc.py",
            "_pb2.pyi",
            ".pb.cc",
            ".pb.h",
            ".g.cs",
            "Grpc.cs",
            "OuterClass.java",
            "Grpc.java",
            ".pb.swift",
            ".g.dart",
            ".freezed.dart",
            ".pb.dart",
            ".pbgrpc.dart",
            ".chopper.dart",
            ".generated.rs",
        ],
    ) {
        return true;
    }

    name.starts_with("mock_") && name.ends_with(".go")
}
