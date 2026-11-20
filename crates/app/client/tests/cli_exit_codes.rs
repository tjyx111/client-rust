//! 验证 CLI 参数/加载失败的退出码；不注册驱动。

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_simple-vpn-client");

// 参数缺失或相对路径 -> 2。
#[test]
fn usage_errors_exit_2() {
    let no_args = Command::new(BIN).output().expect("运行 CLI");
    assert_eq!(no_args.status.code(), Some(2));
    assert!(!no_args.stderr.is_empty());

    let relative = Command::new(BIN)
        .args(["nfapi.dll", "netfilter2.sys"])
        .output()
        .expect("运行 CLI");
    assert_eq!(relative.status.code(), Some(2));
}

// 绝对路径但 DLL 不存在 -> 1，不触碰驱动。
#[test]
fn missing_dll_exits_1() {
    let root = std::env::var_os("SystemRoot").expect("SystemRoot 必须存在");
    let sys = std::path::Path::new(&root).join("System32/drivers/tcpip.sys");
    let output = Command::new(BIN)
        .arg(r"C:\simple-vpn-missing-4f3c\nfapi.dll")
        .arg(sys)
        .output()
        .expect("运行 CLI");
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.is_empty());
}
