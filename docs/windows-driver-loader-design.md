# Windows NFAPI 动态加载阶段设计

状态：**第一轮已于 2026-10-15 获用户确认；W02-a 设计于 2026-11-18 获用户确认，第 1 轮入口/参数契约已实现并通过全量门禁；其余子阶段未授权**

阶段标识：`W01-loader-v1`

日期：2026-10-14

第一轮确认范围：只实现精确绝对路径的 Windows DLL owner，并验证正常加载、相对路径拒绝和缺失文件错误；暂不解析 NFAPI 导出，不注册或启动 `netfilter2.sys`。

## 1. 目标、范围与非目标

本阶段只建立 Windows x64 上的 NFAPI 用户态动态库加载边界，为后续驱动注册、`nf_init`、callback 和规则生命周期提供可测试的前置能力。

范围：

- 只接受显式的 `nfapi.dll` 绝对路径，支持空格和 Unicode 路径。
- 路径先规范化，再以 `LoadLibraryExW` 加载；依赖搜索范围只允许 DLL 所在目录和 `System32`，不得搜索当前目录或 `PATH`。
- 一次性解析本项目后续 W02–W05 所需的必需导出；缺失任一必需导出时整体加载失败，不返回部分可用对象。
- 动态库句柄和函数表由一个 `NfApiLibrary` 对象唯一拥有；成功加载后句柄至少存活到所有函数表借用结束。
- 本 crate 明确只支持 Windows x64，不为非 Windows 目标保留兼容分支，以保持学习阶段代码直接、简单。

本阶段非目标：

- 不复制 `.sys` 到 `System32\\drivers`，不注册、启动、停止或卸载内核驱动。
- 不调用 `nf_registerDriver`、`nf_init`、`nf_setIPEventHandler`、`nf_free` 或安装过滤规则。
- 不定义完整 callback 行为、endpoint 状态、verdict、数据面队列或 runtime。
- 不把 NetFilter demo 驱动加入生产制品，不处理商业授权、正式签名、安装器或升级。
- 不实现 sdk2 的候选驱动名 fallback；是否保留该兼容行为在 W02 单独确认。

兼容对象固定为 `nfsdk-demo-1.7.6.4/nfsdk/wfp` 的 C API x64 `nfapi.dll`。本阶段只保证装载和导出契约，不声称驱动已运行或具备生产可用性。

## 2. 参考来源与采用决定

固定参考提交为 `xacc-route-sdk2` Git `8232fb5d2ae2b2940128b981a056271598bf9e26`。

| 来源 | 决定 | 本仓库证据 |
| --- | --- | --- |
| `crates/platform/windows/native-library/src/lib.rs` | 改写采用绝对 Unicode 路径、规范化、受限 DLL 搜索、RAII 句柄和 Win32 错误保留 | `W01-T01`～`W01-T06` |
| `crates/platform/windows/native-library/tests/unicode_path_tests.rs` | 改写采用 Unicode/空格路径和拒绝相对路径测试 | `W01-T01`、`W01-T02` |
| `crates/platform/windows/backends/nfapi/src/backend/dynamic.rs` | 采用“全部必需符号验证完成后才构造成功对象”；延后 callback lease、`nf_free` 和卸载静止屏障 | `W01-T03`～`W01-T06` |
| `crates/platform/windows/backends/nfapi/tests/non_windows_library_path.rs` | 拒绝采用；本项目当前只构建和运行 Windows x64 客户端 | 不适用 |
| `crates/platform/windows/src/capture/nfapi/assets.rs` | 本阶段拒绝复制 `.sys`、注册驱动和候选名 fallback；这些属于 W02 生命周期设计 | W02 后续测试 |

`gxsdn2` Git `2a850580ac1babd5031bd0d0779a42be3d4d61ce` 不包含可复用的 Windows NFAPI 动态加载能力。`client-cpp` 的相关工作区不干净，只作为 vendor ABI/资产候选来源，不作为本阶段持续构建依赖。

## 3. 模块、接口与依赖

建议新增两个最小 crate，保持系统边界和 NFAPI 契约分离：

```text
crates/platform/windows/native-library/
  src/lib.rs       # 通用、精确路径的 Windows DLL owner
  tests/            # Windows Unicode/路径/卸载测试

crates/platform/windows/nfapi/
  src/lib.rs
  src/error.rs      # 稳定错误分类
  src/dynamic.rs    # NFAPI 必需符号表及整体加载
  tests/            # fake DLL/真实 fixture 合约测试
```

公开接口候选：

```rust
pub struct NativeLibrary { /* 唯一句柄 owner */ }

impl NativeLibrary {
    pub fn load_exact(path: &Path) -> Result<Self, NativeLibraryError>;
    pub fn required_symbol(&self, name: &'static CStr)
        -> Result<NonNull<c_void>, NativeLibraryError>;
    pub fn path(&self) -> &Path;
}

pub struct NfApiLibrary { /* NativeLibrary + typed function table */ }

impl NfApiLibrary {
    pub fn load(path: &Path) -> Result<Self, NfApiLoadError>;
    pub fn path(&self) -> &Path;
}
```

函数指针表不对业务层公开；后续 W02 只能通过受控 adapter 调用，避免上层持有脱离 DLL owner 的裸函数指针。配置解析不属于本阶段，调用方必须传入绝对 `Path`。

实现优先复用 Rust 标准库和最小 Win32 FFI 边界，不引入运行时、线程、队列或缓存。`libloading` 虽可加载 DLL，但本阶段需要明确控制 `LoadLibraryExW` flags、稳定记录 Win32 错误和禁止隐式搜索；因此候选实现与 sdk2 一致，使用小型、隔离的 Win32 adapter。若实施时改用第三方 crate，属于依赖与安全模型变化，必须先更新本文并重新确认。

## 4. 必需导出与 ABI 边界

第一版必需导出固定为：

- `nf_init`
- `nf_free`
- `nf_registerDriver`
- `nf_deleteRules`
- `nf_addRule`
- `nf_ipPostSend`
- `nf_ipPostReceive`
- `nf_udpPostSend`
- `nf_udpPostReceive`
- `nf_setIPEventHandler`

`nf_getPoolStatus`、`nf_getProcessNameA` 和 `nf_getProcessNameFromKernel` 暂不进入本阶段公开能力；是否作为可选符号由对应观测/进程解析阶段确认。必需符号名称和 calling convention 必须与仓库现有 `fixtures/design-freeze/netfilter-abi-x64.json` 及 MSVC C probe 一致。

所有 `unsafe` 仅允许存在于 Windows loader 和函数地址到已确认 ABI 类型的转换点。相关 crate 启用 `unsafe_op_in_unsafe_fn = "deny"`，每个 unsafe block 写明句柄、指针、生命周期和 ABI 前提；其他 crate 继续禁止 unsafe。

## 5. 生命周期、失败与回滚语义

唯一 owner 为 `NfApiLibrary`：

1. 校验输入为绝对路径。
2. 规范化并保存实际加载路径。
3. 以受限搜索 flags 加载模块。
4. 按固定清单解析全部必需符号。
5. 全部成功后才发布 `NfApiLibrary`。
6. owner 销毁时调用一次 `FreeLibrary`。

失败语义：

| 阶段 | 行为 |
| --- | --- |
| 相对路径、路径不存在或无法规范化 | fail-closed，返回包含阶段和路径的稳定错误，不尝试其他搜索路径 |
| DLL 加载失败 | fail-closed，保留 Win32 error code，不重试、不 fallback |
| 任一必需符号缺失 | fail-closed，释放已加载模块，返回缺失符号及规范化路径 |
| `FreeLibrary` 失败 | 记录结构化低基数诊断；由于 `Drop` 无法返回错误，W02 在允许函数调用和 callback 后必须提供显式 shutdown API 来传播卸载结果 |

本阶段没有业务降级或 fail-open：加载失败意味着 NFAPI backend 不可用，但不影响不依赖驱动的文档、配置检查或其他 CLI 能力。没有自动重试，也不扫描候选目录。

本阶段没有 callback，因而可在最后一个 owner 销毁时卸载。W02 一旦引入 callback 或并发调用，必须先增加 lease/静止屏障；不得沿用本阶段的直接 Drop 卸载语义而不重新确认。

## 6. 资源、性能与可观测性

- 资源硬上限：每次成功对象恰好一个 DLL module handle；必需符号数量固定为 10；不创建线程、task、队列、缓存或后台工作。
- 最坏时间复杂度：一次路径规范化、一次 DLL 加载、10 次固定符号查询，均只发生在控制面启动路径。
- 数据面影响：无每包、每 flow 或每 poll 路径，不进入数据面热路径。
- 阻塞：文件系统规范化和 Win32 loader 均为同步阻塞调用，后续组合根必须通过有界 blocking admission 调用；本阶段 crate 自身不创建 runtime。
- 复制：仅路径 UTF-16 编码和错误文本；不接触用户 payload。
- 日志：库层返回类型化错误，不主动写成功日志；卸载失败因 Drop 无返回通道，仅输出不含秘密和动态高基数标签的诊断。W02 应把显式 shutdown 结果接入统一 driver health 指标。

## 7. 自动化测试与验收

| ID | 层次 | 场景与断言 |
| --- | --- | --- |
| `W01-T01` | Windows 集成 | 从含中文和空格的绝对路径加载测试 DLL，并解析一个已知导出 |
| `W01-T02` | 单元 | 相对路径被拒绝，且不搜索当前目录或 `PATH` |
| `W01-T03` | Contract | 完整 fixture 的 10 个必需导出全部解析后才返回成功 |
| `W01-T04` | 故障注入 | 缺失一个必需导出的 fake DLL 整体失败，错误包含稳定阶段和符号名 |
| `W01-T05` | 故障注入 | 不存在、不可访问、非 DLL 文件分别返回可分类错误且不 panic |
| `W01-T06` | 生命周期 | 失败和成功路径均释放模块；重复构造/销毁不遗留测试临时目录或句柄 |
| `W01-T07` | 平台约束 | Windows x64 Release 构建通过；当前不声明非 Windows 编译兼容 |
| `W01-T08` | ABI contract | Rust 必需符号清单与 ABI fixture/C probe 清单一致，禁止静默增删 |

测试 DLL/fake DLL 必须由仓库内测试 fixture 或测试构建脚本生成，不能依赖机器预装 NFAPI。真实 vendor `nfapi.dll` 的 Windows 检查作为附加 contract 证据；真实内核驱动不在本阶段测试中安装或启动。

统一验收入口仍为：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1
```

外部矩阵为 Windows 10/11 x64 MSVC。普通 CI 不要求管理员权限。若 fake DLL 构建需要 MSVC，将其纳入统一脚本并保证失败、取消时清理本次运行专用临时目录；不得使用任意 `sleep`。

## 8. 灰度、停止条件与回滚

本阶段不发布、不灰度、不安装驱动。后续接入时用配置显式选择 NFAPI backend；加载失败不得留下部分初始化状态。

停止实施并重新确认的条件：

- 必需符号清单与固定 vendor header/fixture 不一致。
- 需要接受相对路径、搜索 `PATH`、自动下载或从外部位置发现 DLL。
- 需要在本阶段复制/注册/启动 `.sys` 或调用 `nf_init`。
- DLL 无法在 callback/调用结束前安全卸载，必须引入进程级泄漏或全局 owner。
- 需要新增第三方 loader 依赖或改变 unsafe/依赖搜索模型。

代码回滚为移除新增 crate 和 workspace 成员；因本阶段不改变系统服务、驱动、注册表、route 或 socket，不需要机器状态回滚。

## 9. 建议提交拆分

1. `docs: confirm Windows NFAPI loader contract`：本文和唯一台账链接，仅文档验证。
2. `feat: add exact-path Windows native library owner`：`NativeLibrary`、错误类型及 `W01-T01/T02/T05/T06/T07`。
3. `feat: add atomic NFAPI symbol-table loader`：`NfApiLibrary`、固定符号表及 `W01-T03/T04/T08`。
4. `test: wire Windows loader checks into unified verification`：CI/fixture 编排调整和全量 Release 验证证据。

每个提交独立可构建、可测试。未经单独授权不创建 commit；实现完成后仍需用户显式触发 AI Review，并由具备 Windows/FFI/驱动能力的非作者人工审查。

## 10. W02-a 驱动加载、`nf_init` 与退出卸载设计（2026-11-18 更新）

状态：用户已确认采用 xacc 的复制 `.sys` + `nf_registerDriver` 流程，并扩展到 `nf_init` + 16+2 callback 注册（回调只注册，不实现逻辑）。第 1 轮入口/参数契约已实现；后续按轮实现。

用户目标：程序启动时加载并启动 `netfilter2.sys`，调用 `nf_init` 建立过滤会话并注册 callback；用户按 Enter 后停止过滤、停止并注销驱动、删除本次复制的驱动文件，然后退出。不安装任何过滤规则，不实现 callback 业务逻辑，不开始 VPN 转发。

### 10.1 来源与决策

- 固定 vendor 头文件 `nfsdk-demo-1.7.6.4/nfsdk/wfp/include/nfapi.h` 的 SHA-256 与 `fixtures/design-freeze/netfilter-abi-x64.json` 一致。
- **采用 xacc 的资产流程**：源 `netfilter2.sys` 绝对路径 → 复制到 `%SystemRoot%\System32\drivers\netfilter2.sys` → 调用 `nf_registerDriver("netfilter2")`。不采用 `nf_registerDriverEx` 的自定义目录方式。
- `nf_registerDriver` 注册并启动驱动；它与 `nf_registerDriverEx` 的反汇编都包含注册和启动两个内部步骤。
- 反汇编显示启动辅助函数即使 `StartService` 失败也可能返回成功，因此注册后必须查询服务状态，并要求在 deadline 内到达 `SERVICE_RUNNING`。
- `nf_unRegisterDriver(const char *name)` 只以 `SERVICE_DELETE` 打开服务并调用 `DeleteService`，**不会停止正在运行的驱动**。vendor 的 `uninstall_driver.bat` 顺序是 `sc stop netfilter2` → `nfregdrv -u netfilter2` → 删除驱动文件。本项目采用同样的顺序。
- service name 固定为 vendor 标识 `netfilter2`；参考实现实测改成项目名会使注册失败。
- `nf_registerDriver` 只接收 ASCII 的 service name；源 `.sys` 路径由 `fs::copy` 处理，可以包含 Unicode。
- 注册/启动/停止/删除内核服务需要管理员权限。
- 错误统一使用 `Result<_, String>`，不再定义错误枚举。
- 所有 Rust 注释使用中文；不为功能以外的东西新增结构体。callback 表是外部 C ABI 强制需要的 `#[repr(C, packed)]` 结构体，属于允许的例外。

### 10.2 main 执行顺序

启动：

1. 读取两个位置参数：`nfapi.dll` 绝对路径、源 `netfilter2.sys` 绝对路径；service name 固定为 `netfilter2`。
2. 校验：参数恰好两个、两个路径绝对、源 `.sys` 是已存在的文件。
3. 用 `NativeLibrary::load_exact` 加载 `nfapi.dll`。
4. 解析 5 个导出：`nf_registerDriver`、`nf_init`、`nf_setIPEventHandler`、`nf_free`、`nf_unRegisterDriver`；缺失任一则释放 DLL，退出码 1。
5. 把源 `.sys` 复制到 `%SystemRoot%\System32\drivers\netfilter2.sys`：
   - 目标不存在或内容不同 → 复制，记录“本次复制”标志；
   - 内容完全相同 → 跳过复制，不记录“本次复制”标志；
   - 复制失败 → 释放 DLL，退出码 1。
6. 调用 `nf_registerDriver("netfilter2")`；返回非零 → 释放 DLL，退出码 1。
7. 查询 `netfilter2` 服务状态，10 秒 deadline 内要求到达 `SERVICE_RUNNING`；未到达 → 尝试停止并注销，释放 DLL，退出码 1。
8. 构造 16 个 `NfEventHandler` 槽和 2 个 `NfIpEventHandler` 槽，全部指向无逻辑 callback；放入 `Box` 保持地址稳定。
9. 调用 `nf_init("netfilter2", event_handler)`；返回非零 → 尝试停止并注销，释放 DLL，退出码 1。
10. 调用 `nf_setIPEventHandler(ip_event_handler)`。
11. 输出“驱动和过滤会话已加载”，等待用户按 Enter。

退出：

1. 调用 `nf_free()` 停止过滤线程。
2. 打开 SCM，以 `SERVICE_STOP | SERVICE_QUERY_STATUS` 打开 `netfilter2` 服务。
3. 调用 `ControlService(SERVICE_CONTROL_STOP)`；服务已经不在运行视为已停止。10 秒 deadline 内轮询到 `SERVICE_STOPPED`。
4. 调用 `nf_unRegisterDriver("netfilter2")`；返回非零 → 报告错误，退出码 1。
5. 若本次确实复制过文件，删除 `%SystemRoot%\System32\drivers\netfilter2.sys`；删除失败 → 报告错误，退出码 1。
6. `NativeLibrary` 离开作用域 → `FreeLibrary`。
7. 退出码 0。

失败语义：

| 阶段 | 行为 |
| --- | --- |
| 参数缺失、相对路径、源 `.sys` 不存在 | 打印明确用法/错误，退出码 2，不加载 DLL |
| DLL 加载失败或导出缺失 | 打印阶段和错误，释放已加载 DLL，退出码 1 |
| 复制 `.sys` 失败 | 打印源和目标路径，释放 DLL，退出码 1；目标原本不存在时删除可能的部分文件，目标原本存在时保留并报告 |
| `nf_registerDriver` 返回非零 | 打印 `NF_STATUS` 和 `GetLastError`，释放 DLL，退出码 1 |
| 注册成功但服务未在 deadline 内 RUNNING | 尝试停止并注销，释放 DLL，退出码 1 |
| `nf_init` 返回非零 | 尝试停止并注销，释放 DLL，退出码 1 |
| 停止服务失败或超时 | 打印错误，不调用注销，退出码 1；驱动可能仍在运行 |
| 停止成功但注销失败 | 打印错误，退出码 1；驱动已停止但服务可能仍存在 |
| 注销成功但删除文件失败 | 打印错误，退出码 1；服务已注销但文件可能仍存在 |
| 正常按 Enter | `nf_free`、停止、注销、删除本次复制的文件、释放 DLL，退出码 0 |

只读 SCM 状态查询（注册前）失败只打印，不改变退出码；注册后的 RUNNING 检查失败是致命错误，退出码 1。

### 10.3 callback 注册

- `NfEventHandler` 的 16 个槽全部注册为无逻辑函数：线程开始/结束、TCP connect/connected/closed/receive/send/canReceive/canSend、UDP created/connectRequest/closed/receive/send/canReceive/canSend。
- `NfIpEventHandler` 的 2 个槽全部注册为无逻辑函数：`ipReceive`、`ipSend`。
- 无逻辑 callback 不修改任何 verdict、不分配内存、不阻塞、不写日志，只立即返回；不能 panic。
- 本阶段不安装任何过滤规则，因此 callback 预期不会被触发；注册目的是建立 `nf_init` 所需的 callback 表，为后续阶段留出稳定入口。
- callback 表地址必须在 `nf_init` 前固定；使用 `Box` 持有，直到 `nf_free` 之后才释放。

### 10.4 测试与生命周期

- 不引入 trait、service/repository 或自定义封装。生命周期逻辑用自由函数 + 函数指针元组表达，测试直接传入 fake 函数指针。
- 复制目标路径作为参数传入内部函数，测试使用临时目录，不触碰真实 `System32\drivers`。
- Fake 测试覆盖：复制成功/跳过/失败、`nf_registerDriver` 失败、服务未 RUNNING、`nf_init` 失败、`nf_free`/停止/注销/删除的调用顺序、注销后停止的错误路径、DLL 必须在注销之后卸载。
- 参数测试覆盖：缺失参数、相对路径、源 `.sys` 不存在。
- 真实驱动只在管理员权限的可恢复 Windows VM 中手工验证，不进入普通 CI；未获独立授权不安装或启动真实驱动。
- 每轮新增或修改 Rust 代码（含测试）少于 100 行；先实现最小可运行原型，再按执行逻辑复盘。

### 10.5 实施拆分

| 轮次 | 内容 | 状态 |
| --- | --- | --- |
| 1 | 入口、两个绝对路径参数、中文注释、去掉无用结构体 | 已完成 |
| 2 | 加载 `nfapi.dll`，解析 5 个导出为函数指针元组 | 已完成 |
| 3 | 复制 `.sys`、`nf_registerDriver`、服务 RUNNING 检查 | 已完成 |
| 4 | 16+2 无逻辑 callback、`nf_init`、`nf_setIPEventHandler` | 未开始 |
| 5 | 停止服务、注销、删除本次复制文件、fake 失败测试 | 已完成；`nf_free` 待 `nf_init` 后加入 |
| 6 | 管理员权限可恢复 Windows VM 手工验证 | 未授权 |
| 7 | 启动/关闭清单与预期外偏差记录 | 待确认设计 |

### 10.6 已确认决策（2026-11-18）

1. 参数形式：`client.exe <nfapi.dll 绝对路径> <源 netfilter2.sys 绝对路径>`，service name 固定 `netfilter2`。
2. 采用 xacc 的复制 `.sys` 到 `System32\drivers` + `nf_registerDriver`。
3. 扩展到 `nf_init` + 16+2 callback 注册；callback 只注册、不实现逻辑。
4. 退出顺序：`nf_free` → 停止服务 → `nf_unRegisterDriver` → 删除本次复制的 `.sys` → `FreeLibrary`。
5. 注册后必须看到服务 `RUNNING`，不只信 `NF_STATUS`。
6. 错误统一 `Err(String)`，不保留错误枚举。
7. 真实驱动验证只在可恢复 VM 中进行，并另行授权。

### 10.7 启动清单与预期外偏差（2026-11-19 修订，待确认）

目标：安装完成和清理完成后，输出一份有界、结构化、可机器读取的清单，逐项比较 SDK 期望状态与系统实际状态；任何预期之外的驱动、服务、文件、路径、状态和失败都必须记录，方便后期检查系统行为。当前不做驱动签名校验。

**范围说明**：不枚举全部内核驱动。Windows 自身和第三方驱动很多，全量枚举噪声大，和本 SDK 的安装正确性没有直接关系。第一版只检查：

1. SDK 自己的期望服务/驱动文件/`nfapi.dll`；
2. 其他引用 `netfilter2.sys` 的服务；
3. 名称或路径匹配 `netfilter`/`wfp`/`accel`/`vpn` 的相关网络过滤驱动。

如果以后需要完整系统驱动快照，单独做诊断包（O04），不塞进启动清单。

#### 时机

1. **启动清单**：`nf_registerDriver` 成功且服务进入 `RUNNING` 后，等待 Enter 前输出。
2. **关闭清单**：停止服务、注销、删除文件后输出；失败路径也尽力输出。
3. 清单生成失败只打印警告，不改变主流程结果；但必须可见。

#### 第一版期望项

| 项 | 期望 |
| --- | --- |
| 服务 `netfilter2` | 启动时存在且 `RUNNING`；关闭后不存在 |
| 服务启动类型 | `SYSTEM_START` |
| 服务 ImagePath | `system32\drivers\netfilter2.sys` |
| 驱动文件 `System32\drivers\netfilter2.sys` | 启动时存在；SHA-256 等于源 `.sys`；关闭后按 `copied` 决定应不存在或保持原样 |
| `nfapi.dll` | 从调用方传入的绝对路径加载成功；5 个导出全部解析 |
| 其他引用 `netfilter2.sys` 的服务 | 不应存在 |
| 相关网络过滤驱动 | 除 `netfilter2` 外，任何名称或路径匹配 `netfilter`/`wfp`/`accel`/`vpn` 的驱动都列为偏差 |

#### 实际数据来源

- 已有 SCM 查询：服务状态。
- 新增 `QueryServiceConfigW`：启动类型和 ImagePath。
- 新增 `EnumServicesStatusExW(SERVICE_DRIVER)`：列出内核驱动，按名称/路径过滤，条目上限 64，记录 `truncated=true`。
- 文件：读取源和目标，计算 SHA-256；记录长度、哈希、是否匹配。
- 使用 `sha2` crate（RustCrypto，MIT/Apache）计算 SHA-256；需要更新 `Cargo.lock`。
- `nfapi.dll`：记录输入绝对路径、加载成功、5 个导出解析成功；不枚举其他进程模块。
- 不做签名/版本校验。

#### 结果

只有两个结果：

```text
OK   -> 符合预期
ERR  -> 有偏差
```

`ERR` 只记录，不改变退出码；主流程自身的安装/清理错误仍按原逻辑决定退出码。例如：安装和清理都成功，但清单发现额外的相关 WFP 驱动，程序仍按主流程结果退出 0；只有安装/清理本身失败才退出 1。清单生成或写日志失败只打印警告，不改变主流程结果。

#### 输出

1. stdout 逐行：
   `[DRIVER-CHECK] phase=startup|shutdown item=<稳定名称> expected=<值> actual=<值> result=OK|ERR`
2. 同时追加写入 `logs/driver-checklist.log`；目录不存在时创建；写失败时打印警告。
3. 文件行不写秘密、token 或用户 payload；相关驱动条目有上限。
4. 每次运行写入 phase 标记和 Unix 时间戳，便于区分多次运行。

#### 测试

- SHA-256 已知向量测试。
- 纯比较逻辑用内存中的 expected/actual 列表测试，不依赖真实服务。
- 日志写入失败注入：只打印警告，不改变主流程结果。
- VM：干净快照、预存服务/文件、额外相关驱动三种场景；核对 stdout 和 `logs/driver-checklist.log`。

#### 待确认

1. 日志路径：固定 `logs/driver-checklist.log`（推荐），还是每次运行生成带时间戳的文件？
2. 相关驱动过滤词：`netfilter|wfp|accel|vpn`（推荐），还是先缩到 `netfilter|wfp|accel`？
3. `ERR` 只记录、不改变退出码（推荐），确认？
4. 引入 `sha2` crate 并更新 `Cargo.lock`，确认？
