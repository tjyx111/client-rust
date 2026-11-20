# 可恢复 VM 驱动生命周期验收清单

状态：**脚本和清单已准备，尚未执行**。真实 demo 驱动操作需要管理员权限、可恢复 Windows VM 和单独授权。

关联设计：[Windows NFAPI 动态加载阶段设计](windows-driver-loader-design.md)、[项目进展台账](project-progress.md)。

## 1. 目的

验证 W02-a 的完整真实生命周期：

```text
加载 nfapi.dll
  -> 复制 netfilter2.sys
  -> nf_registerDriver
  -> 等待服务 RUNNING
  -> nf_init + nf_setIPEventHandler
  -> 启动清单
  -> 用户按 Enter
  -> nf_free
  -> 停止服务
  -> nf_unRegisterDriver
  -> 删除本次复制的 .sys
  -> FreeLibrary
```

脚本：`scripts/verify-vm-acceptance.ps1`。

## 2. 前置条件

1. Windows 10/11 x64 可恢复 VM，已创建快照。
2. 管理员权限。
3. 干净或已知的 `netfilter2` 状态：
   - 推荐先确认 `sc query netfilter2` 为 1060；
   - 若已存在同哈希文件，脚本按“复用、不删除”验收；
   - 若已存在服务或不同哈希文件，客户端应 fail-closed，退出 1。
4. 构建产物：

```powershell
cargo build --locked --release -p simple-vpn-client
```

5. 明确传入 vendor `nfapi.dll` 和 `netfilter2.sys` 的绝对路径。

## 3. 运行

从仓库根目录执行，保证 `logs/driver-checklist.log` 落在仓库日志目录：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-vm-acceptance.ps1 `
  -BinaryPath target\release\simple-vpn-client.exe `
  -NfapiDll D:\vendor\nfapi.dll `
  -DriverSys D:\vendor\netfilter2.sys `
  -ConfirmLabVm
```

`-ConfirmLabVm` 是显式确认：已经创建可恢复快照，并接受真实驱动安装/停止。

脚本不直接停止、注销或安装驱动；这些动作只由被测二进制执行。

## 4. 启动清单期望

按 Enter 前，stdout 和 `logs/driver-checklist.log` 应包含：

| item | 期望 |
| --- | --- |
| `service_netfilter2` | expected `4`（RUNNING），actual `4` |
| `service_start_type` | expected `1`（SYSTEM_START），actual `1` |
| `service_image_path` | 规范化后等于 `System32\drivers\netfilter2.sys` |
| `driver_sha256` | 源和目标哈希相同 |
| `nfapi_path` | 输入绝对路径存在 |
| `related_drivers` | 无其他相关驱动时 `NONE` |
| `related_driver_*` | 存在雷神/迅雷/VPN 等驱动时逐条 `ERR`，这是预期诊断 |
| `related_drivers_truncated` | 枚举超过 64 条时 `ERR` |

`ERR` 只记录，不改变退出码。

## 5. Enter 后关闭清单期望

| item | 期望 |
| --- | --- |
| `service_netfilter2` | expected `MISSING`，actual `MISSING` |
| `driver_file` | `copied=true` 时 `MISSING`；`copied=false` 时 `PRESENT` |
| `driver_sha256` | 仅在 `copied=false` 时复核，仍等于源哈希 |
| 退出码 | 0 |

## 6. 脚本自动断言

`scripts/verify-vm-acceptance.ps1` 捕获并断言：

1. 客户端退出码为 0。
2. 退出后 `netfilter2` 服务不存在。
3. 驱动文件状态：
   - 安装前文件哈希等于源 → 退出后仍存在且哈希不变；
   - 其他情况 → 退出后文件不存在。
4. 相关驱动集合（名称/状态/启动类型/路径）在测试前后无变化。
5. `logs/driver-checklist.log` 同时包含 `phase=startup` 和 `phase=shutdown`。
6. 生成 `logs/vm-acceptance/vm-acceptance-<时间戳>.txt` 报告。

相关驱动的 `ERR` 数量会写入报告，但不单独判失败；集合发生变化才失败。

## 7. 附加手工场景

| 场景 | 期望 |
| --- | --- |
| 预存同哈希 `netfilter2.sys`、无服务 | 复用文件，`copied=false`，退出后文件保留 |
| 预存不同哈希文件 | 客户端拒绝覆盖，退出 1，原文件不变 |
| 预存 `netfilter2` 服务 | 客户端拒绝接管，退出 1 |
| 非管理员运行 | `nf_registerDriver` 失败，清理并退出 1，无残留文件 |
| stdin 提前关闭 | 清理路径执行，输出关闭清单 |
| 其他加速器驱动运行 | 清单出现 `related_driver_* ERR`，退出码不变 |

## 8. 回滚与恢复

1. 测试结束恢复 VM 快照，不依赖手工清理。
2. 若快照不可用，按 vendor 顺序人工恢复：

```text
nf_free（若已初始化）
sc stop netfilter2
nfregdrv.exe -u netfilter2
删除 System32\drivers\netfilter2.sys（仅当确认是本 SDK 复制）
```

3. 恢复后重新确认 `sc query netfilter2` 为 1060、驱动文件状态符合预期。

## 9. 证据归档

保留：

- `logs/vm-acceptance/vm-acceptance-<时间戳>.txt`；
- `logs/vm-acceptance/client-output.txt`；
- `logs/driver-checklist.log`；
- 测试前后 `sc query netfilter2` 和驱动文件哈希。

报告需记录 VM 快照 ID、Git SHA、构建产物路径和测试时间；真实 VM 证据是附加证据，不替代离线自动化测试。
