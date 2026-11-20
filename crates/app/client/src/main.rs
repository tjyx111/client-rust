//! 驱动生命周期入口。
//!
//! 第 1 轮只校验两个绝对路径：`nfapi.dll` 和源 `netfilter2.sys`。
//! 后续轮次加载 DLL、复制驱动、注册驱动并调用 `nf_init`。

use std::env;
use std::ffi::{CStr, c_char, c_void};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sha2::{Digest, Sha256};
use simple_vpn_native_library::NativeLibrary;

const USAGE: &str = "用法: simple-vpn-client <nfapi.dll 绝对路径> <netfilter2.sys 绝对路径>";

fn main() -> ExitCode {
    // 跳过程序名，只收集后面的参数。
    let arguments: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    // 参数校验失败时打印原因和用法，返回退出码 2。
    let (nfapi_dll, driver_sys) = match validate_arguments(&arguments) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    // 加载 nfapi.dll；失败返回退出码 1。
    let library = match NativeLibrary::load_exact(&nfapi_dll) {
        Ok(library) => library,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    // 解析 5 个导出；失败返回退出码 1。
    let functions = match load_functions(&library) {
        Ok(functions) => functions,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    // 元组顺序：0=nf_registerDriver，4=nf_unRegisterDriver。
    let destination = match driver_destination() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let copied = match copy_driver("netfilter2", &driver_sys, &destination) {
        Ok(copied) => copied,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    // 注册驱动；失败时删除本次复制的文件。
    if let Err(error) = register_driver(functions.0, c"netfilter2") {
        if let Err(cleanup) = rollback_after_register_failure(&destination, copied) {
            eprintln!("注册失败后的文件清理失败: {cleanup}");
        }
        shutdown_checklist(&driver_sys, &destination, copied);
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    // 等待服务进入 RUNNING；失败时停止并注销。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    if let Err(error) = wait_until_running("netfilter2", deadline) {
        match cleanup_driver(&destination, copied, functions.4, functions.3, false) {
            Ok(()) => eprintln!("{error}"),
            Err(cleanup) => eprintln!("{error}; 清理失败: {cleanup}"),
        }
        shutdown_checklist(&driver_sys, &destination, copied);
        return ExitCode::from(1);
    }
    // 构造并注册 16+2 无逻辑 callback。
    let mut event_handler = Box::new(noop_event_handler());
    let mut ip_event_handler = Box::new(noop_ip_event_handler());
    let event_ptr = std::ptr::from_mut(event_handler.as_mut()).cast::<c_void>();
    if let Err(error) = init_driver(functions.1, c"netfilter2", event_ptr) {
        match cleanup_after_init_failure(
            stop_netfilter2,
            &destination,
            copied,
            functions.4,
            functions.3,
        ) {
            Ok(()) => eprintln!("{error}"),
            Err(cleanup) => eprintln!("{error}; 清理失败: {cleanup}"),
        }
        shutdown_checklist(&driver_sys, &destination, copied);
        return ExitCode::from(1);
    }
    let initialized = true;
    let ip_ptr = std::ptr::from_mut(ip_event_handler.as_mut()).cast::<c_void>();
    unsafe {
        // 安全前提：ip_ptr 指向 Box 持有的稳定 NfIpEventHandler。
        (functions.2)(ip_ptr);
    }
    startup_checklist(&driver_sys, &destination, &nfapi_dll);
    if let Err(error) = wait_for_enter() {
        match cleanup_driver(&destination, copied, functions.4, functions.3, initialized) {
            Ok(()) => eprintln!("{error}"),
            Err(cleanup) => eprintln!("{error}; 清理失败: {cleanup}"),
        }
        // 注销完成后再释放 DLL。
        drop(library);
        shutdown_checklist(&driver_sys, &destination, copied);
        return ExitCode::from(1);
    }
    // 停止服务、注销驱动、删除本次复制的文件，再释放 DLL。
    let cleanup_result =
        cleanup_driver(&destination, copied, functions.4, functions.3, initialized);
    drop(library);
    shutdown_checklist(&driver_sys, &destination, copied);
    match cleanup_result {
        Ok(()) => {
            println!("驱动已卸载");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("清理失败: {error}");
            ExitCode::from(1)
        }
    }
}

// 服务运行后等待 Enter；错误返回给 main 触发清理。
fn wait_for_enter() -> Result<(), String> {
    println!("驱动已加载，按 Enter 卸载");
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(_) => Ok(()),
        Err(error) => Err(format!("读取输入失败: {error}")),
    }
}

// 校验恰好两个绝对路径，源 `.sys` 必须存在且是文件。
fn validate_arguments(arguments: &[PathBuf]) -> Result<(PathBuf, PathBuf), String> {
    if arguments.len() != 2 {
        return Err("需要恰好两个参数".to_owned());
    }
    let nfapi_dll = arguments[0].clone();
    if !nfapi_dll.is_absolute() {
        return Err(format!("nfapi.dll 不是绝对路径: {}", nfapi_dll.display()));
    }
    let driver_sys = arguments[1].clone();
    if !driver_sys.is_absolute() {
        return Err(format!(
            "netfilter2.sys 不是绝对路径: {}",
            driver_sys.display()
        ));
    }
    if !driver_sys.is_file() {
        return Err(format!(
            "netfilter2.sys 不存在或不是文件: {}",
            driver_sys.display()
        ));
    }
    Ok((nfapi_dll, driver_sys))
}

// Win32 服务控制 API；本轮只查询，不启动、停止或删除服务。
const SC_MANAGER_CONNECT: u32 = 0x0001;
const SERVICE_QUERY_STATUS: u32 = 0x0004;
const SERVICE_QUERY_CONFIG: u32 = 0x0001;
const MAX_SERVICE_CONFIG_BYTES: u32 = 64 * 1024;
const SC_MANAGER_ENUMERATE_SERVICE: u32 = 0x0004;
const SERVICE_DRIVER: u32 = 0x000B;
const SERVICE_STATE_ALL: u32 = 0x0003;
const ERROR_MORE_DATA: i32 = 234;
const MAX_ENUM_BYTES: u32 = 1024 * 1024;
const SERVICE_STOP: u32 = 0x0020;
const SERVICE_CONTROL_STOP: u32 = 0x0000_0001;
const SERVICE_STOPPED: u32 = 0x0000_0001;
const SERVICE_RUNNING: u32 = 0x0000_0004;
const SERVICE_SYSTEM_START: u32 = 0x0000_0001;
const ERROR_SERVICE_NOT_ACTIVE: i32 = 1062;
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;

// SERVICE_STATUS 是 Win32 ABI 结构；字段必须完整保留。
#[repr(C)]
#[derive(Default)]
#[allow(dead_code)]
struct ServiceStatus {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
}

// QUERY_SERVICE_CONFIGW 是 Win32 ABI 结构；字段必须完整保留。
#[repr(C)]
#[allow(dead_code)]
struct QueryServiceConfig {
    service_type: u32,
    start_type: u32,
    error_control: u32,
    binary_path_name: *mut u16,
    load_order_group: *mut u16,
    tag_id: u32,
    dependencies: *mut u16,
    service_start_name: *mut u16,
    display_name: *mut u16,
}

// 编译期确认 x64 ABI 大小。
const _: [(); 64] = [(); std::mem::size_of::<QueryServiceConfig>()];

#[repr(C)]
#[allow(dead_code)]
struct ServiceStatusProcess {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
    process_id: u32,
    service_flags: u32,
}

#[repr(C)]
#[allow(dead_code)]
struct EnumServiceStatusProcess {
    service_name: *mut u16,
    display_name: *mut u16,
    status: ServiceStatusProcess,
}

// 编译期确认 x64 ABI 大小。
const _: [(); 56] = [(); std::mem::size_of::<EnumServiceStatusProcess>()];

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenSCManagerW(machine: *const u16, database: *const u16, access: u32) -> *mut c_void;
    fn OpenServiceW(manager: *mut c_void, name: *const u16, access: u32) -> *mut c_void;
    fn QueryServiceStatus(service: *mut c_void, status: *mut ServiceStatus) -> i32;
    fn QueryServiceConfigW(
        service: *mut c_void,
        config: *mut QueryServiceConfig,
        buffer_size: u32,
        bytes_needed: *mut u32,
    ) -> i32;
    fn EnumServicesStatusExW(
        manager: *mut c_void,
        info_level: i32,
        service_type: u32,
        service_state: u32,
        services: *mut u8,
        buffer_size: u32,
        bytes_needed: *mut u32,
        services_returned: *mut u32,
        resume_handle: *mut u32,
        group_name: *const u16,
    ) -> i32;
    fn ControlService(service: *mut c_void, control: u32, status: *mut ServiceStatus) -> i32;
    fn CloseServiceHandle(handle: *mut c_void) -> i32;
}

// 关闭服务句柄；失败只记录，不覆盖主流程错误。
fn close_handle(handle: *mut c_void) {
    if unsafe { CloseServiceHandle(handle) } == 0 {
        eprintln!("关闭服务句柄失败: {}", std::io::Error::last_os_error());
    }
}

// 查询服务状态码；服务不存在或无权访问时返回错误字符串。
fn query_service_state(name: &str) -> Result<u32, String> {
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);
    let manager = unsafe {
        // 安全前提：机器名和数据库名为空，access 为合法常量。
        OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT)
    };
    if manager.is_null() {
        let error = std::io::Error::last_os_error();
        return Err(format!("打开 SCM 失败: {error}"));
    }
    let service = unsafe {
        // 安全前提：manager 有效，wide 以 0 结尾。
        OpenServiceW(manager, wide.as_ptr(), SERVICE_QUERY_STATUS)
    };
    if service.is_null() {
        let error = std::io::Error::last_os_error();
        close_handle(manager);
        return Err(format!("打开服务 {name} 失败: {error}"));
    }
    let mut status = ServiceStatus::default();
    let ok = unsafe {
        // 安全前提：service 有效，status 指针有效。
        QueryServiceStatus(service, &raw mut status)
    };
    let error = std::io::Error::last_os_error();
    close_handle(service);
    close_handle(manager);
    if ok == 0 {
        return Err(format!("查询服务 {name} 失败: {error}"));
    }
    Ok(status.current_state)
}

// 查询服务是否已注册；不存在返回 false，其他错误返回 Err。
fn service_exists(name: &str) -> Result<bool, String> {
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);
    let manager = unsafe {
        // 安全前提：机器名和数据库名为空，access 为合法常量。
        OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT)
    };
    if manager.is_null() {
        let error = std::io::Error::last_os_error();
        return Err(format!("打开 SCM 失败: {error}"));
    }
    let service = unsafe {
        // 安全前提：manager 有效，wide 以 0 结尾。
        OpenServiceW(manager, wide.as_ptr(), SERVICE_QUERY_STATUS)
    };
    if service.is_null() {
        let error = std::io::Error::last_os_error();
        close_handle(manager);
        if error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) {
            return Ok(false);
        }
        return Err(format!("打开服务 {name} 失败: {error}"));
    }
    close_handle(service);
    close_handle(manager);
    Ok(true)
}

// 读取服务配置，返回启动类型和 ImagePath。
fn query_service_config(name: &str) -> Result<(u32, String), String> {
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);
    let manager = unsafe {
        // 安全前提：机器名和数据库名为空，access 为合法常量。
        OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT)
    };
    if manager.is_null() {
        let error = std::io::Error::last_os_error();
        return Err(format!("打开 SCM 失败: {error}"));
    }
    let service = unsafe {
        // 安全前提：manager 有效，wide 以 0 结尾。
        OpenServiceW(manager, wide.as_ptr(), SERVICE_QUERY_CONFIG)
    };
    if service.is_null() {
        let error = std::io::Error::last_os_error();
        close_handle(manager);
        return Err(format!("打开服务 {name} 失败: {error}"));
    }
    let mut needed: u32 = 0;
    unsafe {
        // 第一次调用只取所需缓冲区大小，预期返回失败并写入 needed。
        QueryServiceConfigW(service, std::ptr::null_mut(), 0, &raw mut needed);
    }
    if needed == 0 || needed > MAX_SERVICE_CONFIG_BYTES {
        close_handle(service);
        close_handle(manager);
        return Err(format!("查询服务 {name} 配置大小无效: {needed}"));
    }
    let mut buffer: Vec<u64> = vec![0; (needed as usize).div_ceil(8)];
    let config = buffer.as_mut_ptr().cast::<QueryServiceConfig>();
    let ok = unsafe {
        // 安全前提：service 有效，config 指向足够大且 8 字节对齐的缓冲区。
        QueryServiceConfigW(service, config, needed, &raw mut needed)
    };
    let error = std::io::Error::last_os_error();
    if ok == 0 {
        close_handle(service);
        close_handle(manager);
        return Err(format!("查询服务 {name} 配置失败: {error}"));
    }
    let start_type = unsafe { (*config).start_type };
    let path_ptr = unsafe { (*config).binary_path_name };
    let mut path = String::new();
    if !path_ptr.is_null() {
        let max_units = (needed as usize) / 2;
        let mut length = 0;
        while length < max_units && unsafe { *path_ptr.add(length) } != 0 {
            length += 1;
        }
        let slice = unsafe { std::slice::from_raw_parts(path_ptr, length) };
        path = String::from_utf16_lossy(slice);
    }
    close_handle(service);
    close_handle(manager);
    Ok((start_type, path))
}

// 读取文件并计算 SHA-256，返回 64 位小写十六进制字符串。
fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return Err(format!("读取文件失败: {}: {error}", path.display())),
    };
    let digest = Sha256::digest(&bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let part = format!("{byte:02x}");
        hex.push_str(&part);
    }
    Ok(hex)
}

// 把源 `.sys` 复制到目标；内容相同则跳过，返回是否本次复制。
// 冲突检测后复制驱动；服务已存在或目标哈希不同则拒绝。
// 项目门禁要求显式 match，不使用 `?`。
#[allow(clippy::question_mark)]
fn copy_driver(service_name: &str, source: &Path, destination: &Path) -> Result<bool, String> {
    let exists = match service_exists(service_name) {
        Ok(exists) => exists,
        Err(error) => return Err(error),
    };
    if exists {
        return Err(format!("服务 {service_name} 已存在，拒绝覆盖驱动"));
    }
    let source_hash = match sha256_file(source) {
        Ok(hash) => hash,
        Err(error) => return Err(error),
    };
    let destination_exists = match std::fs::metadata(destination) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!(
                "读取目标驱动信息失败: {}: {error}",
                destination.display()
            ));
        }
    };
    if destination_exists {
        let destination_hash = match sha256_file(destination) {
            Ok(hash) => hash,
            Err(error) => return Err(error),
        };
        if source_hash == destination_hash {
            return Ok(false);
        }
        return Err(format!(
            "目标驱动已存在且 SHA-256 不同，拒绝覆盖: {}",
            destination.display()
        ));
    }
    copy_file(source, destination)
}

// 执行实际复制；失败时清理本次新建的部分文件。
fn copy_file(source: &Path, destination: &Path) -> Result<bool, String> {
    match std::fs::copy(source, destination) {
        Ok(_) => Ok(true),
        Err(error) => {
            let mut message = format!(
                "复制驱动失败: {} -> {}: {error}",
                source.display(),
                destination.display()
            );
            match std::fs::remove_file(destination) {
                Ok(()) => {}
                Err(remove_error) if remove_error.kind() == std::io::ErrorKind::NotFound => {}
                Err(remove_error) => {
                    message = format!("{message}; 清理部分文件失败: {remove_error}");
                }
            }
            Err(message)
        }
    }
}

// 注册失败时只删除本次复制的文件；预存文件不动。
fn rollback_after_register_failure(destination: &Path, copied: bool) -> Result<(), String> {
    delete_driver(destination, copied)
}

// 只删除本次复制的驱动文件；目标已不存在视为成功。
fn delete_driver(destination: &Path, copied: bool) -> Result<(), String> {
    if !copied {
        return Ok(());
    }
    match std::fs::remove_file(destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "删除驱动文件失败: {}: {error}",
            destination.display()
        )),
    }
}

// 停止服务并等待 STOPPED；未安装返回错误，已停止视为成功。
fn stop_service(name: &str) -> Result<(), String> {
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);
    let manager = unsafe {
        // 安全前提：机器名和数据库名为空，access 为合法常量。
        OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT)
    };
    if manager.is_null() {
        let error = std::io::Error::last_os_error();
        return Err(format!("打开 SCM 失败: {error}"));
    }
    let service = unsafe {
        // 安全前提：manager 有效，wide 以 0 结尾。
        OpenServiceW(manager, wide.as_ptr(), SERVICE_STOP | SERVICE_QUERY_STATUS)
    };
    if service.is_null() {
        let error = std::io::Error::last_os_error();
        close_handle(manager);
        return Err(format!("打开服务 {name} 失败: {error}"));
    }
    let mut status = ServiceStatus::default();
    let controlled = unsafe {
        // 安全前提：service 有效，status 指针有效。
        ControlService(service, SERVICE_CONTROL_STOP, &raw mut status)
    };
    if controlled == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_SERVICE_NOT_ACTIVE) {
            close_handle(service);
            close_handle(manager);
            return Err(format!("停止服务 {name} 失败: {error}"));
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let result = loop {
        let queried = unsafe {
            // 安全前提：service 有效，status 指针有效。
            QueryServiceStatus(service, &raw mut status)
        };
        let error = std::io::Error::last_os_error();
        if queried == 0 {
            break Err(format!("查询服务 {name} 状态失败: {error}"));
        }
        if status.current_state == SERVICE_STOPPED {
            break Ok(());
        }
        if std::time::Instant::now() >= deadline {
            break Err(format!("等待服务 {name} 停止超时"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    close_handle(service);
    close_handle(manager);
    result
}

// 调用 nf_unRegisterDriver；非零状态返回错误字符串。
fn unregister_driver(unregister: UnregisterDriverFn, name: &CStr) -> Result<(), String> {
    let status = unsafe {
        // 安全前提：函数指针来自已加载的 nfapi.dll，name 以 0 结尾。
        unregister(name.as_ptr())
    };
    let error = std::io::Error::last_os_error();
    if status != 0 {
        return Err(format!(
            "nf_unRegisterDriver 失败: status={status}, error={error}"
        ));
    }
    Ok(())
}

fn driver_destination() -> Result<PathBuf, String> {
    let Some(system_root) = std::env::var_os("SystemRoot") else {
        return Err("环境变量 SystemRoot 不存在".to_owned());
    };
    Ok(Path::new(&system_root)
        .join("System32")
        .join("drivers")
        .join("netfilter2.sys"))
}

fn register_driver(register: RegisterDriverFn, name: &CStr) -> Result<(), String> {
    let status = unsafe {
        // 安全前提：函数指针来自已加载的 nfapi.dll，name 以 0 结尾。
        register(name.as_ptr())
    };
    let error = std::io::Error::last_os_error();
    if status != 0 {
        return Err(format!(
            "nf_registerDriver 失败: status={status}, error={error}"
        ));
    }
    Ok(())
}

fn init_driver(init: InitFn, name: &CStr, handler: *mut c_void) -> Result<(), String> {
    let status = unsafe {
        // 安全前提：init 来自已加载 nfapi.dll，handler 指向稳定的 callback 表。
        init(name.as_ptr(), handler)
    };
    let error = std::io::Error::last_os_error();
    if status != 0 {
        return Err(format!("nf_init 失败: status={status}, error={error}"));
    }
    Ok(())
}

// 项目门禁要求显式 match，不使用 `?`。
type ServiceStateFn = fn(&str) -> Result<u32, String>;

// 生产路径：使用真实 query_service_state。
fn wait_until_running(name: &str, deadline: std::time::Instant) -> Result<(), String> {
    wait_until_running_with(query_service_state, name, deadline)
}

// 可注入状态查询的等待实现；测试用 fake 覆盖迁移、超时和查询错误。
#[allow(clippy::question_mark)]
fn wait_until_running_with(
    state_fn: ServiceStateFn,
    name: &str,
    deadline: std::time::Instant,
) -> Result<(), String> {
    loop {
        let state = match state_fn(name) {
            Ok(state) => state,
            Err(error) => return Err(error),
        };
        if state == SERVICE_RUNNING {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("等待服务 {name} 启动超时"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// 停止服务、注销驱动、删除本次复制的文件；停止失败不注销，注销失败不删除。
type StopDriverFn = fn() -> Result<(), String>;

// 停止 netfilter2 服务；抽成函数便于测试注入。
fn stop_netfilter2() -> Result<(), String> {
    stop_service("netfilter2")
}

// 生产路径：使用真实 stop_netfilter2。
fn cleanup_driver(
    path: &Path,
    copied: bool,
    unregister: UnregisterDriverFn,
    free: FreeFn,
    initialized: bool,
) -> Result<(), String> {
    cleanup_driver_with(stop_netfilter2, path, copied, unregister, free, initialized)
}

// nf_init 失败时不做 nf_free，只停止/注销/删除。
fn cleanup_after_init_failure(
    stop: StopDriverFn,
    path: &Path,
    copied: bool,
    unregister: UnregisterDriverFn,
    free: FreeFn,
) -> Result<(), String> {
    cleanup_driver_with(stop, path, copied, unregister, free, false)
}

// 可注入停止/free 的清理实现；测试不得触碰真实服务。
fn cleanup_driver_with(
    stop: StopDriverFn,
    path: &Path,
    copied: bool,
    unregister: UnregisterDriverFn,
    free: FreeFn,
    initialized: bool,
) -> Result<(), String> {
    if initialized {
        unsafe {
            // 安全前提：free 来自已加载 nfapi.dll，且 nf_init 已成功。
            free();
        }
    }
    let mut failures: Vec<String> = Vec::new();
    match stop() {
        Err(error) => failures.push(error),
        Ok(()) => match unregister_driver(unregister, c"netfilter2") {
            Err(error) => failures.push(error),
            Ok(()) => {
                if let Err(error) = delete_driver(path, copied) {
                    failures.push(error);
                }
            }
        },
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

// 生成一行清单文本；只有 expected 与 actual 相等才是 OK。
fn checklist_text(phase: &str, item: &str, expected: &str, actual: &str) -> (String, bool) {
    let ok = expected == actual;
    let result = if ok { "OK" } else { "ERR" };
    (
        format!(
            "[DRIVER-CHECK] phase={phase} item={item} expected={expected} actual={actual} result={result}"
        ),
        ok,
    )
}

// Unix 秒时间戳；取不到时间时记录 ERR 文本。
fn unix_now() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(error) => format!("ERR:{error}"),
    }
}

// 追加一行到 logs/driver-checklist.log；失败只打印警告。
// 追加到指定日志；返回错误供测试断言。
fn append_checklist_log_to(path: &Path, line: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        return Err(format!("创建清单目录失败: {error}"));
    }
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) => return Err(format!("打开清单日志失败: {error}")),
    };
    match writeln!(file, "{line}") {
        Ok(()) => Ok(()),
        Err(error) => Err(format!("写入清单日志失败: {error}")),
    }
}

// 生产路径：固定 logs/driver-checklist.log；失败只告警。
fn append_checklist_log(line: &str) {
    match append_checklist_log_to(Path::new("logs/driver-checklist.log"), line) {
        Ok(()) => {}
        Err(error) => eprintln!("{error}"),
    }
}

// 输出并记录一行清单；ERR 不影响退出码。
fn checklist_line(phase: &str, item: &str, expected: &str, actual: &str) {
    let (line, _) = checklist_text(phase, item, expected, actual);
    println!("{line}");
    append_checklist_log(&line);
}

// 判断驱动名是否属于需要关注的相关网络过滤驱动。
fn is_related_driver(name: &str) -> bool {
    let lower = name.to_lowercase();
    for part in ["netfilter", "wfp", "accel", "vpn"] {
        if lower.contains(part) {
            return true;
        }
    }
    false
}

// 读取 UTF-16 指针并转 String；max_units 防止越界扫描。
fn wide_ptr_to_string(ptr: *const u16, max_units: usize) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut length = 0;
    while length < max_units && unsafe { *ptr.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, length) })
}

type RelatedDriver = (String, u32, String);
type RelatedDriverList = (Vec<RelatedDriver>, bool);

// 枚举相关内核驱动，返回 (name, state, ImagePath) 和 truncated。
fn enum_related_drivers() -> Result<RelatedDriverList, String> {
    let manager = unsafe {
        // 安全前提：机器名和数据库名为空，access 为合法常量。
        OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            SC_MANAGER_ENUMERATE_SERVICE,
        )
    };
    if manager.is_null() {
        let error = std::io::Error::last_os_error();
        return Err(format!("打开 SCM 失败: {error}"));
    }
    let mut needed: u32 = MAX_ENUM_BYTES;
    let mut returned: u32 = 0;
    let mut resume: u32 = 0;
    let mut buffer: Vec<u64> = vec![0; (MAX_ENUM_BYTES as usize).div_ceil(8)];
    let ok = unsafe {
        // 安全前提：manager 有效，buffer 为 1 MiB 且 8 字节对齐。
        EnumServicesStatusExW(
            manager,
            0,
            SERVICE_DRIVER,
            SERVICE_STATE_ALL,
            buffer.as_mut_ptr().cast::<u8>(),
            MAX_ENUM_BYTES,
            &raw mut needed,
            &raw mut returned,
            &raw mut resume,
            std::ptr::null(),
        )
    };
    let error = std::io::Error::last_os_error();
    let truncated_by_more_data = ok == 0 && error.raw_os_error() == Some(ERROR_MORE_DATA);
    if ok == 0 && !truncated_by_more_data {
        close_handle(manager);
        return Err(format!("枚举驱动失败: {error}"));
    }
    let mut drivers = Vec::new();
    let max_entries = (returned as usize).min(64);
    for index in 0..max_entries {
        let entry = unsafe {
            // 安全前提：index 在 returned 范围内，缓冲区包含该结构。
            &*(buffer
                .as_ptr()
                .cast::<EnumServiceStatusProcess>()
                .add(index))
        };
        let max_units = (needed.min(MAX_ENUM_BYTES) as usize) / 2;
        let name = wide_ptr_to_string(entry.service_name, max_units);
        if !is_related_driver(&name) || name.eq_ignore_ascii_case("netfilter2") {
            continue;
        }
        let path = match query_service_config(&name) {
            Ok((_, path)) => normalize_image_path(&path),
            Err(error) => format!("ERR:{error}"),
        };
        drivers.push((name, entry.status.current_state, path));
    }
    let truncated = truncated_by_more_data || (returned as usize) > 64;
    close_handle(manager);
    Ok((drivers, truncated))
}

// 规范化 ImagePath：去引号、转小写、展开 SystemRoot/System32 前缀。
fn normalize_image_path(raw: &str) -> String {
    let lower = raw.trim_matches('"').to_lowercase();
    let lower = match lower.strip_prefix("\\??\\") {
        Some(rest) => rest.to_owned(),
        None => lower,
    };
    let system_root = match std::env::var("SystemRoot") {
        Ok(value) => value.to_lowercase(),
        Err(_) => "c:\\windows".to_owned(),
    };
    if let Some(rest) = lower.strip_prefix("%systemroot%\\") {
        return format!("{system_root}\\{rest}");
    }
    if let Some(rest) = lower.strip_prefix("\\systemroot\\") {
        return format!("{system_root}\\{rest}");
    }
    if lower.starts_with("\\system32\\") {
        return format!("{system_root}{lower}");
    }
    if lower.starts_with("system32\\") {
        return format!("{system_root}\\{lower}");
    }
    lower
}

// 启动清单：服务状态、驱动文件 SHA-256、nfapi.dll 路径。
fn startup_checklist(source: &Path, destination: &Path, nfapi_dll: &Path) {
    let header = format!("[DRIVER-CHECK] phase=startup timestamp={}", unix_now());
    println!("{header}");
    append_checklist_log(&header);

    let service_actual = match query_service_state("netfilter2") {
        Ok(state) => state.to_string(),
        Err(error) => format!("ERR:{error}"),
    };
    checklist_line(
        "startup",
        "service_netfilter2",
        &SERVICE_RUNNING.to_string(),
        &service_actual,
    );

    let expected_start = SERVICE_SYSTEM_START.to_string();
    let expected_path = destination.display().to_string().to_lowercase();
    match query_service_config("netfilter2") {
        Ok((start_type, image_path)) => {
            checklist_line(
                "startup",
                "service_start_type",
                &expected_start,
                &start_type.to_string(),
            );
            checklist_line(
                "startup",
                "service_image_path",
                &expected_path,
                &normalize_image_path(&image_path),
            );
        }
        Err(error) => {
            let actual = format!("ERR:{error}");
            checklist_line("startup", "service_start_type", &expected_start, &actual);
            checklist_line("startup", "service_image_path", &expected_path, &actual);
        }
    }

    match enum_related_drivers() {
        Ok((drivers, truncated)) => {
            if drivers.is_empty() {
                checklist_line("startup", "related_drivers", "NONE", "NONE");
            } else {
                for (name, state, path) in drivers {
                    let actual = format!("state={state} path={path}");
                    checklist_line(
                        "startup",
                        &format!("related_driver_{name}"),
                        "ABSENT",
                        &actual,
                    );
                }
            }
            if truncated {
                checklist_line("startup", "related_drivers_truncated", "false", "true");
            }
        }
        Err(error) => {
            checklist_line(
                "startup",
                "related_drivers",
                "NONE",
                &format!("ERR:{error}"),
            );
        }
    }

    let source_hash = match sha256_file(source) {
        Ok(hash) => hash,
        Err(error) => format!("ERR:{error}"),
    };
    let destination_hash = match sha256_file(destination) {
        Ok(hash) => hash,
        Err(error) => format!("ERR:{error}"),
    };
    checklist_line("startup", "driver_sha256", &source_hash, &destination_hash);

    let expected_dll = nfapi_dll.display().to_string();
    let actual_dll = if nfapi_dll.is_file() {
        expected_dll.clone()
    } else {
        "MISSING".to_owned()
    };
    checklist_line("startup", "nfapi_path", &expected_dll, &actual_dll);
}

// 关闭时驱动文件的期望状态：本次复制过就应删除，否则应保留。
fn file_expectation(copied: bool) -> &'static str {
    if copied { "MISSING" } else { "PRESENT" }
}

// 关闭清单：服务是否残留、驱动文件是否按 copied 清理或保留。
fn shutdown_checklist(source: &Path, destination: &Path, copied: bool) {
    let header = format!("[DRIVER-CHECK] phase=shutdown timestamp={}", unix_now());
    println!("{header}");
    append_checklist_log(&header);

    let service_actual = match service_exists("netfilter2") {
        Ok(true) => "PRESENT".to_owned(),
        Ok(false) => "MISSING".to_owned(),
        Err(error) => format!("ERR:{error}"),
    };
    checklist_line("shutdown", "service_netfilter2", "MISSING", &service_actual);

    let actual_file = if destination.exists() {
        "PRESENT"
    } else {
        "MISSING"
    };
    checklist_line(
        "shutdown",
        "driver_file",
        file_expectation(copied),
        actual_file,
    );

    if !copied {
        let source_hash = match sha256_file(source) {
            Ok(hash) => hash,
            Err(error) => format!("ERR:{error}"),
        };
        let destination_hash = match sha256_file(destination) {
            Ok(hash) => hash,
            Err(error) => format!("ERR:{error}"),
        };
        checklist_line("shutdown", "driver_sha256", &source_hash, &destination_hash);
    }
}

// 本阶段只解析地址，不调用；callback 表在下一轮定义真实类型。
type RegisterDriverFn = unsafe extern "C" fn(*const c_char) -> i32;
type InitFn = unsafe extern "C" fn(*const c_char, *mut c_void) -> i32;
type SetIpEventHandlerFn = unsafe extern "C" fn(*mut c_void);
type FreeFn = unsafe extern "C" fn();
type UnregisterDriverFn = unsafe extern "C" fn(*const c_char) -> i32;
type NfApiFunctions = (
    RegisterDriverFn,
    InitFn,
    SetIpEventHandlerFn,
    FreeFn,
    UnregisterDriverFn,
);

type ThreadCallback = unsafe extern "C" fn();
type ConnCallback = unsafe extern "C" fn(u64, *mut c_void);
type BufferCallback = unsafe extern "C" fn(u64, *const c_char, i32);
type EndpointCallback = unsafe extern "C" fn(u64);
type UdpBufferCallback = unsafe extern "C" fn(u64, *const u8, *const c_char, i32, *mut c_void);
type IpCallback = unsafe extern "C" fn(*const c_char, i32, *mut c_void);

#[repr(C, packed)]
#[allow(dead_code)]
struct NfEventHandler {
    thread_start: Option<ThreadCallback>,
    thread_end: Option<ThreadCallback>,
    tcp_connect_request: Option<ConnCallback>,
    tcp_connected: Option<ConnCallback>,
    tcp_closed: Option<ConnCallback>,
    tcp_receive: Option<BufferCallback>,
    tcp_send: Option<BufferCallback>,
    tcp_can_receive: Option<EndpointCallback>,
    tcp_can_send: Option<EndpointCallback>,
    udp_created: Option<ConnCallback>,
    udp_connect_request: Option<ConnCallback>,
    udp_closed: Option<ConnCallback>,
    udp_receive: Option<UdpBufferCallback>,
    udp_send: Option<UdpBufferCallback>,
    udp_can_receive: Option<EndpointCallback>,
    udp_can_send: Option<EndpointCallback>,
}

#[repr(C, packed)]
#[allow(dead_code)]
struct NfIpEventHandler {
    ip_receive: Option<IpCallback>,
    ip_send: Option<IpCallback>,
}

const _: [(); 128] = [(); std::mem::size_of::<NfEventHandler>()];
const _: [(); 16] = [(); std::mem::size_of::<NfIpEventHandler>()];

unsafe extern "C" fn noop_thread() {}
unsafe extern "C" fn noop_conn(_: u64, _: *mut c_void) {}
unsafe extern "C" fn noop_buffer(_: u64, _: *const c_char, _: i32) {}
unsafe extern "C" fn noop_endpoint(_: u64) {}
unsafe extern "C" fn noop_udp_buffer(
    _: u64,
    _: *const u8,
    _: *const c_char,
    _: i32,
    _: *mut c_void,
) {
}
unsafe extern "C" fn noop_ip(_: *const c_char, _: i32, _: *mut c_void) {}

fn noop_event_handler() -> NfEventHandler {
    NfEventHandler {
        thread_start: Some(noop_thread),
        thread_end: Some(noop_thread),
        tcp_connect_request: Some(noop_conn),
        tcp_connected: Some(noop_conn),
        tcp_closed: Some(noop_conn),
        tcp_receive: Some(noop_buffer),
        tcp_send: Some(noop_buffer),
        tcp_can_receive: Some(noop_endpoint),
        tcp_can_send: Some(noop_endpoint),
        udp_created: Some(noop_conn),
        udp_connect_request: Some(noop_conn),
        udp_closed: Some(noop_conn),
        udp_receive: Some(noop_udp_buffer),
        udp_send: Some(noop_udp_buffer),
        udp_can_receive: Some(noop_endpoint),
        udp_can_send: Some(noop_endpoint),
    }
}

fn noop_ip_event_handler() -> NfIpEventHandler {
    NfIpEventHandler {
        ip_receive: Some(noop_ip),
        ip_send: Some(noop_ip),
    }
}

// 解析 5 个 NFAPI 导出；任一缺失立即返回错误。
// 项目门禁要求显式 match，不使用 `?`；这里局部允许 Clippy 的风格建议。
#[allow(clippy::question_mark)]
fn load_functions(library: &NativeLibrary) -> Result<NfApiFunctions, String> {
    let register: RegisterDriverFn = match unsafe { library.required_symbol(c"nf_registerDriver") }
    {
        Ok(symbol) => {
            // 安全前提：符号已确认存在，函数指针类型与 vendor 声明一致。
            unsafe { std::mem::transmute_copy(&symbol.as_ptr()) }
        }
        Err(error) => return Err(error),
    };
    let init: InitFn = match unsafe { library.required_symbol(c"nf_init") } {
        Ok(symbol) => {
            // 安全前提：符号已确认存在，函数指针类型与 vendor 声明一致。
            unsafe { std::mem::transmute_copy(&symbol.as_ptr()) }
        }
        Err(error) => return Err(error),
    };
    let set_ip: SetIpEventHandlerFn =
        match unsafe { library.required_symbol(c"nf_setIPEventHandler") } {
            Ok(symbol) => {
                // 安全前提：符号已确认存在，函数指针类型与 vendor 声明一致。
                unsafe { std::mem::transmute_copy(&symbol.as_ptr()) }
            }
            Err(error) => return Err(error),
        };
    let free: FreeFn = match unsafe { library.required_symbol(c"nf_free") } {
        Ok(symbol) => {
            // 安全前提：符号已确认存在，函数指针类型与 vendor 声明一致。
            unsafe { std::mem::transmute_copy(&symbol.as_ptr()) }
        }
        Err(error) => return Err(error),
    };
    let unregister: UnregisterDriverFn =
        match unsafe { library.required_symbol(c"nf_unRegisterDriver") } {
            Ok(symbol) => {
                // 安全前提：符号已确认存在，函数指针类型与 vendor 声明一致。
                unsafe { std::mem::transmute_copy(&symbol.as_ptr()) }
            }
            Err(error) => return Err(error),
        };
    Ok((register, init, set_ip, free, unregister))
}

#[cfg(test)]
mod tests {
    use super::{
        SERVICE_RUNNING, append_checklist_log_to, checklist_text, cleanup_after_init_failure,
        cleanup_driver_with, copy_driver, delete_driver, driver_destination, file_expectation,
        init_driver, is_related_driver, load_functions, normalize_image_path, query_service_config,
        query_service_state, register_driver, rollback_after_register_failure, service_exists,
        sha256_file, stop_service, unregister_driver, validate_arguments, wait_until_running,
        wait_until_running_with, wide_ptr_to_string,
    };
    use simple_vpn_native_library::NativeLibrary;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    // 把字符串切片转成路径列表，供测试复用。
    fn paths(values: &[&str]) -> Vec<PathBuf> {
        values.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(validate_arguments(&paths(&[])).is_err());
        assert!(validate_arguments(&paths(&[r"C:\a.dll"])).is_err());
        assert!(
            validate_arguments(&paths(&[
                "nfapi.dll",
                r"C:\Windows\System32\drivers\tcpip.sys",
            ]))
            .is_err()
        );
        assert!(validate_arguments(&paths(&[r"C:\a.dll", "netfilter2.sys"])).is_err());
        assert!(
            validate_arguments(&paths(&[r"C:\a.dll", r"C:\simple-vpn-missing-4f3c.sys",])).is_err()
        );
    }

    #[test]
    fn accepts_existing_driver_file() {
        assert!(
            validate_arguments(&paths(&[
                r"C:\a.dll",
                r"C:\Windows\System32\drivers\tcpip.sys",
            ]))
            .is_ok()
        );
    }

    #[test]
    fn reports_missing_nfapi_exports() {
        let root = std::env::var_os("SystemRoot").expect("SystemRoot 必须存在");
        let path = Path::new(&root).join("System32/winhttp.dll");
        let library = NativeLibrary::load_exact(&path).expect("winhttp.dll 必须能加载");
        assert!(load_functions(&library).is_err());
    }

    #[test]
    fn queries_service_state() {
        assert!(query_service_state("simple-vpn-missing-service").is_err());
        assert!(query_service_state("EventLog").is_ok());
    }

    #[test]
    fn detects_existing_service() {
        assert_eq!(service_exists("simple-vpn-missing-service"), Ok(false));
        assert_eq!(service_exists("EventLog"), Ok(true));
    }

    #[test]
    fn formats_checklist_result() {
        assert!(checklist_text("p", "i", "a", "a").1);
        assert!(checklist_text("p", "i", "a", "b").0.contains("ERR"));
        assert_eq!(file_expectation(true), "MISSING");
        assert_eq!(file_expectation(false), "PRESENT");
    }

    #[test]
    fn queries_service_config() {
        assert!(query_service_config("EventLog").is_ok());
        assert!(query_service_config("simple-vpn-missing-service").is_err());
    }

    #[test]
    fn filters_related_drivers() {
        assert!(is_related_driver("xlaccwfp"));
        assert!(is_related_driver("nfwfp"));
        assert!(is_related_driver("PgyVPNService"));
        assert!(!is_related_driver("EventLog"));
    }

    #[test]
    fn converts_wide_string() {
        let value = [u16::from(b'a'), u16::from(b'b'), 0];
        assert_eq!(wide_ptr_to_string(value.as_ptr(), value.len()), "ab");
    }

    #[test]
    fn normalizes_service_image_paths() {
        let root = std::env::var("SystemRoot").expect("SystemRoot 必须存在");
        let expected = format!("{}\\system32\\drivers\\netfilter2.sys", root.to_lowercase());
        assert_eq!(
            normalize_image_path("\\SystemRoot\\System32\\drivers\\netfilter2.sys"),
            expected
        );
        assert_eq!(
            normalize_image_path(&format!("\"{root}\\System32\\drivers\\netfilter2.sys\"")),
            expected
        );
        assert_eq!(
            normalize_image_path(&format!("\\??\\{root}\\System32\\drivers\\netfilter2.sys")),
            expected
        );
        assert_eq!(
            normalize_image_path("%SystemRoot%\\System32\\drivers\\netfilter2.sys"),
            expected
        );
    }

    #[test]
    fn computes_sha256() {
        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-sha-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        let path = directory.0.join("abc");
        std::fs::write(&path, b"abc").expect("写入测试文件");
        assert_eq!(
            sha256_file(&path),
            Ok("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned())
        );
    }

    // 测试用临时目录守卫；离开作用域时删除目录。
    struct TestDirectory(PathBuf);

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0) {
                eprintln!("清理测试目录失败: {error}");
            }
        }
    }

    #[test]
    fn copies_and_deletes_driver_file() {
        let path = std::env::temp_dir().join(format!("simple-vpn-driver-{}", std::process::id()));
        if path.exists() {
            std::fs::remove_dir_all(&path).expect("清理旧测试目录");
        }
        let directory = TestDirectory(path);
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        let source = directory.0.join("source.sys");
        let destination = directory.0.join("netfilter2.sys");
        std::fs::write(&source, b"driver").expect("写入源文件");

        assert_eq!(
            copy_driver("simple-vpn-missing-service", &source, &destination),
            Ok(true)
        );
        assert_eq!(
            copy_driver("simple-vpn-missing-service", &source, &destination),
            Ok(false)
        );

        std::fs::write(&destination, b"old").expect("写入旧目标");
        assert!(copy_driver("simple-vpn-missing-service", &source, &destination).is_err());
        assert_eq!(std::fs::read(&destination).expect("读取旧目标"), b"old");

        let missing_source = directory.0.join("missing.sys");
        let missing_destination = directory.0.join("missing-copy.sys");
        assert!(
            copy_driver(
                "simple-vpn-missing-service",
                &missing_source,
                &missing_destination
            )
            .is_err()
        );
        assert!(!missing_destination.exists());

        std::fs::write(&destination, b"keep").expect("写入保留目标");
        assert!(copy_driver("simple-vpn-missing-service", &missing_source, &destination).is_err());
        assert_eq!(std::fs::read(&destination).expect("读取保留目标"), b"keep");

        let missing_parent = directory.0.join("missing-dir").join("copy.sys");
        assert!(copy_driver("simple-vpn-missing-service", &source, &missing_parent).is_err());

        assert_eq!(delete_driver(&destination, false), Ok(()));
        assert!(destination.exists());
        assert_eq!(delete_driver(&destination, true), Ok(()));
        assert!(!destination.exists());
        assert_eq!(delete_driver(&destination, true), Ok(()));
    }

    #[test]
    fn refuses_existing_service() {
        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-conflict-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        let source = directory.0.join("source.sys");
        let destination = directory.0.join("netfilter2.sys");
        std::fs::write(&source, b"driver").expect("写入源文件");
        assert!(copy_driver("EventLog", &source, &destination).is_err());
        assert!(!destination.exists());
    }

    #[test]
    fn waits_for_running_transition() {
        static STATE: AtomicU32 = AtomicU32::new(0);
        #[allow(clippy::unnecessary_wraps)]
        fn transition(_: &str) -> Result<u32, String> {
            let current = STATE.fetch_add(1, Ordering::SeqCst);
            if current == 0 {
                Ok(0)
            } else {
                Ok(SERVICE_RUNNING)
            }
        }
        STATE.store(0, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        assert!(wait_until_running_with(transition, "netfilter2", deadline).is_ok());
    }

    #[test]
    fn reports_running_timeout_and_query_error() {
        #[allow(clippy::unnecessary_wraps)]
        fn never_running(_: &str) -> Result<u32, String> {
            Ok(0)
        }
        fn query_fails(_: &str) -> Result<u32, String> {
            Err("query failed".to_owned())
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1);
        assert!(wait_until_running_with(never_running, "netfilter2", deadline).is_err());
        let future = std::time::Instant::now() + std::time::Duration::from_secs(1);
        assert!(wait_until_running_with(query_fails, "netfilter2", future).is_err());
    }

    #[test]
    fn registration_failure_removes_copied_file() {
        unsafe extern "C" fn fail(_: *const std::ffi::c_char) -> i32 {
            -1
        }
        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-register-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        let path = directory.0.join("netfilter2.sys");
        std::fs::write(&path, b"driver").expect("写入文件");
        assert!(register_driver(fail, c"netfilter2").is_err());
        assert!(rollback_after_register_failure(&path, true).is_ok());
        assert!(!path.exists());
        std::fs::write(&path, b"keep").expect("写入预存文件");
        assert!(rollback_after_register_failure(&path, false).is_ok());
        assert!(path.exists());
    }

    #[test]
    fn checklist_log_failure_is_reported() {
        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-checklist-log-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        assert!(append_checklist_log_to(&directory.0, "line").is_err());
        let file = directory.0.join("driver-checklist.log");
        assert!(append_checklist_log_to(&file, "line").is_ok());
        assert!(
            std::fs::read_to_string(&file)
                .expect("读取清单日志")
                .contains("line")
        );
    }

    #[test]
    fn reports_delete_failure() {
        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-delete-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        assert!(delete_driver(&directory.0, true).is_err());
    }

    #[test]
    fn stops_missing_service() {
        assert!(stop_service("simple-vpn-missing-service").is_err());
    }

    #[test]
    fn register_and_unregister_status() {
        unsafe extern "C" fn ok(_: *const std::ffi::c_char) -> i32 {
            0
        }
        unsafe extern "C" fn fail(_: *const std::ffi::c_char) -> i32 {
            -1
        }
        assert!(register_driver(ok, c"netfilter2").is_ok());
        assert!(register_driver(fail, c"netfilter2").is_err());
        assert!(unregister_driver(ok, c"netfilter2").is_ok());
        assert!(unregister_driver(fail, c"netfilter2").is_err());
    }

    #[test]
    fn builds_driver_destination() {
        let destination = driver_destination().expect("SystemRoot 应存在");
        assert!(destination.ends_with("System32/drivers/netfilter2.sys"));
    }

    #[test]
    fn waits_for_missing_service_until_error() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1);
        assert!(wait_until_running("simple-vpn-missing-service", deadline).is_err());
    }

    #[test]
    fn cleanup_orders_stop_unregister_delete() {
        #[allow(clippy::unnecessary_wraps)]
        fn stop_ok() -> Result<(), String> {
            Ok(())
        }
        fn stop_fail() -> Result<(), String> {
            Err("stop failed".to_owned())
        }
        unsafe extern "C" fn unregister_ok(_: *const std::ffi::c_char) -> i32 {
            0
        }
        unsafe extern "C" fn unregister_fail(_: *const std::ffi::c_char) -> i32 {
            -1
        }
        unsafe extern "C" fn must_not_run(_: *const std::ffi::c_char) -> i32 {
            panic!("停止失败时不应注销");
        }
        unsafe extern "C" fn free_noop() {}

        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-cleanup-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        let path = directory.0.join("netfilter2.sys");
        std::fs::write(&path, b"driver").expect("写入文件");

        assert!(
            cleanup_driver_with(stop_fail, &path, true, must_not_run, free_noop, false).is_err()
        );
        assert!(path.exists());
        assert!(
            cleanup_driver_with(stop_ok, &path, true, unregister_fail, free_noop, false).is_err()
        );
        assert!(path.exists());
        assert!(cleanup_driver_with(stop_ok, &path, true, unregister_ok, free_noop, false).is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn free_runs_before_stop() {
        static ORDER: AtomicU32 = AtomicU32::new(0);
        unsafe extern "C" fn free_first() {
            assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 0);
        }
        #[allow(clippy::unnecessary_wraps)]
        fn stop_second() -> Result<(), String> {
            assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 1);
            Ok(())
        }
        unsafe extern "C" fn unregister_third(_: *const std::ffi::c_char) -> i32 {
            assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 2);
            0
        }

        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-free-order-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        let path = directory.0.join("netfilter2.sys");
        std::fs::write(&path, b"driver").expect("写入文件");
        ORDER.store(0, Ordering::SeqCst);
        assert!(
            cleanup_driver_with(stop_second, &path, true, unregister_third, free_first, true)
                .is_ok()
        );
        assert!(!path.exists());
    }

    #[test]
    fn init_failure_cleanup_skips_free() {
        static FREE_CALLED: AtomicBool = AtomicBool::new(false);
        unsafe extern "C" fn free_must_not_run() {
            FREE_CALLED.store(true, Ordering::SeqCst);
        }
        #[allow(clippy::unnecessary_wraps)]
        fn stop_ok() -> Result<(), String> {
            Ok(())
        }
        unsafe extern "C" fn unregister_ok(_: *const std::ffi::c_char) -> i32 {
            0
        }

        let directory = TestDirectory(
            std::env::temp_dir().join(format!("simple-vpn-init-fail-{}", std::process::id())),
        );
        std::fs::create_dir_all(&directory.0).expect("创建测试目录");
        let path = directory.0.join("netfilter2.sys");
        std::fs::write(&path, b"driver").expect("写入文件");
        FREE_CALLED.store(false, Ordering::SeqCst);
        assert!(
            cleanup_after_init_failure(stop_ok, &path, true, unregister_ok, free_must_not_run)
                .is_ok()
        );
        assert!(!FREE_CALLED.load(Ordering::SeqCst));
        assert!(!path.exists());
    }

    #[test]
    fn init_driver_status() {
        unsafe extern "C" fn ok(_: *const std::ffi::c_char, _: *mut std::ffi::c_void) -> i32 {
            0
        }
        unsafe extern "C" fn fail(_: *const std::ffi::c_char, _: *mut std::ffi::c_void) -> i32 {
            -1
        }
        assert!(init_driver(ok, c"netfilter2", std::ptr::null_mut()).is_ok());
        assert!(init_driver(fail, c"netfilter2", std::ptr::null_mut()).is_err());
    }
}
