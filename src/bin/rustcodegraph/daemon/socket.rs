//! daemon transport 的跨平台适配层。
//!
//! Unix 使用 Unix domain socket；Windows 复用稳定映射的 loopback TCP 端口来承载
//! 同一套 daemon/proxy 协议，同时继续对外暴露命名管道风格的标识路径。

use std::io::{self, Read};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[cfg(windows)]
use rustcodegraph::mcp::daemon_paths::daemon_loopback_addr;

#[cfg(unix)]
pub(super) type LocalStream = std::os::unix::net::UnixStream;
#[cfg(unix)]
pub(super) type LocalListener = std::os::unix::net::UnixListener;

#[cfg(unix)]
pub(super) fn detach_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe extern "C" {
        fn setsid() -> i32;
    }
    // pre_exec 只能调用 async-signal-safe 的小块逻辑；这里仅创建新 session，
    // 让 daemon 脱离启动它的 MCP stdio 进程组。
    unsafe {
        command.pre_exec(|| {
            if setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
pub(super) type LocalStream = std::net::TcpStream;
#[cfg(windows)]
pub(super) type LocalListener = std::net::TcpListener;

#[cfg(windows)]
pub(super) fn detach_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    // Windows 没有 setsid；这两个 flag 提供相同的“不要跟随父控制台退出”语义。
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(all(not(unix), not(windows)))]
pub(super) type LocalStream = std::net::TcpStream;
#[cfg(all(not(unix), not(windows)))]
pub(super) type LocalListener = std::net::TcpListener;

#[cfg(all(not(unix), not(windows)))]
pub(super) fn detach_command(_command: &mut Command) {}

#[cfg(unix)]
pub(super) fn daemon_socket_may_exist(socket_path: &Path) -> bool {
    socket_path.exists()
}

#[cfg(windows)]
pub(super) fn daemon_socket_may_exist(_socket_path: &Path) -> bool {
    // Windows 使用稳定 loopback 端口；没有 pid/socket 文件可探测时，直接尝试连接即可。
    true
}

#[cfg(all(not(unix), not(windows)))]
pub(super) fn daemon_socket_may_exist(_socket_path: &Path) -> bool {
    false
}

#[cfg(unix)]
pub(super) fn connect_local_stream(socket_path: &Path) -> io::Result<LocalStream> {
    LocalStream::connect(socket_path)
}

#[cfg(windows)]
pub(super) fn connect_local_stream(socket_path: &Path) -> io::Result<LocalStream> {
    LocalStream::connect(daemon_loopback_addr(socket_path))
}

#[cfg(all(not(unix), not(windows)))]
pub(super) fn connect_local_stream(_socket_path: &Path) -> io::Result<LocalStream> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "local daemon sockets are not implemented on this platform",
    ))
}

pub(super) fn connection_failure_is_unavailable(err: &io::Error) -> bool {
    // 这些错误都表示“共享 daemon 当前不可用”，代理可以启动或退回进程内 session。
    matches!(
        err.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::Unsupported
    )
}

pub(super) fn read_limited_line(
    stream: &mut LocalStream,
    max_bytes: usize,
    timeout: Duration,
) -> Result<Option<String>, String> {
    // daemon hello 是一行 JSON；限制长度和读取时间，防止半开 socket 卡住 CLI 启动。
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("failed to set daemon hello timeout: {err}"))?;

    let mut line = Vec::new();
    let result = loop {
        let mut byte = [0u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => break Ok(None),
            Ok(_) => {
                if byte[0] == b'\n' {
                    break String::from_utf8(line)
                        .map(|line| Some(line.trim_end_matches('\r').to_string()))
                        .map_err(|err| format!("daemon hello was not valid UTF-8: {err}"));
                }
                line.push(byte[0]);
                if line.len() >= max_bytes {
                    break Err(format!("daemon hello exceeded {max_bytes} bytes"));
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                break Ok(None);
            }
            Err(err) => break Err(format!("failed to read daemon hello: {err}")),
        }
    };

    let _ = stream.set_read_timeout(None);
    result
}
