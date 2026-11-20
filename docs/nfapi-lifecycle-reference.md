# xacc-route-sdk2 NFAPI 驱动加载与动态库执行逻辑

状态：**研究/设计记录，不是生产实现，也不授权真实驱动操作。**

来源固定：`xacc-route-sdk2` Git `8232fb5d2ae2b2940128b981a056271598bf9e26`。

取证说明：核对时该仓库工作区 HEAD 为 `46c75829ca3a03a92164dfcdf009c478245a060e`，且有 22 个未提交或未跟踪改动（包括 `crates/platform/windows/src/capture/nfapi/runtime.rs`、`examples/windows_nfapi_smoke.rs`、`src/config.rs`）。本文全部内容来自固定提交的 Git 对象，不采用当前脏工作区。

vendor 基线：`client-cpp/nfsdk-demo-1.7.6.4/nfsdk/wfp`，`nfapi.h` SHA-256 与 `fixtures/design-freeze/netfilter-abi-x64.json` 一致。本文同时记录 vendor 头文件声明和 `nfapi.dll` 实际反汇编结论。

## 1. 三层边界

xacc 的 NFAPI 相关实现分成三层，理解执行顺序时必须先分清：

| 层 | 代表类型/文件 | 职责 |
| --- | --- | --- |
| DLL owner | `NativeLibrary`，`crates/platform/windows/native-library/src/lib.rs` | 精确路径加载 `nfapi.dll`，持有唯一 module handle，`Drop` 时 `FreeLibrary` |
| NFAPI 函数表 | `DynamicNfApiBackend`，`crates/platform/windows/backends/nfapi/src/backend/dynamic.rs` | 解析导出、保存 typed function pointers、调用 `nf_registerDriver`/`nf_init`/`nf_free`/规则/post API |
| 生命周期组合 | `WindowsNfapiRuntime`，`crates/platform/windows/src/capture/nfapi/runtime.rs` | 组合 runtime、localserver、callback、驱动启停、规则和停止顺序 |

`NativeLibrary` 只负责“DLL 已映射且句柄有效”；`DynamicNfApiBackend` 只负责“函数表可调用”；`WindowsNfapiRuntime` 才负责“驱动已注册、过滤会话已初始化、规则已安装”。

## 2. 动态库导入

### 2.1 DLL 路径来源

xacc 的默认路径解析在 `crates/platform/windows/src/capture/nfapi/assets.rs`：

1. 默认相对路径是 `nfapi/Release_c_api/x64/nfapi.dll`（`default_nfapi_dll_path`，48–55 行）。
2. 先看可执行文件所在目录是否存在该相对路径。
3. 不存在时回退到 workspace 内 `backends/nfapi/assets/windows/netfilter/...` 的打包资产。
4. 两处都不存在时仍返回候选路径，让后续加载失败并保留明确错误。
5. 配置显式提供 `driver.dll` 时直接使用该路径。

xacc 允许自动发现和回退；本项目 W02-a 明确只接受调用方传入的绝对路径，不搜索目录、不尝试候选名。

### 2.2 `NativeLibrary::load_exact`

实现位于 `crates/platform/windows/native-library/src/lib.rs`：

1. `path.is_absolute()` 为假 → 返回 `PathNotAbsolute`，不搜索当前目录或 `PATH`。
2. `path.canonicalize()` → 失败返回 `Canonicalize { path, source }`。
3. `wide_null(canonical_path.as_os_str())` 生成以 `0u16` 结尾的 UTF-16 缓冲区。
4. 调用 `LoadLibraryExW(wide.as_ptr(), null_mut(), 0x0000_0100 | 0x0000_0800)`：
   - `0x100` = `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR`；
   - `0x800` = `LOAD_LIBRARY_SEARCH_SYSTEM32`；
   - 依赖 DLL 只从被加载 DLL 自己的目录和 `System32` 查找，不搜当前目录和 `PATH`。
5. 返回空 → `NonNull::new` 失败，返回 `Load { path, win32_error }`，错误码来自紧跟其后的 `GetLastError`。
6. 成功 → `NativeLibrary { module: NonNull<c_void>, canonical_path }`，唯一 owner。
7. 成功对象的 `Drop` 调用 `FreeLibrary`；失败则没有 `Self`，不需要释放。
8. `leak(self)` 故意保留 module 到进程结束，供无法证明 callback 已静止的所有者使用。
9. xacc 对 `NativeLibrary` 声明 `unsafe impl Send/Sync`，把并发约束交给上层 owner（`NfApi` 串行化）。

本项目当前 `NativeLibrary` 已实现第 1–7 步的简化版；尚未实现 `optional_symbol`、`leak`、`Send/Sync` 和原始 `win32_error` 类型。错误类型使用 `io::Error`，后续接 ABI 时需要决定是否改为稳定 `u32`。

### 2.3 导出解析

`DynamicNfApiBackend::load`（`backend/dynamic.rs` 84–113 行）在 DLL 加载成功后一次性解析函数表。

必需导出（任一缺失，整体加载失败）：

| 导出 | xacc 类型 | 调用时机 |
| --- | --- | --- |
| `nf_init` | `unsafe extern "C" fn(*const c_char, *mut NfEventHandler) -> i32` | 驱动注册后 |
| `nf_free` | `unsafe extern "C" fn()` | 停止过滤时 |
| `nf_registerDriver` | `unsafe extern "C" fn(*const c_char) -> i32` | 初始化驱动前 |
| `nf_deleteRules` | `unsafe extern "C" fn() -> i32` | 停止时、`nf_free` 前 |
| `nf_addRule` | `unsafe extern "C" fn(*const NfRule, c_int) -> i32` | `nf_init` 后 |
| `nf_ipPostSend` | `unsafe extern "C" fn(*const c_char, c_int, *const NfIpPacketOptions) -> i32` | 回灌 IP 上行 |
| `nf_ipPostReceive` | 同上 | 回灌 IP 下行 |
| `nf_udpPostSend` | `unsafe extern "C" fn(EndpointId, *const c_uchar, *const c_char, c_int, *mut c_void) -> i32` | 回灌 UDP 上行 |
| `nf_udpPostReceive` | 同上 | 回灌 UDP 下行 |
| `nf_setIPEventHandler` | `unsafe extern "C" fn(*mut NfIpEventHandler)` | `nf_init` 后 |

可选导出（缺失不阻止加载，对应能力降级）：

| 导出 | xacc 类型 | 用途 |
| --- | --- | --- |
| `nf_getPoolStatus` | `unsafe extern "C" fn() -> NfDebugMemStatus` | 调试内存状态 |
| `nf_getProcessNameA` | `unsafe extern "C" fn(u32, *mut c_char, u32) -> i32` | ANSI 进程路径解析 |
| `nf_getProcessNameFromKernel` | `unsafe extern "C" fn(u32, *mut u16, u32) -> i32` | UTF-16 进程路径解析 |

解析实现（`dynamic.rs` 451–461 行）：

1. `NativeLibrary::symbol(name)` 调 `GetProcAddress`。
2. 空指针 → `MissingSymbol`。
3. 非空 → `transmute_copy` 成对应的 `unsafe extern "C" fn` 类型。
4. 函数表存入 `DynamicNfApiFns`，DLL owner 同时存入 `Option<NativeLibrary>`。
5. 只要 owner 活着，函数地址就有效；owner 释放后所有地址失效。

`NfStatus` 是 `#[repr(i32)]` 枚举（`abi/types.rs` 18–45 行）：`0` 成功，`-1` 失败，`-2` 无效 endpoint，`-3` 未初始化，`-4` I/O，`-5` 需要重启。`ensure_status` 把非零值转换为带 operation 名称的错误。

## 3. NFAPI 方法注册

### 3.1 回调函数表

`NfEventHandler`（`abi/types.rs` 317–336 行）是 `#[repr(C, packed)]` 的 16 个函数指针：

```text
threadStart, threadEnd,
tcpConnectRequest, tcpConnected, tcpClosed,
tcpReceive, tcpSend, tcpCanReceive, tcpCanSend,
udpCreated, udpConnectRequest, udpClosed,
udpReceive, udpSend, udpCanReceive, udpCanSend
```

`NfIpEventHandler` 是 2 个函数指针：`ipReceive`、`ipSend`。

xacc 的 `NfEventHandler::runtime_callbacks()` 和 `NfIpEventHandler::runtime_callbacks()`（`callback/ffi.rs` 1518–1560 行）把 16 + 2 个槽全部填成 Rust 回调函数。回调结构体放在 `Box` 中，确保地址稳定；nfapi 会一直保存这些指针直到 `nf_free`。

### 3.2 post 函数表

UDP/IP callback 需要把包回灌给 nfapi。xacc 在安装回调前先注册四个 post 函数指针（`callback/ffi.rs` 1267–1284、1327–1330 行）：

```text
nf_udpPostSend, nf_udpPostReceive, nf_ipPostSend, nf_ipPostReceive
```

它们存入全局原子快照 `CALLBACK_POST_FNS`。这样 callback 线程不需要拿到 `DynamicNfApiBackend` 的可变引用，也不需要等待 SDK 锁。

### 3.3 驱动注册与 `nf_init`

`NfApi::init_driver_handler`（`backend/api.rs` 268–275 行）的顺序固定：

1. `backend.init(driver_name, event_handler)` → `nf_init(name, handler)`。
2. 成功后才 `backend.set_ip_event_handler(ip_event_handler)` → `nf_setIPEventHandler(handler)`。
3. 设置 `opened = true`。

驱动注册在 `init` 之前，由 `NfApi::register_driver`（277–279 行）调用 `nf_registerDriver(name)`。

xacc 的 `init_first_available_driver`（`assets.rs` 12–46 行）把两者串起来：

1. 驱动名：配置显式值，或默认候选 `xlaccwfp`、`xlaccwfpB`。
2. 若 `config.driver.register`：
   - 解析源 `.sys` 路径；
   - 复制到 `%SystemRoot%\System32\drivers\<name>.sys`，内容相同则跳过复制；
   - 调 `nf_registerDriver(name)`；失败记录 attempt，尝试下一个候选。
3. 调 `nf_init(name, handler)` + `nf_setIPEventHandler(handler)`；成功返回该驱动名，失败记录 attempt 并尝试下一个。
4. 所有候选失败 → 返回汇总错误。

### 3.4 规则注册

`NfApi::start_default_filters`（`api.rs` 281–293 行）在 `nf_init` 成功后通过 `nf_addRule` 安装：

- TCP connect request（IPv4/IPv6）；
- IPv4-mapped loopback allow；
- UDP filter（IPv4/IPv6）；
- ICMP filter（IPv4/IPv6，冲突模式可跳过）；
- UDP loopback bypass allow 规则插到表头。

`NfApi::close`（322–328 行）调用 `nf_deleteRules`，把 `opened` 置回 false。

## 4. xacc 完整启动执行顺序

入口示例见固定提交的 `crates/platform/windows/examples/windows_nfapi_smoke.rs` 30–227 行。

```text
main
  -> 创建 ControlPlaneRuntime 和 DataPlaneRuntime
  -> 加载配置和 filter rules
  -> WindowsNfapiRuntime::start_with_options...
       -> 校验配置，取得 shutdown deadline
       -> prepare_nfapi_ingress（control blocking）
            -> 解析 nfapi.dll 绝对路径
            -> DynamicNfApiBackend::load
                 -> NativeLibrary::load_exact
                 -> 解析 10 个必需导出 + 3 个可选导出
            -> 绑定 localserver TcpListener
            -> 构建策略 engine 和路由规则
            -> install_post_fns(4 个 post 函数)
            -> 构建 16 + 2 个 callback 表
            -> install_callback_runtime(adapter, process resolver)
            -> NfApi::with_handlers(backend, event_handler, ip_event_handler)
            -> init_first_available_driver
                 -> 复制 .sys 到 System32\drivers（可跳过）
                 -> nf_registerDriver(name)
                 -> nf_init(name, handler)
                 -> nf_setIPEventHandler(handler)
       -> 配置 DNS proxy upstream（失败则 close/free 回滚）
       -> 启动 xlan target generations（失败则回滚）
       -> 启动网络变化 monitor（失败则回滚）
       -> 准备 localserver、MITM、direct observation
       -> spawn localserver / observation / route drainer 任务
       -> commit nfapi ingress（control blocking）
            -> 可选 nf_addRule：UDP connect request 规则
            -> nf_addRule：默认 TCP/UDP/ICMP/loopback 规则
       -> 启动统一调试指标
       -> flush DNS cache
       -> 发布 Running
  -> 等待停止信号
  -> runtime.stop()
```

### 4.1 启动失败回滚

xacc 在 `prepare_nfapi_ingress` 之后的每一步失败都会调用 `close_and_free_api`：

1. 停止新 UDP proxy dispatch，等待已接收的 dispatch。
2. `nf_deleteRules`。
3. `nf_free`。
4. 停止 callback post admission，等待已获得的 post lease。
5. `drop(api)` → 回调上下文空闲时 `FreeLibrary`，否则故意泄漏 module 并记录。
6. 再按逆序停止 xlan、网络 monitor、localserver、MITM、指标等。

失败保留首个具体错误；清理错误作为附加信息追加，不覆盖根因。

## 5. xacc 完整停止执行顺序

`WindowsNfapiRuntime::stop_until`（`runtime.rs` 594–720 行）：

1. 已到 `DataPlaneDrained` → 返回已保存错误或成功。
2. 移除 rule debug 注册，进入 `StopRequested`。
3. `close_and_free_api`：
   - `quiesce_proxy_dispatches_until`：停止 UDP proxy 新准入，等待已接收 dispatch。
   - control blocking 中执行 `api.close()` → `nf_deleteRules`，然后 `api.free()` → `nf_free`。
   - `quiesce_callback_posts_until`：停止 callback post 新准入，清除函数表/回调 runtime 快照，等待已获得 lease 的 post 调用结束。
   - control blocking 中 `drop(api)`：
     - `DynamicNfApiBackend::drop` 停止 UDP proxy admission；
     - `initialized` 为真时再 `nf_free`（正常路径已经为假）；
     - 停止 callback post admission；
     - 取出 DLL owner；callback 上下文空闲 → `FreeLibrary`；否则 `leak()` 保留 module 并记录 `module_unload_skipped`。
4. 进入 `IngressStopped`。
5. 结束 UDP connection lifecycle。
6. 停止 direct observation、网络变化 monitor、localserver、HTTP upstream pool、MITM/CDN、callback runtime、xlan registry、route drainer、统一指标，最后 flush DNS cache。
7. 有失败 → 保存 `WindowsNfapiShutdownError` 并返回；否则推进到 `DataPlaneDrained`。

关键事实：固定提交的 xacc **没有调用 `nf_unRegisterDriver`**。`git grep` 在固定提交中找不到该符号。它只做 `nf_free` 和 `FreeLibrary`，驱动 service 仍留在系统中。因此它不满足本项目“程序退出后卸载驱动”的需求，不能照搬。

## 6. 与当前项目的差异和采用决定

| 主题 | xacc 固定提交 | 本项目 W02-a | 决定 |
| --- | --- | --- | --- |
| DLL 路径 | 自动发现 exe 目录/workspace 资产，可配置覆盖 | 只接受显式绝对路径，不搜索 | 拒绝自动发现，保持可审计 |
| DLL 加载 | `NativeLibrary::load_exact`，flags `0x100|0x800` | 已实现同 flags 的简化版 | 采用 |
| 导出解析 | 10 必需 + 3 可选，一次性函数表 | 解析 5 个：`nf_registerDriver`、`nf_init`、`nf_setIPEventHandler`、`nf_free`、`nf_unRegisterDriver` | 分阶段采用 |
| 函数指针类型 | `DynamicNfApiFns` 结构体 + `transmute_copy` | 函数指针元组 + `transmute_copy` | 采用，去掉结构体 |
| 回调注册 | 16 TCP/UDP + 2 IP，post 函数表，callback runtime | 注册 16+2 个无逻辑 callback；不注册 post 函数、不建 callback runtime | W02-a 子集 |
| `nf_init` | 驱动注册后调用，再 `nf_setIPEventHandler` | W02-a 同样调用 | 采用 |
| 规则 | `nf_addRule` 默认规则，停止时 `nf_deleteRules` | W02-a 不安装规则 | 延后 |
| 驱动注册 | 复制 `.sys` 到 `System32\drivers`，再 `nf_registerDriver(name)` | 同 xacc | 采用 |
| 停止驱动 | 无 `nf_unRegisterDriver`，service 保留 | `nf_free` → SCM 停止 → `nf_unRegisterDriver` → 删除本次复制文件 → `FreeLibrary` | **有意扩展**，满足退出卸载 |
| 启动健康检查 | 只信 `NF_STATUS` | 注册后查询服务状态到 `RUNNING` | 增加，防止启动失败被吞 |
| 回调卸载安全 | quiesce + lease + 必要时 leak module | 无 callback 逻辑；先 `nf_free` 再停止和释放 | W02-a 简化；加入 callback 逻辑前必须补静止屏障 |
| 错误类型 | 结构化 `NfApiError` + `NativeLibraryError` | 统一 `Err(String)` | 用户决定，去掉错误枚举 |
| 注释 | 英文 | 中文 | 用户决定 |

## 7. 本项目 W02-a 的完整执行逻辑

用户已确认：采用 xacc 的复制 `.sys` + `nf_registerDriver`，扩展到 `nf_init` + 16+2 callback 注册；callback 只注册、不实现逻辑；按 Enter 退出。

### 7.1 启动

1. 读取两个位置参数：`nfapi.dll` 绝对路径、源 `netfilter2.sys` 绝对路径。
2. 校验：参数恰好两个、两个路径绝对、源 `.sys` 是已存在的文件。
3. `NativeLibrary::load_exact` 加载 `nfapi.dll`：
   - 绝对路径 → 规范化 → `LoadLibraryExW`（`0x100|0x800`）；
   - 失败 → 退出码 1；
   - 成功后由 `NativeLibrary` 独占 handle。
4. 解析 5 个导出：
   - `nf_registerDriver(const char*) -> i32`
   - `nf_init(const char*, *mut NfEventHandler) -> i32`
   - `nf_setIPEventHandler(*mut NfIpEventHandler)`
   - `nf_free()`
   - `nf_unRegisterDriver(const char*) -> i32`
   - 任一缺失 → 释放 DLL，退出码 1。
5. 复制源 `.sys` 到 `%SystemRoot%\System32\drivers\netfilter2.sys`：
   - 目标不存在或内容不同 → 复制，记录“本次复制”；
   - 内容完全相同 → 跳过，不记录“本次复制”；
   - 复制失败 → 释放 DLL，退出码 1。
6. 调用 `nf_registerDriver("netfilter2")`；非零 → 释放 DLL，退出码 1。
7. 查询服务状态，10 秒内要求 `SERVICE_RUNNING`；未到达 → 尝试停止并注销，释放 DLL，退出码 1。
8. 构造 16 个 `NfEventHandler` 槽和 2 个 `NfIpEventHandler` 槽，全部指向无逻辑 callback；放入 `Box` 保持地址稳定。
9. 调用 `nf_init("netfilter2", event_handler)`；非零 → 尝试停止并注销，释放 DLL，退出码 1。
10. 调用 `nf_setIPEventHandler(ip_event_handler)`。
11. 输出“驱动和过滤会话已加载”，等待 Enter。

### 7.2 退出

1. 调用 `nf_free()` 停止过滤线程。
2. 打开 SCM，以 `SERVICE_STOP | SERVICE_QUERY_STATUS` 打开 `netfilter2` 服务。
3. `ControlService(SERVICE_CONTROL_STOP)`；服务已经不在运行视为已停止。10 秒内轮询到 `SERVICE_STOPPED`。
4. 调用 `nf_unRegisterDriver("netfilter2")`；非零 → 报告错误，退出码 1。
5. 若本次确实复制过文件，删除 `%SystemRoot%\System32\drivers\netfilter2.sys`；删除失败 → 报告错误，退出码 1。
6. `NativeLibrary` 离开作用域 → `FreeLibrary`。
7. 退出码 0。

### 7.3 callback 行为

- 16 个 `NfEventHandler` 槽和 2 个 `NfIpEventHandler` 槽全部注册为无逻辑函数。
- 不修改 verdict、不分配内存、不阻塞、不写日志，只立即返回；不能 panic。
- 本阶段不安装规则，callback 预期不会被触发；注册目的是建立 `nf_init` 所需的 callback 表。
- callback 表必须在 `nf_init` 前固定地址；`Box` 持有到 `nf_free` 之后。

### 7.4 不做的事

- 不安装 TCP/UDP/ICMP 规则。
- 不实现任何 callback 业务逻辑。
- 不创建 localserver、数据面、tunnel 或 runtime。
- 不自动发现 DLL 或 `.sys`。
- 不处理 Ctrl+C；只支持 Enter。

### 7.5 vendor 行为依据

1. `nf_registerDriver` 注册并启动 `System32\drivers\<name>.sys`；xacc 使用该流程。
2. `nf_unRegisterDriver` 反汇编只以 `SERVICE_DELETE` 打开服务并调用 `DeleteService`，没有 `ControlService`；因此必须先停止服务。
3. vendor `uninstall_driver.bat` 的顺序是 `sc stop netfilter2` → `nfregdrv -u netfilter2` → 删除驱动文件。
4. service name 固定 `netfilter2`；改成项目名会使注册失败。
5. `nfapi.dll` 的启动辅助函数即使 `StartService` 失败也返回成功，所以必须独立查询服务状态。

## 8. 实施拆分与当前状态

| 轮次 | 内容 | 状态 |
| --- | --- | --- |
| 1 | `crates/app/client` 入口、两个绝对路径参数、中文注释、去掉无用结构体 | 进行中 |
| 2 | 加载 `nfapi.dll`，解析 5 个导出为函数指针元组 | 未开始 |
| 3 | 复制 `.sys`、`nf_registerDriver`、服务 RUNNING 检查 | 未开始 |
| 4 | 16+2 无逻辑 callback、`nf_init`、`nf_setIPEventHandler` | 未开始 |
| 5 | `nf_free`、停止服务、注销、删除本次复制文件、fake 失败测试 | 未开始 |
| 6 | 管理员权限可恢复 Windows VM 手工验证真实 demo 驱动 | 未授权 |

真实驱动验证不进入普通 CI；没有单独授权不安装或启动真实驱动。

## 9. 关键源码索引

固定提交 `8232fb5d2ae2b2940128b981a056271598bf9e26`：

| 文件 | 关键行 | 内容 |
| --- | ---: | --- |
| `crates/platform/windows/native-library/src/lib.rs` | 16–28, 42–110, 117–162 | flags、FFI、`load_exact`、`symbol`、`leak`、`Drop`、错误 |
| `crates/platform/windows/backends/nfapi/src/backend/dynamic.rs` | 47–113, 209–301, 451–471 | 函数表、加载、注册/初始化/释放、符号转换、状态检查 |
| `crates/platform/windows/backends/nfapi/src/backend/api.rs` | 228–333 | `NfApi` owner、`init_driver_handler`、规则、close/free |
| `crates/platform/windows/backends/nfapi/src/abi/types.rs` | 18–45, 317–373 | `NfStatus`、两类 callback 表 |
| `crates/platform/windows/backends/nfapi/src/callback/ffi.rs` | 1267–1339, 1516–1560 | post 函数表、callback runtime、16+2 callback 表 |
| `crates/platform/windows/src/capture/nfapi/assets.rs` | 12–123 | 驱动候选、DLL/`.sys` 路径、复制、注册与 init |
| `crates/platform/windows/src/capture/nfapi/runtime.rs` | 194–547, 589–768, 971–1158 | 启动组合、停止、回滚、`prepare_nfapi_ingress` |
| `crates/platform/windows/examples/windows_nfapi_smoke.rs` | 30–227 | 完整进程入口示例 |

vendor：

| 文件 | 关键行 | 内容 |
| --- | ---: | --- |
| `client-cpp/nfsdk-demo-1.7.6.4/nfsdk/wfp/include/nfapi.h` | 76–103 | `nf_registerDriver`/`nf_registerDriverEx`/`nf_unRegisterDriver` 声明 |
| `client-cpp/nfsdk-demo-1.7.6.4/nfsdk/include/nfevents.h` | 18–26 | `NF_STATUS` 枚举 |
| `client-cpp/nfsdk-demo-1.7.6.4/nfsdk/wfp/bin/uninstall_driver.bat` | — | `sc stop` → 注销 → 删除文件的 vendor 顺序 |

## 10. 残留风险与待确认

1. 固定提交的 xacc 工作区当前是脏的；本文没有采用当前工作区。若需要比较最新实现，必须先把工作区固定到新的干净 commit 并更新本文。
2. `nf_registerDriverEx` 的“注册并启动”来自 vendor 头文件和反汇编，尚未在本机真实驱动上验证；真实 VM 验证需要单独授权。
3. `nf_unRegisterDriver` 只删除服务、不停服务，来自反汇编和 vendor 脚本；真实停止行为需要在 VM 中确认。
4. 首次全量测试时既有 `native-library` 测试出现过一次错误 487，强制重新编译后消失；疑似陈旧测试二进制，尚未完全证明根因。
5. 一旦 W02-a 之后引入 `nf_init`/callback，必须重新设计 DLL 卸载静止屏障；不能直接沿用当前“注销后立刻 FreeLibrary”的假设。
