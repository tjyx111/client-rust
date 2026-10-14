# Simple VPN Client 项目进展与实施规划

> 本文件是本项目唯一的功能状态台账。新增、完成、调整、废弃功能时，必须在同一提交中更新本文件。
>
> 只有生产实现、对应自动化测试和适用验收全部完成后，功能才可由 `[ ]` 改为 `[x]`。参考项目已经完成不代表本项目已经完成。

## 1. 当前状态

- [x] [P0] PRJ-001：项目已关联远端仓库 `git@github.com:tjyx111/simple-vpn-client.git`。
- [x] [P0] PRJ-002：已完成第一轮参考实现调研、架构拆分、实施优先级和测试策略记录。
- [x] [P0] PRJ-005：已基于 `gxsdn2`、`xacc-route-sdk2` 及其团队规范整理根级全局门禁 `AGENTS.md`；测试：文档链接、UTF-8、唯一状态台账和 `git diff --check`。
- [ ] [P0] PRJ-003：安装并固定 Rust 工具链，建立 Cargo workspace、格式检查、Clippy、测试和 Windows 构建门禁。
- [ ] [P0] PRJ-004：完成需求、兼容边界、失败策略和首版范围确认后开始生产实现。

当前仓库除 README 和本规划外尚无生产代码。所有功能项默认未实现。

## 2. 需求与参考基线

### 2.1 已知需求

1. 使用 Rust 实现。
2. 功能和数据面行为参考 `D:\code\xunlei\xacc-route-sdk2`。
3. VPN 线协议使用 `D:\code\lbsprivate\client-cpp` 中的 8 字节 legacy VPN header。
4. Windows 流量捕获使用 NetFilter SDK demo 驱动。
5. 每个功能必须有自动化测试；真实驱动、签名和外部服务验收不能替代自动化测试。

### 2.2 本次调研固定的参考版本

| 参考对象 | 基线 | 用途 |
| --- | --- | --- |
| `gxsdn2` | Git `2a850580ac1babd5031bd0d0779a42be3d4d61ce` | 单一功能台账、CI 等价验证、数据面热路径约束、提交/推送/Tag 门禁 |
| `xacc-route-sdk2` | Git `8232fb5d2ae2b2940128b981a056271598bf9e26` | 分层架构、策略、生命周期、Flow/Netstack、Windows NFAPI、完整功能台账 |
| `xacc-route-sdk2` 团队规范 | Git `67c949ef3bcaf69824d896d07dbd8360c289e139` | 需求与设计确认、人机职责、独立授权、审查、Tag 制品、灰度与回滚 |
| `client-cpp` | Git `b7e5427b877f0bddff7bf0c67a0912493ed206fd` 加调研时工作区内容 | 8 字节协议、NetFilter demo API、TCP/UDP/ICMP V1 设计 |
| NetFilter SDK | `nfsdk-demo-1.7.6.4` WFP C API x64 | 驱动、`nfapi.dll`、`nfapi.h`、回调与 post/injection API |

`client-cpp` 调研时存在未提交和未跟踪文件，因此实现前必须把采用的协议黄金样本、ABI 头文件版本和行为决策复制为本仓库内的可复现测试 fixture，不能长期依赖外部脏工作区。

### 2.4 全局门禁学习结论

根级 [AGENTS.md](../AGENTS.md) 是本仓库重新整理后的全局门禁，当前裁决为：

1. 本文件继续作为唯一功能状态台账，不增设 `feature.md` 或 `todo.md`。
2. 生产实现必须经过需求与设计明确确认；实现、commit、push、Tag、发布和回滚分别授权。
3. 数据面逐包路径必须有界、非阻塞、无外部 I/O；所有 queue、缓存、flow、session、packet bytes 和每轮工作量有硬上限。
4. 生产错误不得静默吞掉；连接在首次可能失败操作前建档并保留首个失败原因。
5. 每个功能必须具备正常、边界和失败路径自动化测试，测试通过不替代 Release 构建通过。
6. 当前批准 Git 身份是 `libenshuai <tjyx111@gmail.com>`，这是对两个迅雷内部参考仓库身份规则的项目级适配。
7. 当前仍处于设计待确认阶段；P0 工具链和验证脚本建立前，只能报告文档门禁结果。

### 2.3 8 字节 VPN 协议基线

多字节字段使用大端序：

| Offset | Size | 字段 | 语义 |
| ---: | ---: | --- | --- |
| 0 | 1 | `first_byte` | 高 4 位必须为 `0x9`；发送方规范值 `0x90` |
| 1 | 1 | `next_protocol` | `0x01` 表示完整 IPv4 包 |
| 2 | 2 | `flags` | 当前发送为 0；接收和返回路径保留未知值 |
| 4 | 4 | `session_id` | 32 位 session；配置 `next_node_id` 时低 16 位保存节点 ID |
| 8 | N | `inner_ipv4` | 一个完整 IPv4 包 |

发送格式：

```text
outer UDP payload = legacy_vpn_header || complete_inner_ipv4_packet
```

协议实现必须满足：

- 严格检查 magic、next protocol、IPv4 version、IHL、total length 和 MTU。
- 编码不得直接发送 Rust 原生 struct 内存布局。
- 解码后的 payload 可以零拷贝借用输入，但跨异步边界前必须转移到有所有权的 `PacketBuffer`。
- 回程需要保留收到的精确 8 字节 header，不得重建后丢失未知 flags 或低 nibble。
- 首版不在该 header 中新增版本、长度、加密、序号或防重放字段。

当前 C++ inline codec 对 IPv4 IHL/total length 的检查弱于协议设计文档。本项目暂按“线格式兼容、解析严格”规划；实现前必须把这一点确认为兼容决策。

## 3. 首版范围和非目标

### 3.1 建议首版范围

- Windows 10/11 x64。
- IPv4 TCP、UDP、ICMP 基础代理。
- 明确进程规则门禁；没有进程身份或规则未命中时强制直连。
- Direct、Block、Proxy 三种基础动作和连接级动作锁定。
- NetFilter SDK demo 驱动的安装/初始化、回调、verdict 和回灌。
- UDP 承载的 8 字节 legacy VPN tunnel。
- 本地 TCP 收口和 Rust 用户态 TCP/IP 栈。
- 有界连接状态、指标、日志、确定性启动/停止。

### 3.2 首版非目标，但保留在后续台账

- IPv6、Happy Eyeballs 和 IPv6 Fake-IP。
- HTTP forward proxy、HTTPS MITM、CDN。
- SOCKS5、KCP/FEC、多链路和智能双通道。
- Android、iOS、macOS PCAP/UTUN、Linux。
- 生产驱动授权、微软生产签名和自动更新。

## 4. 目标架构与所有权

```text
crates/
|-- foundation/
|   |-- core/                 # 公共错误、配置、类型、取消与 generation
|   `-- executor/             # ControlPlaneRuntime / DataPlaneRuntime
|-- observability/
|   |-- traffic-stats/        # 有界连接与流量统计
|   `-- diagnostics/          # 指标、日志、问题诊断包
|-- domain/
|   |-- config/               # 配置 DTO 和校验
|   `-- policy/               # 纯规则与动作语义
|-- filter/
|   |-- core/                 # 平台无关事件、decision、flow guard
|   `-- nfapi/                # Windows NFAPI ABI、callback、verdict、injection
|-- dataplane/
|   |-- packet/               # PacketBuffer、pool、IPv4/UDP/ICMP codec
|   |-- flow/                 # flow identity、binding、generation fence
|   |-- netstack/             # smoltcp TCP、UDP return、fragment/NAT
|   |-- tunnel/               # 8 字节协议和 UDP transport
|   |-- proxy/                # 后续 SOCKS5 等 stream proxy
|   `-- localserver/          # TCP 本地收口与 relay
|-- platform/
|   `-- windows/              # 服务、驱动资产、路由、进程查询、组合适配
`-- app/
    `-- client/               # 配置加载、组合根、启动停止和 CLI/service
```

所有权规则：

1. 组合根只创建一个控制面 Tokio runtime 和一个数据面 Tokio runtime；子模块不得创建私有 runtime 或原生工作线程。
2. NFAPI callback 线程属于外部 SDK。callback 必须短小、非阻塞，只允许同步快速判定或向有界队列 `try_send`。
3. callback 提供的 buffer 生命周期只覆盖本次调用。需要异步代理时，返回 callback 前必须复制到池化的 owned buffer。
4. 高频可变状态由 actor 或固定分片单 owner 持有；禁止每包全表扫描和共享粗粒度锁。
5. 所有 queue、packet bytes、pending flow、session、缓存和每轮处理量都有硬上限及 Full/Closed 指标。
6. flow 首次决定后粘性绑定 target、policy revision 和 runtime generation；旧代结果与回包必须拒绝。
7. handle 只提供提交或查询能力；owner 独占 stop、drain、join 和资源释放责任。

## 5. 核心数据流

### 5.1 启动和停止

```text
load + validate config
  -> create control/data runtimes
  -> prepare packet pool, policy snapshot and tunnel
  -> start local TCP server
  -> register/start driver and nfapi
  -> install self-process allow rule and TCP/UDP/ICMP rules
  -> wait all children Ready
  -> publish Running and admit traffic
```

停止按严格反序执行：停止新准入、停止过滤、等待 callback 静止、drain 数据面、关闭 tunnel/localserver、join 所有任务、最后销毁两个 runtime。启动任一步失败都必须回滚已经成功的前序资源，并保留首个具体错误。

### 5.2 TCP

```text
tcpConnectRequest
  -> process guard + policy
  -> Direct: allow original
  -> Block: block original
  -> Proxy: create session and rewrite to local listener
  -> localserver accepts by source port/session identity
  -> smoltcp converts byte stream to complete IPv4/TCP packets
  -> legacy VPN tunnel
  -> downlink packet -> smoltcp -> local stream -> application
```

TCP 决策在连接期间锁定。必须处理 SYN、重传、半关闭、FIN、RST、连接超时、发送缓冲背压和隧道失败；ordered bytes 不能因队列 Full 静默丢弃。

### 5.3 UDP

NetFilter 的 `udpCreated` 表示驱动第一次观察到 endpoint，不等同于应用刚调用 `socket()`。常见上行顺序为 `udpCreated -> udpSend -> udpClosed`，`udpCreated` 时本地端点可能仍为 `0.0.0.0:0`。

```text
udpCreated: 保存 endpoint id、PID、进程路径和当前 local
udpSend:
  -> nf_getUDPConnInfo 刷新 local
  -> process guard + flow/session decision cache
  -> Direct: nf_udpPostSend，且只调用一次
  -> Block: drop，记录稳定原因
  -> Proxy: copy options + payload into owned storage
            build complete IPv4/UDP packet + checksum
            send through legacy VPN tunnel
downlink:
  -> strict decapsulation
  -> lookup exact generation + flow/session
  -> extract UDP payload
  -> nf_udpPostReceive(endpoint id, remote, payload, saved options)
```

代理已消费的原始 datagram 绝不能再次 post。未知 session、旧 generation 或无精确回程路由的包必须丢弃，不能猜测最近 endpoint。

### 5.4 DNS

首版可把 DNS 当普通 UDP/TCP 流量。完整兼容阶段增加 transaction 级 DNS 状态：解析 question，按 domain/process 决策，Direct 原样 post，Block 生成明确空响应，Proxy 改写到代理 DNS upstream，并使用 endpoint、resolver、transaction id、domain 和 qtype 关联响应。缓存必须有 TTL、容量和 generation 隔离。

### 5.5 ICMP

```text
ipSend complete IPv4/ICMP packet
  -> 获取或确认进程归属
  -> Direct / Block / Proxy
  -> Proxy 原包进入 tunnel
downlink -> exact session -> nf_ipPostReceive
```

NFAPI IP packet callback 的可靠进程归属仍是阻塞设计问题。若不能证明来源命中明确进程规则，必须直连。不得仅因为安装过某个进程过滤规则，就把无法归属的 ICMP 包推断为该进程。

### 5.6 资源失败策略

- 未消费原包前：普通 Proxy 的准入失败可按已确认策略 fail-open 直连；`must_proxy` 必须 drop/fail-closed。
- 已消费原包后：隧道、队列或回程失败不得再回退原始路径，避免重复发送和乱序。
- flow/session 范围错误终止该连接；generation/共享状态损坏终止整代。
- 所有失败先发布有界、机器可读的 reason/counter，再执行 drop、fallback 或关闭。

## 6. 实施阶段与优先级

| 阶段 | 优先级 | 交付目标 | 阶段出口 |
| --- | --- | --- | --- |
| S0 | P0 | 需求、协议、驱动 ABI、失败策略和容量设计冻结 | 设计确认；黄金 fixture 可复现 |
| S1 | P0 | Cargo workspace、双 runtime、测试基础设施 | fmt、Clippy、test、Windows build 全通过 |
| S2 | P0 | PacketBuffer/pool、IPv4、8 字节 codec | codec golden、property/fuzz、池与 MTU 测试通过 |
| S3 | P0 | Driver trait、Fake NFAPI、真实 DLL backend 和生命周期 | ABI probe、callback、verdict、故障回滚测试通过 |
| S4 | P0 | 规则、process guard、flow/session、generation | 并发、容量、锁定、过期结果测试通过 |
| S5 | P0 | UDP tunnel 和本地 fake VPN peer | 不依赖公网的双向协议集成测试通过 |
| S6 | P0 | UDP 完整竖切及真实 demo driver smoke | Mock E2E 全自动；VM 驱动 smoke 可重复 |
| S7 | P1 | TCP localserver + smoltcp 完整竖切 | TCP 状态、背压、关闭、回程自动测试通过 |
| S8 | P1 | ICMP，或在进程归属不可解时保持显式 Direct | 归属与无泄漏自动测试通过 |
| S9 | P1 | 控制面、IPC、节点、DNS、观测完整化 | 合约、故障注入和本地 E2E 通过 |
| S10 | P2 | SOCKS5/KCP/HTTP/CDN、跨平台和生产交付 | 性能、soak、真实平台矩阵和回滚演练通过 |

## 7. 完整功能待办

### 7.1 项目基线与共享架构

- [ ] [P0] A01：建立 foundation、observability、domain、filter、dataplane、platform、app 单向依赖 workspace。测试：依赖边界源码约束、所有 crate 独立构建。
- [ ] [P0] A02：建立双 runtime、类型化 handle、有界 blocking admission、task group 和稳定指标。测试：饱和、panic、取消、deadline、join、禁止私有 runtime/线程的源码约束。
- [ ] [P0] A03：实现 generation prepare/ready/commit/replacement/rollback/stop 状态机。测试：每个阶段故障注入、旧代结果隔离、反序清理和重复启停。
- [ ] [P0] A04：建立 source-constraint tests，阻止平台层复制策略、协议、IPC、重连和生命周期逻辑。测试：对禁止依赖和禁止 API 的静态扫描。
- [ ] [P1] A05：建立参考行为追踪表，将 xacc 和 client-cpp 的每项采用、改写、延后、拒绝决定关联到测试。测试：台账完整性检查确保每个参考项都有状态、证据路径和至少一个自动化测试 ID。

### 7.2 配置、IP 地域与策略

- [ ] [P0] C01：实现配置、规则、action、redirect、protocol priority、NetFilter 和 tunnel 配置的强类型解析与校验。测试：合法/非法 JSON、默认值、未知字段和边界值。
- [ ] [P0] C02：实现 domain、IPv4、port、process、protocol 多维规则，明确优先级和 match trace。测试：表驱动 golden、重叠规则、通配、空白条件拒绝和容量拒绝。
- [ ] [P0] C03：实现 process guard、自进程豁免、私网/loopback guard、未命中直连和 Direct/Block/Proxy verdict。测试：guard 顺序及每个分支。
- [ ] [P0] C04：实现 action lock-in、规则 revision 原子发布和既有 flow 粘性。测试：运行时更新、并发读取、旧 flow 不换路、新 flow 使用新规则。
- [ ] [P1] C05：实现 IP/CIDR/地址段/端口段二分查询与可选 ip2region 地域符号。测试：边界、重叠、IPv4 golden、地区库失败。
- [ ] [P2] C06：实现 `INTEL_SAME_AREA`、`INTEL_BLOCK`、`INTEL_LOW_LATENCY` 及探测/回退语义。测试：节点矩阵、同区/跨区、无探测样本和回退耗尽。
- [ ] [P2] C07：实现 domain rule 转 IP rule、IP range TTL 和条件表达式。测试：过期、刷新、缓存隔离和错误表达式。

### 7.3 SDK 合约与 IPC

- [ ] [P1] I01：实现有界 IPC frame、session、request/callback 顺序、ACK-paced 流控、重连和关闭。测试：拆帧/粘帧、超限、ACK 丢失、重连和取消。
- [ ] [P1] I02：定义 `InitSdk`、`ExitSdk`、`StartAcc`、`StopAcc`、`AreYouOk`、`OnNetworkChange`、`UpdateAccToken`、`GetAccInfo`、`GetAccDetail`。测试：参数数目/类型、错误码、回调顺序和 JSON golden。
- [ ] [P1] I03：实现 Ready、状态变化、Token、消息 handler 等服务回调。测试：一次性、顺序、接收方消失和重入。
- [ ] [P2] I04：补齐节点延迟、智能 block/redirect、检测修复、游戏流量历史和远程调试 IPC。测试：分页、筛选、稳定错误码和兼容编码。

### 7.4 加速控制面

- [ ] [P1] L01：实现 StartAcc 配置解析、服务端补全、主/redirect target catalog 和不可变 AccelerationPlan。测试：多 target、缺失目标、重复 key 和服务失败。
- [ ] [P1] L02：实现节点 API、节点探测、UDP→可选 KCP→TCP 的有界 fallback、粘性选择和健康状态。测试：超时预算、部分失败、全部失败和恢复。
- [ ] [P1] L03：实现 access token 更新、进度、稳定错误码和状态快照。测试：更新竞态、过期 token、首错保留。
- [ ] [P1] L04：实现 Windows 网络变化时保留 logical generation、更新物理 route/socket 或安全 replacement。测试：重复事件、旧事件、变化中 stop 和回滚。
- [ ] [P2] L05：实现附加 CA 下载、验签、动态轮换和旧 generation 隔离。测试：签名错误、过期、并发更新和回滚。

### 7.5 包、Flow、Netstack 与 Tunnel

- [ ] [P0] D01：实现带 headroom 的池化 PacketBuffer、就地 prepend/consume 和包数/字节双容量批次。测试：生命周期、复用、容量、headroom、跨线程所有权和池耗尽。
- [ ] [P0] D02：实现 IPv4/TCP/UDP/ICMP 解析、构造、校验和、MTU 和畸形包拒绝。测试：golden、边界、property 和 fuzz。
- [ ] [P0] D03：实现五元组、flow instance/epoch、metadata、pending resolution、sticky binding 和 generation barrier。测试：同五元组重用、迟到结果、容量、顺序 flush。
- [ ] [P0] D04：实现 8 字节 legacy VPN codec、session/next-node 组合和精确 header 回显。测试：已知字节向量、未知 flags、错误 magic/protocol/长度、round-trip、fuzz。
- [ ] [P0] D05：实现 UDP tunnel transport、发送/接收 actor、超时、重连、回程路由和停止。测试：本地 fake peer、断网、错误 peer、乱序、旧 generation 和关闭。
- [ ] [P0] D06：实现 UDP 上行构包、decision cache、downlink 拆包和 NFAPI endpoint 回灌。测试：Direct/Block/Proxy、每包 exactly-once verdict、options 保留、未知 session。
- [ ] [P1] D07：实现 smoltcp TCP shards、localserver、complete-IP tunnel、Direct TCP 和有界背压。测试：握手、重传、双向字节、FIN/RST、半关闭、队列 Full 和超时。
- [ ] [P1] D08：实现 ICMP echo/session 和回灌；进程归属不能证明时 Direct。测试：id 匹配、checksum、未知 session、缺失身份和旧代。
- [ ] [P1] D09：实现 IPv4 分片重组、首片 metadata、TTL、容量、乱序和重叠片策略。测试：乱序、重复、重叠、缺片、超时、攻击容量。
- [ ] [P1] D10：实现 peer NAT、发送失败还原原视图和精确 target 关联。测试：双向改写、checksum、redirect 隔离和失败返还所有权。
- [ ] [P2] D11：实现 TCP/UDP 多 underlay、有界公平合并、去重、lane-local fault 和共享回包。测试：跨 lane 重复、单 lane 故障、公平性和容量。
- [ ] [P2] D12：实现 KCP/FEC、动态 parity、三 socket actor、reconnect 和可选双通道。测试：丢包矩阵、乱序、FEC golden、socket 黑洞、服务重启和性能基准。
- [ ] [P2] D13：实现 IPv6 数据面、Happy Eyeballs 和 IPv6 Fake-IP。测试：双栈决策、IPv6 checksum、fallback 和地址租约。

### 7.6 DNS、HTTP、CDN、SOCKS5

- [ ] [P1] N01：实现 UDP/TCP Proxy DNS codec、transaction correlation、CNAME、TTL/LRU、single-flight、源端口租约和 generation 隔离。测试：并发、端口冲突、错误 transaction、取消、缓存过期和重绑。
- [ ] [P1] N02：统一 NFAPI UDP、TCP DNS 和内部 DNS 的 transaction 级观测预算。测试：请求/响应配对、无响应、重复响应、容量和统计守恒。
- [ ] [P2] N03：实现 SOCKS5 TCP CONNECT、认证、可选 TLS、健康监督和 fallback。测试：RFC wire golden、认证失败、TLS 校验、超时和 fallback 耗尽。
- [ ] [P2] N04：实现 HTTP forward proxy、CONNECT、HTTPS MITM、证书 owner 和 HTTP/1.1 上游池。测试：keep-alive、chunked、超时、池容量、取消、证书错误。
- [ ] [P2] N05：实现 CDN 鉴权、请求头、回源、频控和 590–593 来源标记。测试：签名 golden、频控、禁止回源、取消和 header 兼容。
- [ ] [P2] N06：实现 Fake-IP DNS 租约、DNS 应答改写和精确 flow NAT。测试：租约冲突、过期、反向映射、跨 generation 和容量。
- [ ] [P2] N07：补齐 SOCKS5 UDP、直连 UDP 和其他 proxy protocol。测试：UDP associate、fragment 拒绝、回程关联和关闭。

### 7.7 Windows 驱动与平台组合

- [ ] [P0] W01：建立与 demo `nfapi.h` 对齐的 Rust C ABI、动态 DLL backend 和 C probe ABI 校验。测试：size/alignment/offset、calling convention、缺失符号和 Unicode 路径。
- [ ] [P0] W02：实现驱动注册、启动、`nf_init`、IP handler、规则安装、`nf_free` 和停止。测试：严格调用顺序、每步失败回滚、重复 start/stop 和 callback 静止屏障。
- [ ] [P0] W03：实现 TCP/UDP/IP callback 转换、进程路径解析、endpoint 状态和数据所有权复制。测试：Fake backend、短生命周期 buffer、未知 endpoint、状态竞争和 panic 隔离。
- [ ] [P0] W04：实现 allow/post/drop/block/rewrite verdict 及 UDP/IP injection。测试：每个输入恰好一个终态动作、地址转换、options 往返和下层错误传播。
- [ ] [P0] W05：实现 self-process allow、TCP connect、UDP connect/filter、ICMP-as-IP rules 和 loopback 防递归。测试：规则结构 golden、顺序、IPv4/IPv6-mapped loopback。
- [ ] [P1] W06：实现过滤会话真实健康检查，识别“服务 RUNNING、nf_init 成功但无 callback”的假健康并可控恢复。测试：模拟 callback 黑洞、释放失败、重置和状态可观测。
- [ ] [P1] W07：实现 Windows route lease、underlay 防递归、DNS cache 更新、网络变化和 crash journal 恢复。测试：路由冲突、重复 lease、进程崩溃恢复和切网。
- [ ] [P1] W08：实现 Windows service/CLI、优先级、配置、日志、资产发现和确定性停止。测试：参数优先级、服务控制、路径空格/Unicode 和 shutdown timeout。
- [ ] [P2] W09：实现正式驱动安装、升级、卸载、版本/权限/签名检查；demo 驱动仅供开发验证。测试：VM 快照矩阵、回滚和缺失驱动不影响非驱动命令。

### 7.8 可观测性、调试和上报

- [ ] [P1] O01：实现有界连接记录、first-failure-wins、进程、flow、target、route、协议和终态。测试：所有提前返回、容量 LRU、clear、并发和错误不被覆盖。
- [ ] [P1] O02：实现 runtime、queue、packet/byte、drop/reject、latency、generation 和 driver 指标快照。测试：静止点守恒、饱和、delta、代际清理和无高基数 label。
- [ ] [P1] O03：实现本地 debug HTTP API：状态、连接、规则、数据面和日志；访问控制与输出转义保持明确。测试：schema golden、分页、过滤、并发更新和 HTML/JSON 转义。
- [ ] [P2] O04：实现问题诊断包：manifest、日志、配置摘要、连接/指标快照和可选 PCAPNG；排除密钥和真实用户 payload。测试：可复现归档、容量、取消、清理和脱敏。
- [ ] [P2] O05：实现 generation-scoped debug 配置原子更新和远程调试。测试：既有 flow 不变、新 flow 生效、权限和过期。
- [ ] [P2] R01：实现 GACS `gxsdn/user/report` typed lifecycle、连接/ACK/traffic/down-loss/device/reconnect 上报。测试：wire/signature golden、重试、队列满和 final report。
- [ ] [P2] R02：实现 `accel_lan_report` schema、签名、有界 worker、ACK/keepalive/downstat 聚合。测试：固定序列、聚合守恒、replacement 和 endpoint 失败。

### 7.9 多平台扩展

- [ ] [P2] M01：抽象 host packet FD/virtual NIC 边界，确保业务策略和数据面不复制到平台 adapter。测试：共享核心一致性和源码约束。
- [ ] [P2] M02：Android JNI、VpnService RawFd、socket protect/bind、网络切换和 `.so` 交付。测试：host fake、JNI contract、Android instrumentation 和真机矩阵。
- [ ] [P2] M03：iOS/macOS RawFd/UTUN、Network.framework 路径、XCFramework 和 Objective-C facade。测试：C ABI、simulator/device build、network change 和真机矩阵。
- [ ] [P2] M04：macOS PCAP 局域网主机加速、ARP/ICMP、网卡发现、非阻塞 RX/TX 和 crash-safe 配置。测试：pcap adapter fake、frame golden、候选故障隔离和性能 A/B。
- [ ] [P2] M05：产品确认后增加 Linux TUN/NFQUEUE 或其他捕获组合。测试：network namespace E2E、路由恢复和发行包安装。

### 7.10 构建、测试与发布

- [ ] [P0] B01：固定 Rust toolchain、Cargo.lock、依赖来源和许可证审计。测试：离线/锁定构建和依赖策略检查。
- [ ] [P0] B02：CI 执行 fmt、Release Clippy、workspace all-features tests、Windows release build 和文档链接检查。测试：CI 脚本自身 fixture。
- [ ] [P0] B03：建立单元、contract/golden、组件、集成、故障注入、源码约束、fuzz、benchmark、driver VM E2E 分层体系。测试：为测试编排器提供成功、失败、超时、取消和清理 fixture。
- [ ] [P1] B04：建立可复现 Windows zip/MSI 或服务包、版本、哈希、SBOM 和安装/卸载验证。测试：干净 VM 安装、升级、回滚。
- [ ] [P1] B05：建立吞吐、CPU、RSS、queue、drop、P95/P99 延迟和长时 soak 门禁。测试：固定负载 Release A/B；结果关联 commit。
- [ ] [P1] B06：正式发布前完成 NetFilter 商业授权与 Microsoft 驱动签名流程；demo 驱动有连接/socket 数量限制，不能作为生产制品。测试：CI 校验驱动版本、哈希、签名链、授权制品来源和禁止 demo 资产进入生产包。

## 8. 自动化测试总策略

### 8.1 默认门禁

```powershell
cargo fmt --all -- --check
cargo clippy --release --workspace --all-targets --all-features -- -D warnings
cargo test --release --workspace --all-features
cargo build --release --workspace --all-features
```

### 8.2 测试层次

1. **单元测试**：纯 parser、codec、policy、state machine、checksum、容量算法。
2. **Contract/Golden**：8 字节协议、NFAPI ABI、配置、IPC、错误码及参考实现字节样本。
3. **组件测试**：Fake NFAPI、Fake clock、Fake process resolver、Fake tunnel、Fake node API。
4. **本地集成测试**：真实 loopback UDP/TCP socket 和本地 fake VPN server；默认不访问公网。
5. **故障注入**：每个新增失败分支至少覆盖单包失败、flow/session 终止、generation 失败、重试耗尽和关闭失败中适用的级别。
6. **Fuzz/property**：协议/IP/DNS/IPC parser 不 panic、不越界，并满足 round-trip 不变量。
7. **性能测试**：Release 模式固定包大小、连接数、队列和线程数，记录吞吐、CPU、RSS、队列占用、丢弃和尾延迟。
8. **真实驱动 E2E**：专用 Windows VM、管理员权限、串行执行、恢复快照；不进入普通无特权测试套件。

### 8.3 测试确定性要求

- 使用 fake clock 或显式事件，不用任意 sleep 等待状态变化。
- 异步测试必须等待实际消费、状态迁移或 join 完成，不能只断言入队成功。
- 需要全局 NFAPI owner 的测试串行化该 owner，而不是降低整个测试套件并发。
- 测试产生的驱动、socket、路由、临时配置和子进程必须在成功、失败、超时和取消时清理。
- 真实公网 echo、真实 VPN server、签名驱动和人工 UI 验收是附加证据，不能替代本地自动化断言。

## 9. 完成定义

一个功能只有同时满足下列条件才可标记完成：

1. 设计和兼容行为已记录，无关键歧义。
2. 正常路径、边界和新增失败分支都有自动化测试。
3. 生产错误没有被静默转成成功、EOF、默认值或无条件 fallback。
4. 队列、缓存、session、packet bytes 和诊断存储有明确硬上限。
5. 启动失败、停止、取消和 panic 不遗留任务、driver callback、socket、路由或临时文件。
6. 适用的 fmt、Clippy、测试、构建、集成和性能门禁通过。
7. 文档状态、外部行为和测试证据在同一提交中更新。

## 10. 已知风险与待确认事项

1. **Rust 工具链缺失**：调研机器当前 PATH 中没有 `rustc`/`cargo`；建议对齐 xacc 的 Rust `1.96.0`，安装后再开始 S1。
2. **参考基线不可复现**：client-cpp 关键协议和驱动文件位于脏工作区；必须先生成仓库内黄金 fixture。
3. **协议严格度差异**：C++ codec 与协议文档对 IHL/total length 的检查不一致，建议采用严格解析并显式确认。
4. **ICMP 进程归属**：当前没有已证明可靠的 NFAPI IP callback PID 映射；未解决前只能 Direct。
5. **demo 驱动限制**：过滤 TCP/UDP 数量达到 demo 上限后可能要重启系统才能恢复；测试必须使用可恢复 VM，生产需要正式授权。
6. **驱动签名**：最终用户环境需要微软认可的 kernel driver signing 流程。
7. **失败策略**：需要最终确认普通 Proxy 准入失败是否 fail-open，以及 `must_proxy`/Block 是否一律 fail-closed。
8. **控制面来源**：需要确认 VPN server、session ID、next-node ID、token 和节点列表来自静态配置还是服务端 API。
9. **完整 xacc 范围**：本文件已纳入其主要生产能力，但 P2 能力不应阻塞首版基础 VPN 客户端交付。

## 11. 更新记录

- 2026-10-14：重新学习 `gxsdn2`、`xacc-route-sdk2` 和固定团队规范，新增根级 `AGENTS.md`，明确需求/设计、数据面、测试、错误、Git、Tag、发布和停止门禁。
- 2026-10-14：创建项目进展台账；记录 Rust/NetFilter/8 字节协议基线、P0–P2 实施顺序、完整 xacc 功能映射及逐功能自动化测试要求。
