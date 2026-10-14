# Simple VPN Client 全局研发门禁

本文件适用于仓库根目录及全部子目录，是本项目的全局执行门禁。子目录可以增加更严格的 `AGENTS.md`，但不得放宽本文件。发现本文件、已确认设计、代码或上级指令冲突时，立即停止受影响工作并请求人工裁决。

## 1. 门禁来源与适配结论

本门禁重新学习并合并了以下基线：

- `D:\code\xunlei\gxsdn2`，Git `2a850580ac1babd5031bd0d0779a42be3d4d61ce`：单一功能台账、CI 等价本地验证、数据面热路径约束、提交/推送/Tag 门禁。
- `D:\code\xunlei\xacc-route-sdk2`，Git `8232fb5d2ae2b2940128b981a056271598bf9e26`：需求与设计确认、Release 全量验证、运行时所有权、错误传播、连接观测、跨平台复用、迁移保真和发布证据。
- `xacc-route-sdk2` 固定的团队规范子模块，Git `67c949ef3bcaf69824d896d07dbd8360c289e139`：人机职责边界、独立授权、非作者审查、不可变 Tag 制品、灰度和回滚。

适配当前仓库后的裁决如下：

1. `docs/project-progress.md` 是唯一功能状态台账，承担参考项目中 `feature.md` 的职责；不得再创建 `feature.md`、`todo.md` 或第二份状态清单。
2. 当前仓库是 GitHub 项目，使用仓库所有者明确指定的 `libenshuai <tjyx111@gmail.com>`。参考仓库的 `@xunlei.com` 邮箱要求不适用于本仓库。
3. 当前仓库尚未建立 Cargo workspace、固定 Rust 工具链和统一验证脚本。完成 P0 基线项前，只能声称文档检查通过，不得声称 Rust 全量门禁通过。
4. 两个参考仓库的实现是行为和工程约束来源，不是可无审查复制的代码来源。采用行为必须记录来源、兼容决定和本仓库自动化测试。

## 2. 权威文档与状态台账

- `docs/project-progress.md` 记录范围、非目标、架构、优先级、功能状态、测试要求、风险和待确认事项。
- 每个功能保持一个简短状态项；复杂设计、协议表、基准报告和验收证据放入独立文档并由状态项链接。
- 新增、完成、调整优先级、废弃或拆分功能时，代码和台账必须在同一提交更新。
- 只有生产实现、正常/边界/失败路径自动化测试、适用构建和验收全部完成后，才允许将 `[ ]` 改为 `[x]`。
- 外部参考项目已经完成、人工试用成功或测试代码已编写，都不等于本项目功能已完成。
- 文档与实现不一致视为功能未完成；不得用口头说明替代台账更新。

## 3. 人机职责与独立授权

以下授权彼此独立，任何沉默、讨论、部分确认或历史授权都不能推导出下一项授权：

1. 需求和范围确认。
2. 设计确认并允许进入生产实现。
3. 允许创建 Git commit。
4. 允许 push 或创建远端分支/MR/PR。
5. 允许打 Tag。
6. 允许发布、灰度、回滚或执行其他高风险外部操作。

AI 可以调研、设计、实现、测试和提供审查意见，但不能替代需求确认人、非作者审查人或发布负责人。汇报必须区分“已修改、已测试、已审查、已提交、已推送、已发布、已在线验证”，不得混用。

## 4. 需求与设计确认门禁

任何生产代码实现前，必须先在仓库文档中记录并由用户明确确认：

- 目标、范围、非目标、兼容对象和验收标准。
- 成功、失败、降级、fail-open/fail-closed 和回滚语义。
- 文件/模块边界、公开接口、配置、数据模型、上下游和依赖方向。
- runtime、线程、actor、队列、缓存、连接、驱动、socket、路由和临时资产的 owner 与生命周期。
- 数据面最坏路径、资源硬上限、性能预算和可观测性。
- 自动化测试层次、外部环境矩阵、灰度、监控、停止条件和回滚方案。
- 建议的提交拆分顺序及每个提交的独立可验证性。

明确确认前，只允许调研和修改门禁、需求、设计、台账及测试 fixture 说明；不得开始生产实现。实现中发现歧义、范围变化、关键假设不成立或需要偏离已确认设计时，停止受影响实现，更新设计并重新确认。

当前状态：`docs/project-progress.md` 已记录第一版实施规划，但兼容边界、失败策略和首版范围仍待明确确认，因此生产实现门禁尚未打开。

## 5. 参考实现保真与复用

- 开始功能前先检索本仓库、`gxsdn2`、`xacc-route-sdk2` 和 `client-cpp` 是否已有可复用能力，优先复用共享核心、成熟 crate 和经过验证的协议/状态机。
- 对迁移或兼容行为记录精确来源路径、参考 Git SHA、采用/改写/延后/拒绝决定，以及对应自动化测试 ID。
- 不得凭记忆重写协议、ABI、错误码、回调顺序或生命周期语义。
- 参考行为有歧义时停止并确认；发现疑似缺陷时不得静默复制，也不得静默修复，必须记录兼容影响后由人工决定。
- 平台 adapter 只处理系统边界；策略、协议、IPC、重连、生命周期和数据面业务逻辑必须放在共享核心，禁止按平台复制。
- 引入自研基础设施前必须评估成熟生态方案，记录选择理由、维护成本、安全性、许可证和退出方案。

## 6. 架构与运行时所有权

- 只允许一个 `ControlPlaneRuntime` 和一个 `DataPlaneRuntime`，由组合根创建、注入和关闭。
- 子模块不得私建 Tokio runtime、常驻原生线程或脱离 task group 的后台任务。
- 阻塞系统调用和 CPU 重任务必须通过有界 blocking admission；数据面 async 任务不得直接阻塞。
- NFAPI callback 运行在外部线程，只允许短小、确定、非阻塞操作；跨 callback 生命周期的数据必须先复制或转移到 owned buffer。
- 高频可变状态由 actor 或固定分片的单 owner 管理；共享状态必须证明锁竞争上界。
- start/stop/restart/cancel/panic/timeout 均须有明确 owner、顺序、deadline、join 和清理结果。
- generation replacement 必须经过 prepare、ready、commit、旧代隔离和 rollback；迟到事件不得污染新代。

## 7. 数据面硬门禁

每包、每 flow 和每次 poll 的生产路径必须同时满足：

- 有界、非阻塞，无外部网络/磁盘 I/O，无 DNS、HTTP、进程查询或同步日志写入。
- 无随业务规模增长的全表扫描、无无界遍历、无隐式大分配、无不必要的全包复制。
- queue、packet count、packet bytes、pending flow、session、缓存、重组、重试、连接和每轮工作量都有显式硬上限。
- 使用索引、固定分片、桶、时间轮、游标或预算化增量清理替代全表扫描。
- 队列必须定义 Full/Closed 行为、指标和失败策略；不得用无界队列“解决”背压。
- side-band 指标、日志、调试、配置和上报不得阻塞数据面或改变数据面结果。
- 每个输入包或事件恰好产生一个终态 verdict；错误和取消也不能丢失所有权。
- flow 动作一旦锁定，对已有 flow 保持粘性；新规则只影响新 flow，除非已确认的 replacement 设计另有规定。

数据面评审必须说明：状态 owner、最坏时间/空间成本、容量耗尽行为、锁与阻塞风险、是否存在扫描、复制次数，以及 Release 模式吞吐、CPU、RSS、queue、drop/reject 和 P95/P99 延迟证据。

## 8. 协议、FFI 与驱动边界

- 8 字节 legacy VPN header、IPv4/TCP/UDP/ICMP、DNS、IPC 和 NFAPI ABI 必须由显式 codec/adapter 处理；禁止直接发送 Rust struct 内存布局。
- parser 必须检查长度、版本、枚举、MTU、算术溢出和容量，畸形输入只能返回稳定错误，不能 panic 或越界。
- 线协议兼容使用 golden vector、round-trip、property 和 fuzz 测试；未知但允许回显的位必须精确保留。
- C ABI 必须验证 size、alignment、offset、calling convention、符号和字符串编码；callback panic 不得跨 FFI 边界。
- 驱动调用顺序、回滚、重复启停、callback 静止屏障、buffer 生命周期和 exactly-once verdict 必须可由 fake backend 自动化验证。
- NetFilter demo 驱动只用于开发验证；未完成商业授权、版本/哈希/签名链检查前，禁止进入生产制品。
- 真实签名驱动、管理员权限和 Windows VM 测试是附加证据，不能替代 host fake、contract 和状态机自动化测试。

## 9. 错误、资源、安全与可观测性

- 生产代码不得用 `Err(_)`、`.ok()`、`unwrap_or_default()`、空错误分支或 `let _ =` 静默吞掉错误。
- 每个失败分支必须分类为返回、重试、降级、终止或不可恢复，并说明是否影响 flow、session、generation 或进程。
- 重试只能针对明确可重试且满足幂等条件的操作，必须有次数、总时限、退避和可观测耗尽结果。
- join、shutdown、flush、rollback 和 cleanup 失败必须传播或形成可见的稳定终态；不能用成功覆盖首个根因。
- 连接记录必须在第一个可能失败的操作之前创建，采用 first-failure-wins，并以有界容量保留稳定 reason、stage、target、route、protocol 和终态。
- 所有外部 I/O 有 timeout；所有资源有容量；配置采用安全默认值；不得将秘密、token、真实用户 payload 或未脱敏个人数据写入代码、日志、报告或 fixture。
- 指标和日志须结构化且低基数，覆盖成功/失败、延迟、降级、queue/capacity、drop/reject、generation 和 driver 健康。
- 调试 API、诊断包和 PCAP 默认受控、容量有界、输出转义并脱敏；诊断失败不得改变主流程结果。

## 10. 自动化测试门禁

每个功能必须在台账项中写明自动化测试，至少覆盖：

- 正常路径。
- 边界和容量路径。
- 每个新增失败、超时、取消、重试耗尽、回滚和清理分支中适用的路径。
- 兼容/golden 行为和对外错误码或 schema。

测试层次按风险选取：

1. 单元测试：纯 parser、codec、policy、state machine、checksum、容量算法。
2. Contract/Golden：8 字节协议、NFAPI ABI、配置、IPC、错误码和参考字节样本。
3. 组件测试：fake clock、fake NFAPI、fake process resolver、fake tunnel、fake node API。
4. 本地集成测试：loopback socket、本地 fake VPN server，默认不访问公网。
5. 故障注入：所有新失败分支、队列饱和、资源耗尽、迟到事件和关闭失败。
6. Property/Fuzz：所有不可信协议 parser，要求不 panic、不越界并满足 round-trip 不变量。
7. 性能测试：固定 Release 负载和环境，输出吞吐、CPU、RSS、queue、drop 和尾延迟并关联 commit。
8. 驱动 E2E：专用可恢复 Windows VM 串行执行，验证安装、启停、流量和恢复。

确定性要求：

- 异步测试必须等待实际消费、状态迁移或 task join，不能只断言入队成功。
- 禁止用任意 `sleep` 等待状态；使用 fake clock、事件、barrier 或带诊断的 deadline。
- 禁止删除、跳过、弱化断言、无限重跑或只提高超时来让测试变绿。
- 需要全局 NFAPI owner 的测试只串行化该 owner，不得无理由把整个测试套件改成单线程。
- 测试创建的驱动、服务、route、socket、子进程、临时目录和配置必须在成功、失败、超时和取消时清理。
- 测试失败输出必须包含阶段、输入摘要、预期、实际和资源状态，且保持脱敏。

## 11. 构建与验证门禁

### 11.1 当前文档阶段

Cargo workspace 和统一验证脚本建立前，至少执行：

```powershell
git diff --check
```

并人工确认所有相对链接存在、Markdown 为 UTF-8、状态台账没有第二份副本。此阶段只能报告“文档门禁通过”。

### 11.2 Rust 基线建立后

P0 基线必须固定 `rust-toolchain.toml`、`Cargo.lock`、MSRV、依赖来源和许可证策略，并提供一个仓库内的一键验证入口。统一入口尚未建立时，最低全量门禁为：

```powershell
cargo fmt --all -- --check
cargo clippy --release --workspace --all-targets --all-features -- -D warnings
cargo test --release --workspace --all-features
cargo build --release --workspace --all-features
```

要求：

- CI 与本地全量门禁使用相同脚本和锁定依赖，测试成功不能替代 Release 构建成功。
- quick gate 只能缩短开发反馈；提交、push、Tag 和发布仍须运行适用的全量门禁。
- Windows 驱动/服务和任何跨平台目标必须分别构建，不能用 host workspace build 推定目标制品可用。
- 不使用 `cargo clean` 作为常规操作；复用受控 target cache。清理只能作用于明确归属本项目的有界目录。
- 验证报告记录命令、开始/结束时间、结果、源 SHA、工作区是否干净和产物位置；高风险证据必须绑定 SHA。
- 门禁失败不得通过忽略退出码、删测试、降级 lint、缩小默认功能集或改为 Debug 构建绕过。

## 12. 审查、提交与 Git 门禁

- 开发者完成编码和自测后，必须显式请求 AI 审查；审查基于实际 diff，列出严重级别、影响、测试、未验证项和残余风险。
- 协议、数据面、核心路由、驱动、运行时和安全变更需要相应模块 owner 或具备该能力的非作者人工审查。
- 一个 commit 只表达一个意图，并保持可构建、可测试；修复审查问题后重新审查受影响 diff。
- 创建 commit 前验证 `user.name`、`user.email`、author 和 committer。当前批准身份为 `libenshuai <tjyx111@gmail.com>`；不一致时停止并请求确认，不得擅自改成参考仓库身份。
- 未经用户明确要求不得 commit；未经单独明确要求不得 push、创建远端分支或 PR。
- 禁止直接 push `main`；远端变更使用功能分支和 PR。push 前 fetch 最新远端：无未发布提交时 fast-forward，有未发布提交时 rebase 到最新目标分支，然后重跑门禁并检查最终 diff/history。
- 不得使用破坏性 Git 操作覆盖用户改动；不得清理、暂存、提交或改写与本任务无关的工作树内容。

## 13. Tag、发布、灰度与回滚

- Tag 只能从干净、已审查、已合并且与 `origin/main` 一致的提交创建，并需要独立明确授权。
- 生产制品只能由 Tag CI 构建；禁止发布本机构建、分支构建或可变的 `latest` 制品。
- 制品 metadata 至少包含 Tag、Git SHA、构建时间、pipeline、工具链版本、依赖锁和哈希；适用时包含 SBOM 和签名链。
- 发布负责人定义灰度批次、观测指标、停止条件和回滚 owner；在线验证前不得宣称发布完成。
- 回滚使用原始 Tag 的不可变制品，不得临时从旧源码重新构建。紧急发布不豁免记录、审查、测试和可追溯要求。

## 14. 强制停止条件

遇到以下任一情况必须停止受影响工作并请求人工决定：

- 需求、设计、兼容性、失败策略、授权或验收标准不明确。
- 需要偏离已确认设计、扩大范围或引入新外部系统/高风险权限。
- 参考实现互相冲突、来源工作区不干净且无法固定 fixture、协议或 ABI 无可验证基线。
- 数据面无法证明有界/非阻塞，资源 owner 不唯一，或错误只能通过静默降级处理。
- 测试需要真实秘密、真实用户数据、不可恢复驱动环境或无法自动清理的外部状态。
- Git 身份不符、目标分支不明确、远端已分叉、门禁失败或证据无法绑定当前 SHA。
- 需要 push、Tag、发布、回滚、生产驱动安装或其他未获独立授权的外部变更。

## 15. 完成定义

一个功能只有在以下条件全部满足时才能标记完成：

1. 已确认设计、兼容行为和失败语义均已实现，无关键歧义。
2. 正常、边界和新增失败分支都有确定性自动化测试。
3. 生产错误不被静默转换为成功、EOF、默认值或无条件 fallback。
4. 队列、缓存、session、flow、packet bytes、重试和诊断存储均有硬上限和可观测耗尽行为。
5. 启动、停止、取消、panic 和 rollback 不遗留任务、callback、socket、route、driver 状态或临时资产。
6. 适用的格式、Clippy、Release 测试、Release 构建、集成、性能和平台门禁通过。
7. 实现、测试、文档、唯一状态台账和可追溯证据在同一变更中保持一致。
