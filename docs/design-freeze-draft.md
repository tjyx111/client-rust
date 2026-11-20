# 后续阶段需求与接口冻结草案

状态：**设计草案，不属于 S0，等待后续逐阶段确认**

版本：`design-draft-v1`

日期：2026-10-14

本文件保存首个 Windows VPN 客户端纵向切片的需求、8 字节线协议、NetFilter ABI、失败策略和容量预算草案。按 2026-10-14 最新阶段定义，S0 只负责 Rust 环境和自动化测试准备；本文内容必须在后续功能阶段按用户要求逐项确认，不能整体视为已经冻结。

机器可检查的协议、ABI 和容量草案位于 `fixtures/design-freeze/`，统一执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-design-freeze.ps1
```

若本文件与 fixture 冲突，以验证通过的 fixture 为事实基线并立即修正文档；不得在生产代码中自行选择第三种行为。

## 1. 来源与可复现性

| 来源 | 固定基线 | 用途 |
| --- | --- | --- |
| `gxsdn2` | Git `2a850580ac1babd5031bd0d0779a42be3d4d61ce` | 有界数据面、队列 Full/Closed、渐进清理和性能证据 |
| `xacc-route-sdk2` | Git `8232fb5d2ae2b2940128b981a056271598bf9e26` | runtime owner、flow/generation、错误与连接观测、Windows 平台边界 |
| `client-cpp` | Git `b7e5427b877f0bddff7bf0c67a0912493ed206fd` 加 2026-10-14 工作区内容 | 8 字节协议和 NetFilter WFP demo 适配行为 |
| NetFilter SDK | `nfsdk-demo-1.7.6.4`，WFP C API x64 | `nfapi.h`、`nfevents.h`、`nfdriver_data.h`、`nfapi.dll`、`netfilter2.sys` |

`client-cpp` 在取证时存在大量未提交、删除和未跟踪内容，所以它不能作为持续构建依赖。本草案把候选事实、源文件 SHA-256、ABI 布局和协议向量复制为本仓库 fixture。后续参考工作区变化不得自动改变本项目行为；采用任何内容前仍需逐阶段确认，并同步更新 fixture、兼容性说明和自动化测试。

## 2. 首版需求冻结

### 2.1 交付范围

- 目标系统为 Windows 10/11 x64，用户态程序使用 Rust；NetFilter 厂商驱动和 `nfapi.dll` 保持原制品。
- 首个完整版本支持 IPv4 TCP、UDP 和 ICMP；第一条可交付竖切先完成 UDP，再完成 TCP 和 ICMP。
- 规则动作固定为 `Direct`、`Block`、`Proxy`；每个 flow 首次决策后锁定动作。
- 捕获边界使用 NetFilter SDK `nfsdk-demo-1.7.6.4` WFP C API x64 demo 驱动，仅用于开发与验证。
- 代理承载使用 UDP tunnel，payload 为 8 字节 legacy header 加完整 IPv4 包。
- 候选首版控制面使用本地强类型配置提供 VPN endpoint、session ID、next-node ID、token 和规则；具体落地阶段等待逐步实施时确认。
- 客户端提供确定性的 start、stop、restart、网络变化和故障回滚；所有后台任务、callback、socket、route 和驱动状态都有唯一 owner。
- 所有产品功能必须有不依赖公网的自动化测试；真实驱动 VM 测试属于附加证据。

### 2.2 首版非目标

- IPv6、Happy Eyeballs、IPv6 Fake-IP。
- SOCKS5、HTTP forward proxy、HTTPS MITM、CDN、KCP/FEC、多链路和智能双通道。
- Android、iOS、macOS、Linux 捕获适配。
- 生产驱动授权、Microsoft 驱动签名、安装器和自动更新。
- 在 8 字节 legacy header 内新增版本、长度、加密、认证、序号或防重放字段。

### 2.3 首版验收条件

1. Fake NFAPI 下 Direct、Block、Proxy 对每个输入恰好产生一个终态 verdict。
2. 本地 fake VPN peer 可完成 UDP 双向转发，不访问公网。
3. 8 字节协议的 canonical、兼容、错误和 round-trip fixture 全部通过。
4. start/stop/restart、队列满、tunnel 断开、驱动初始化失败和迟到 generation 结果均有自动化测试。
5. 数据面无外部 I/O、无无界队列、无随 flow 数增长的全表扫描。
6. Release fmt、Clippy、测试和 Windows build 通过；真实 demo driver smoke 在可恢复 VM 中可重复。
7. demo 驱动不会进入生产发布包；没有授权和签名链时不得声称可生产发布。

## 3. 8 字节 legacy VPN 协议冻结

所有多字节整数使用网络字节序（大端）：

| Offset | Size | 字段 | 冻结语义 |
| ---: | ---: | --- | --- |
| 0 | 1 | `first_byte` | 高 4 位必须为 `0x9`；canonical encoder 固定发送 `0x90` |
| 1 | 1 | `next_protocol` | `0x01` 表示完整 IPv4 包；其他值拒绝 |
| 2 | 2 | `flags` | canonical encoder 发送 0；合法入站的未知值必须随该包精确保留，以供要求回显的路径使用 |
| 4 | 4 | `session_id` | 32 位大端；`next_node_id != 0` 时结果为 `(session_id & 0xffff0000) | next_node_id` |
| 8 | N | `inner_ipv4` | 一个且仅一个完整 IPv4 包 |

严格解析规则：

- datagram 至少为 28 字节；空指针/空 buffer 在 Rust API 中以空 slice 表示并拒绝。
- `first_byte & 0xf0 == 0x90`；低 4 位不是版本字段，不得擅自解释。
- inner version 必须为 4，IHL 必须在 20 字节到实际 payload 长度之间。
- IPv4 `total_length` 必须不小于 IHL，且必须等于 datagram 中 inner payload 的实际长度；禁止尾随字节。
- 最大协议长度为 IPv4 的 65535 字节；运行时还要满足配置的 inner MTU。
- 编码不得直接发送 Rust struct 内存；解码可借用 slice，但跨异步边界前必须转为 owned `PacketBuffer`。
- 回程关联保存收到的精确 8 字节 header，不能重建后丢失低 nibble 或未知 flags。
- IPv4 header/L4 checksum 的具体校验在 codec 阶段完成；当前草案 fixture 包含一个有效 IPv4 checksum 样本。

与 C++ 当前实现的有意差异：C++ 只验证最小长度、magic 高 nibble、protocol 和 IPv4 version；Rust 版本冻结为严格检查 IHL 和 `total_length`。这是“线格式兼容、畸形输入更严格”的明确选择。

## 4. NetFilter 驱动 ABI 冻结

### 4.1 制品选择

- 只采用 `nfsdk-demo-1.7.6.4/nfsdk/wfp`，不混用 `sockfilter` 变体。
- 目标为 C API、x64、`__cdecl`，Rust target 预期为 `x86_64-pc-windows-msvc`。
- 生产代码动态加载 `nfapi.dll`，必须先验证全部必需 symbol，缺失任一 symbol 都不得部分启动。
- `ENDPOINT_ID` 固定为 64 位无符号整数；vendor struct 使用 `#pragma pack(1)`，Rust 对应类型必须 `#[repr(C, packed)]` 或通过等价的逐字段解码隔离未对齐访问。
- callback function table 在 x64 下为 16 个函数指针、128 字节；IP callback table 为 2 个函数指针、16 字节。
- callback panic 绝不能跨 FFI；callback 输入指针只在本次调用有效，异步处理前必须完成有界复制和 admission。

### 4.2 候选 ABI 结构

机器可读的候选 size、offset 和字段宽度位于 `../fixtures/design-freeze/netfilter-abi-x64.json`。核心 size 为：

| 类型 | x64 size | align |
| --- | ---: | ---: |
| `NF_RULE` | 83 | 1 |
| `NF_TCP_CONN_INFO` | 67 | 1 |
| `NF_UDP_CONN_INFO` | 34 | 1 |
| `NF_UDP_CONN_REQUEST` | 66 | 1 |
| `NF_UDP_OPTIONS` 固定前缀加 `options[1]` | 9 | 1 |
| `NF_IP_PACKET_OPTIONS` | 22 | 1 |
| `NF_EventHandler` | 128 | 1 |
| `NF_IPEventHandler` | 16 | 1 |

当前 MSVC C probe 已直接包含固定 vendor header，对 size、alignment、offset、常量和 calling convention 做验证；后续 Rust ABI 阶段还必须增加 Rust/C 双边 contract test 和真实 DLL symbol 检查。

### 4.3 生命周期和回调顺序

启动顺序固定为：验证资产与权限 → 注册/启动驱动 → 装配 callback table → `nf_init` → 设置 IP callback → self-process allow 规则 → TCP/UDP/ICMP 规则 → ready。任何一步失败都按逆序回滚。

停止顺序固定为：停止新 admission → 删除/禁用规则 → 等待 callback 静止屏障 → `nf_free` → 清理 endpoint 状态 → 停止驱动（仅由本进程拥有时）→ join task。重复 stop 必须幂等。

## 5. 失败策略冻结

| 场景 | 冻结行为 |
| --- | --- |
| 未命中规则、进程身份未知、自进程、loopback/私网 guard | `Direct` |
| 显式 `Block` | fail-closed，禁止 fallback |
| 普通 `Proxy` 在首包被消费前准入失败 | fail-open 到 `Direct`，记录稳定 reason |
| `must_proxy` 准入失败 | fail-closed，Block/drop，不得泄漏到直连 |
| 已锁定 Proxy 的 flow 发生 tunnel/queue/peer 故障 | 不允许中途改直连；丢弃当前包并终止或重置该 flow |
| callback 转换失败且原包尚未消费 | 普通流重新 post 原包；`must_proxy` 仅在已有可证明身份/规则时阻断，否则采用未知身份 `Direct` |
| 驱动注册、`nf_init` 或规则安装失败 | 整次启动失败并逆序回滚；非驱动 CLI/诊断仍可用 |
| 数据面队列 Full | 不阻塞；新普通 Proxy 在可安全回退时 Direct，`must_proxy` 拒绝，既有 Proxy flow 丢包/终止 |
| 数据面队列 Closed | generation 失败；拒绝新代理并清理关联 flow |
| VPN peer 断开 | 有界重连；预算耗尽后关闭既有 Proxy flow，新普通流可 Direct，`must_proxy` Block |
| malformed/未知 session/迟到 generation 下行 | drop 并计数，不回灌、不影响新 generation |
| 配置、协议或 ABI 不合法 | 启动失败，不用默认值掩盖 |
| shutdown deadline 耗尽 | 返回失败终态并记录遗留 owner；不得报告成功 |

错误记录采用 first-failure-wins。每个连接在第一次可能失败的操作前建档，后续 cleanup 错误作为附加信息，不能覆盖根因。

## 6. 容量设计冻结

所有值只是 `design-draft-v1` 候选，不是 S0 决策或性能承诺。每项容量会在对应功能进入实现前单独确认；提高 hard max 或改变耗尽动作属于设计变更，在 hard max 内调整默认值也必须有 Release A/B 证据。

| 资源 | 默认值 | hard max | 耗尽行为 |
| --- | ---: | ---: | --- |
| inner MTU | 1400 bytes | 9000 bytes | 超限拒绝，不分配 |
| 单 inner IPv4 协议长度 | 65535 bytes | 65535 bytes | parser 拒绝 |
| PacketBuffer 池总字节 | 64 MiB | 256 MiB | admission 拒绝，不临时堆分配绕过 |
| callback→数据面队列 | 4096 packets / 8 MiB | 16384 / 32 MiB | `try_send` Full 策略，不阻塞 |
| tunnel 上行队列 | 4096 packets / 8 MiB | 16384 / 32 MiB | 既有 Proxy flow 丢包/终止 |
| TCP flow | 8192 | 65536 | 新 flow 按规则 fail-open/fail-closed |
| UDP session | 16384 | 131072 | 新 session 按规则 fail-open/fail-closed |
| pending resolution | 4096 | 16384 | 拒绝新 pending，不扫描旧表 |
| decision cache | 65536 | 262144 | LRU 淘汰，只影响新 flow |
| IPv4 重组 | 2048 sets / 16 MiB / 15 s | 8192 / 64 MiB / 30 s | drop 最旧/超时集合并计数 |
| DNS cache | 16384 | 65536 | LRU 淘汰 |
| 连接诊断记录 | 10000 | 50000 | LRU 淘汰已终止记录，活跃记录另受 flow cap 约束 |
| 单 actor 每轮预算 | 256 events 或 1 MiB | 1024 或 4 MiB | yield，保持公平 |
| tunnel 重连 | 5 次、总计 30 s | 10 次、总计 60 s | 预算耗尽进入失败终态 |
| shutdown | 总计 10 s | 30 s | deadline 耗尽返回失败 |

每个双容量队列必须同时检查元素数和字节数。指标至少包含 current、limit、high-watermark、admission rejected、drop、closed 和 wait/yield 次数。

## 7. 自动化证据和变更规则

草案自动化检查验证：

- canonical encoder、next-node 组合、兼容低 nibble/flags 保留、严格错误向量。
- ABI source hash 格式、x64 packed size/offset 无重叠、callback 数量和必需 DLL symbol 清单。
- 所有容量都有正的默认值、hard max、耗尽行为和指标；默认值不得超过 hard max。
- 本文件、fixture 和唯一项目状态台账的链接存在，且不存在第二份状态台账。

某项设计逐阶段确认后，修改对应文档或 fixture 必须：

1. 在 `docs/project-progress.md` 增加或更新对应功能项。
2. 说明兼容性、数据面、资源和失败策略影响。
3. 更新 golden/ABI/capacity 自动化测试。
4. 由用户重新明确确认受影响设计，之后才可修改生产实现。

## 8. 后续逐阶段确认方式

当前已具备：候选需求范围、协议 fixture、ABI manifest、失败矩阵、容量矩阵和独立验证脚本。这些材料用于减少后续重复调研，不代表用户已经接受其中的产品决策。

后续每进入一个功能阶段，只确认与该阶段有关的条目，包括但不限于：

- 普通 Proxy 首次准入失败允许 fail-open 到 Direct。
- `must_proxy` 与显式 Block 始终 fail-closed。
- 已锁定 Proxy 的 flow 发生故障后不允许中途切换到 Direct。
- 首版控制输入来自本地强类型配置，远程控制延后到 S9。
- 默认 inner MTU 是否为 1400，以及采用哪些 `design-draft-v1` 容量值。
- 采用严格 IPv4 IHL/total-length 校验，即使比当前 C++ decoder 更严格。
