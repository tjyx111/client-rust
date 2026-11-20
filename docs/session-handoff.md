# 新 Session 研发与学习交接

> 本文用于帮助新的 AI session 恢复上下文，不是第二份功能状态台账。功能状态、优先级和完成定义只以 [project-progress.md](project-progress.md) 为准。

## 1. 项目要做什么

本项目是一个 **Windows x64 Rust 学习与实验项目**，目标是逐步参考 `client-cpp` 和 `xacc-route-sdk2` 实现 Simple VPN Client。执行逻辑可理解优先于开发速度、抽象程度和代码简短。

长期工作包括：

1. Windows NetFilter demo 驱动边界：加载 `nfapi.dll`，校验 C ABI 和导出，随后分阶段实现 demo `netfilter2.sys` 的注册、过滤会话、callback、规则、verdict 和回灌。
2. 共享核心：配置、规则、进程保护、flow/session、generation、容量和错误语义。
3. 数据面：PacketBuffer、IPv4/TCP/UDP/ICMP、8 字节 legacy VPN codec、UDP tunnel，之后再扩展 TCP、DNS 等能力。
4. 控制面与工程能力：runtime、IPC、节点选择、可观测性、故障回滚、性能、Windows VM 驱动 E2E 和发布证据。

完整范围和阶段 S0～S10 见 [project-progress.md](project-progress.md)。NetFilter 当前设计见 [windows-driver-loader-design.md](windows-driver-loader-design.md)。

## 2. 新 Session 必须遵守的工作方式

1. 首先完整阅读根目录 [AGENTS.md](../AGENTS.md)，它是最高优先级门禁。
2. 每个生产功能先调研并记录设计，得到用户明确确认后才能实现。
3. 每轮新增或修改的 Rust 代码（含测试）少于 100 行，先做最小原型。
4. 优先显式、简单语法；禁止 `use ... as ...` 导入别名。错误处理优先显式 `match` 和提前 `return`。
5. 每轮代码和自动化测试完成后立即停止；AI 必须显式请用户描述新增代码的项目特定执行逻辑，不得跳过；AI 只提供文件/行号。基础语法/API 细节直接补充，不考试；只有项目特定的控制流、失败分支、所有权/生命周期、并发、ABI 或设计偏差存在实质问题时才追问。只有用户明确说“继续执行”后才能开始下一轮 Rust 代码或后续实现步骤；沉默或一般性肯定不算授权。
6. 实现以最直接、最少抽象为目标。发现过度设计、提前抽象或与已确认设计不符时，立即停止并请求用户裁决。
7. 极致简化：每个函数只保留一条主流程；不为功能以外的东西做封装、分层或预留结构。
8. 所有 Rust 代码注释使用中文；标识符仍用英文。
9. 纯数据转换可用简单的一层函数式写法；错误处理仍用显式 `match` 和提前 `return`；避免自定义结构体，除非所有权/RAII、外部 ABI 或测试替身强制需要。
10. 新增结构体、trait、泛型或模块前，先说明为什么语言/ABI/测试强制需要，并取得用户确认。
11. 用户是熟练的 Go/C++ 程序员，擅长 Linux 网络编程；禁止基础语法/API 测试题，只讨论项目特定的执行逻辑、所有权、并发、ABI、失败策略和设计取舍。
12. 驱动安装完成和清理完成后必须输出有界清单，逐项记录 expected vs actual 和所有预期之外的驱动、服务、文件、路径、状态与失败；清单失败不得改变主流程，但必须可见。
13. 项目定位为 Windows x64 Rust SDK；当前 `crates/app/client` 继续作为开发 harness，未获单独确认前不重构为库 crate。该定位用于后续 API/输出/错误设计，不改变当前门禁。
6. 未经独立授权不得 commit、push、创建 PR、打 Tag、安装真实驱动或发布。

## 3. 当前实现状态（2026-11-17）

- S0 工具链和验证基础已建立：Rust 1.97.1 MSVC x64、Cargo workspace、`Cargo.lock`、Windows CI 和 `scripts/verify.ps1`。
- 当前工作区存在 Windows DLL owner：`crates/platform/windows/native-library/src/lib.rs`。
- `NativeLibrary::load_exact` 已实现：拒绝相对路径、规范化路径、UTF-16 转换、受限 `LoadLibraryExW`、失败分类、owner 持有 handle，并在 `Drop` 中调用 `FreeLibrary`。
- 工作区还存在 `NativeLibrary::required_symbol` 单符号原型：用 `GetProcAddress` 查找导出，成功返回 `NonNull<c_void>`，缺失时返回 `MissingSymbol`。
- 2026-11-17 已运行统一门禁并通过：Release fmt、Clippy、3 个 native-library 测试、workspace Release test 和 Release build。
- W01 整体仍未完成：尚未建立 NFAPI 的 10 个必需导出原子校验、完整 ABI 类型、fake DLL 故障测试和 C probe 闭环。
- W02-a 设计已于 2026-11-18 更新：采用 xacc 的复制 `.sys` + `nf_registerDriver`，扩展到 `nf_init` + 16+2 无逻辑 callback；参数为 `nfapi.dll` 绝对路径 + 源 `netfilter2.sys` 绝对路径；错误统一 `Err(String)`。`main` 已接入复制、注册、RUNNING 等待、Enter 等待、停止/注销/删除和启动/关闭清单；`QueryServiceConfigW` 启动类型/ImagePath 查询和 ImagePath 规范化已接入启动清单；`EnumServicesStatusExW` 的 ABI/FFI、相关驱动过滤和宽字符串助手已实现，枚举函数和清单接入待下一轮；`nf_init`/callback 尚未实现，真实驱动未运行。
- 构建缓存风险：2026-11-19 多次出现源文件 mtime 早于构建产物，Cargo 跳过重编译，`native-library` 测试或 client 测试报 487；强制重建后通过。`scripts/verify.ps1` 现已在构建前刷新 `crates/**/*.rs` mtime，避免陈旧产物；这不删除 target cache。
- 487 复现结论：独立探针循环加载/卸载 `kernel32/version/winhttp/advapi32` 各 1000 次全部通过；生产 loader 未改。测试改用非 KnownDLL 的 `winhttp.dll` 后，连续 3 次全量门禁通过。测试目标更接近实际的 `nfapi.dll`。
- 测试隔离修复：`cleanup_driver_with` 允许注入 stop 函数；`cleanup_orders_stop_unregister_delete` 不再触碰真实 `netfilter2` 服务，覆盖停止失败不注销、注销失败不删除、成功删除三条路径。
- 生命周期注入：`wait_until_running_with` 支持 fake 状态查询，测试覆盖 missing→running、超时、查询错误；另补注册失败后文件清理和删除失败测试。
- 清单日志注入：`append_checklist_log_to` 返回错误，生产包装只告警；测试覆盖失败路径和正常追加。
- 相关驱动枚举已接入：`enum_related_drivers` 使用 1 MiB 缓冲、64 条上限、`truncated` 标记，排除 `netfilter2`，每个相关驱动输出 state/ImagePath；固定启动清单显示 `related_drivers`/`related_driver_*`/`related_drivers_truncated`。
- 退出码与回滚：新增 `cli_exit_codes` 集成测试，覆盖无参数/相对参数退出 2、缺失 DLL 退出 1；注册失败回滚通过 `rollback_after_register_failure` 测试 copied=true 删除、copied=false 保留。
- Callback 表与 `nf_init`：`NfEventHandler` 16 槽、`NfIpEventHandler` 2 槽、packed ABI 大小 128/16 和 6 个无逻辑回调已接入；Box 保持地址稳定，`nf_init` 成功后调用 `nf_setIPEventHandler`；清理顺序为 `nf_free → stop → unregister → delete`，`free_runs_before_stop` 用 fake 断言顺序。
- `nf_init` 失败清理：`cleanup_after_init_failure` 不调用 `nf_free`，测试断言 free 未执行且 stop/unregister/delete 完成；正常/输入失败路径在注销后显式 `drop(library)`。
- 工作区当前有未提交和未跟踪内容；没有本交接可证明的 commit、push、PR、Tag、发布或真实驱动在线验证。

## 4. 当前评审方式与已讲解内容

2026-11-17 用户明确调整学习门禁：不再要求逐行语法讲解和语法理解小测，改为**执行逻辑评审**。评审只关注输入、输出、控制流、失败分支、资源所有权与生命周期，以及实现是否符合用户的设计意图；不复述语法细节，不考语法题。

已经讲解过的内容：

- `Path` 与 `PathBuf`，相对路径与绝对路径，错误为什么保存 owned 路径。
- `#[cfg]`、`#[link(name = "kernel32")]`、`#[derive(Debug)]` 的作用；项目已决定只支持 Windows。
- 显式 `match` 处理 `canonicalize` 错误，避免 `map_err`、闭包、`?`和变量遮蔽叠加。
- `unsafe` FFI 边界、UTF-16 结尾 `0`、`LoadLibraryExW` 的三个参数、返回 handle 和两个搜索 flags。
- `NativeLibraryError::Load(canonical_path, source)` 同时保存 DLL 绝对路径和底层 `io::Error`。
- handle 的唯一 owner 与 `Drop`/`FreeLibrary` 的基本生命周期目的。

2026-11-18 门禁对齐后代码共 205 行，全量门禁通过；按新门禁，等待用户描述执行逻辑并明确说“继续执行”后才进入下一轮。

按新门禁，`load_exact` 和 `required_symbol` 仍各需要一次**执行逻辑复盘**并由用户确认；旧的语法小测作废，不再补考。`required_symbol` 尚未做过该复盘，不能推定其地址生命周期约束已确认。

## 5. 新 Session 的正确起点

1. 先对已有的 `load_exact` 做执行逻辑复盘：输入、成功/失败路径、资源 owner、`FreeLibrary` 时机，用户确认是否符合设计意图。
2. 再对 `required_symbol` 做执行逻辑复盘：输入输出、缺失符号路径、为什么是 `unsafe`、函数地址与 DLL owner 的生命周期约束。
3. 两次复盘都确认后，才讨论 W01 下一轮最小代码；建议是 NFAPI 单个已知导出的类型声明或原子符号表设计，不得直接进入 W02。
4. 每轮顺序固定为：先说明设计与执行逻辑并取得确认 → 写少于 100 行最简代码 → 自动化测试 → 执行逻辑复盘。禁止过度优化和提前抽象。
5. W02-a 设计已确认；下一步按轮实现 `nfapi.dll` 加载与两个导出解析、fake 生命周期、真实 SCM 停止/注销。真实 demo 驱动操作还需要单独的高风险外部操作授权和可恢复 Windows VM。

