# 窗口切换结果与诊断日志

此流程用于选择一个已有窗口后的切换，包括 DingTalk 等应用。它不把系统返回的“请求获准”当作已经切到前台，也不为某个应用硬编码特例。

## 工作方式

1. 分配进程内唯一的 `request_id`，记录应用 Bundle ID、目标 PID、所选窗口 ID、WindowServer ID 和原前台 PID。
2. 释放 Winlane 的非激活面板及键盘焦点，再发送协作激活请求。`cooperative-result.allowed=true` **不是成功证明**。
3. 使用仅在请求存在期间运行的 50ms 定时器，在后台读取目标焦点窗口及窗口表面状态，主线程读取实际前台 PID 和应用活动状态。
4. 只有两次新鲜观测稳定匹配至少 50ms，才写入 `confirmed` 并提交本次选择的 MRU / 搜索偏好：
   - 前台 PID 等于目标 PID，应用仍在运行且 `active=true`；
   - AX 焦点窗口 ID 等于所选窗口，或其 WindowServer ID 与所选窗口相同（兼容 AX 对象重建）；
   - 已知的窗口存在及可见性状态不能为 false。
5. 若目标已在前台但窗口不对，后台尝试恢复 / 置前**这个指定窗口**；可选的 `AXMain` / `AXFocused` 写入失败不会阻止随后尝试 `AXRaise`。所有操作均写入阶段和实际 AX 错误码。
6. 请求被拒绝，或 250ms 后仍未完成交接时，可回退一次 LaunchServices。不开新应用实例、不选择任意兄弟窗口；完成回调的错误必须处理，完成回调本身仍不代表验证成功。
7. 每个阶段最多两次窗口置前尝试，整个确认流程 1.5 秒超时。确认定时器结束后立即停止，不增加空闲周期轮询。后台 AX 读取可能晚返回，但取消 / 超时后不会继续执行受控的后续 AX 写入。

窗口表面状态不可读取、但前台应用和精确焦点窗口已确认时，日志会保留 `on_screen=null`，校验结果为 `visibility_verified=false`；这不是屏幕像素级可见性验证。`on_screen=false` 则不能确认成功。

## 防止迟到操作抢焦点

新 Winlane 手势、关闭、设置窗口、新应用启动请求、退出，或用户切往其他前台应用，会取消旧请求。目标曾成为前台后又回到原应用，也按新的用户意图取消。

取消会撤销请求的原子令牌；后台任务在查找窗口后、每个属性写入前及 `AXRaise` 前再次检查令牌和截止时间。过期样本不会参与确认，旧请求结果不能提交或取消新请求。

LaunchServices 完成回调**不直接调用 AXRaise**。旧回调无法再投递时，仅记录 `late-completion` / `ignored=true`。已经提交给 macOS 的系统激活请求本身无法撤回；本流程停止的是 Winlane 的后续重试和窗口写入，不通过抢回焦点来“撤销”用户当前操作。

## 日志与隐私

使用现有日志位置和设置；默认：

```text
~/Library/Logs/Winlane/winlane.log
```

将日志等级保持为 **Info** 或 **Debug**。所有这些事件的组件为 `window-activation`；JSON 数据仍位于现有文本日志的 `data="..."` 中，不改变旧日志格式。

| 事件 | 意义 |
| --- | --- |
| `requested` | 发起选择后的激活请求 |
| `cooperative-result` | 系统是否允许请求；不可用它证明成功 |
| `observed` | 前台 / 活动状态 / 焦点窗口 / 表面状态变化 |
| `raise-requested` | 为已到前台的目标安排指定窗口置前 |
| `ax-step` | `AXMain`、`AXFocused`、`AXMinimized`、`AXRaise` 的结果，含错误码或跳过 / 取消原因 |
| `raise-result` | AX 操作返回；仍不等于确认成功 |
| `fallback-requested` | 使用一次 LaunchServices 回退 |
| `launchservices-result` | 回退回调结果；失败含 NSError domain / code |
| `confirmed` | 满足实际前台及窗口身份的稳定观测条件 |
| `failed` | 超时、目标退出 / 窗口关闭、回退失败等 |
| `cancelled` | 新手势或用户切换应用等取消原因 |
| `late-completion` | 已结束请求的回调，已忽略、不执行窗口操作 |

原始 AX 错误值例如 `-25204` 表示不能完成请求、`-25202` 表示失效元素。取消的内部标记不会伪装成 macOS AX 错误码。

日志不会记录窗口标题、聊天内容、搜索文本或剪贴板内容。后台 AX 任务的最后一条返回记录可能晚于 `cancelled`；它不等于在取消后重新安排了置前。

## 在本机核验 DingTalk

1. 保持 DingTalk 主窗口存在且未最小化，前台切到其他应用。
2. 从 Winlane 选择 DingTalk，观察是否实际到前台。
3. 等本次操作结束后，在仓库目录执行：

```sh
python3 scripts/check-window-activation-log.py \
  --bundle-id 5ZSL2CJU2T.com.dingtalk.mac
```

脚本只读日志，不操控应用。默认读取当前及上一个轮转日志；自定义日志位置请用 `--log /绝对路径/日志文件`，可以重复传入多个文件（旧文件在前）。`--request-id 123` 可检查指定编号在最近一个匹配 Winlane 进程中的请求。

脚本默认只检查当前正在运行的 Winlane 进程，避免重签 / 重启后拿旧进程的成功记录当作当前证据；可用 `--winlane-pid` 显式指定进程，或用数字 `--request-id` 查询历史。脚本以“Winlane 进程 PID + request_id”关联事件，防止重启后的编号复用串线。它会独立检查 `confirmed` 的前台 PID、活动状态、窗口身份、稳定时间和先前观测，不会只相信终态名称。

| 退出码 | 结果 |
| --- | --- |
| 0 | `confirmed` 的证据通过；同时输出可见性是否已验证 |
| 1 | 实际 `failed`，或日志声称成功但终态字段证据矛盾 / 违反协议 |
| 2 | 未找到请求、尚未结束、先前匹配观测缺失、日志丢弃或无法解析；不能下成功结论 |
| 3 | 请求正常取消；不等于成功，也不应归类为激活故障 |

重点对比：普通切换、应用返回 true 但前台 PID 不变、多窗口选中特定窗口、跨 Space、快速发起新手势、切往第三个应用。禁止将 `cooperative-result` 或 `raise-result` 单独当作实际成功率。

## 自动回归与模拟日志

```sh
cargo test --locked --test activation
python3 -m unittest discover -s scripts -p 'test_window_activation_log.py'
WINLANE_ACTIVATION_SMOKE=1 \
WINLANE_ACTIVATION_TEST_LOG=/绝对路径/window-activation-simulated.log \
  cargo test --locked --test native_panels
```

原生定向测试使用隐藏面板和模拟状态，不激活真实应用。它通过生产日志实现输出 `test_mode=true`、`example.activation-test` 的记录；这是状态机及日志协议验证，不是 DingTalk 实测。

对这些模拟记录可分别检查：`--bundle-id example.activation-test --request-id 1001` 应通过，1002 应为取消，1003 应为超时失败。不要用模拟日志宣称实际应用已成功到前台。


## AX 身份重建与列表缺失

AX 对象销毁或查找失败，不代表物理窗口关闭。保留最后已知窗口元数据，直到 WindowServer 证实该表面消失；重建后的焦点句柄会替换旧 CF 句柄，不丢弃对应的可见行信息。选择已知窗口时携带稳定的 WindowServer ID，避免 AX ID 改变后失去目标。没有实际失效缓存 token 的重复查询，不重置远程恢复扫描进度。

`window-discovery` 组件补充了以下 Info 事件，均不含窗口标题或聊天内容：

- `lookup-unresolved`：AX 重查未能解析目标，`metadata_preserved` 表示保底信息是否仍在。
- `retained-window-used`：AX 暂未发布窗口，但物理表面存在，保底窗口进入扫描结果。
- `ax-handle-rebuilt`：同一 WindowServer 表面的 AX 身份改变，旧 / 新 AX ID 和物理窗口 ID 可关联。
- `ax-destroyed-check`：销毁通知只触发异步物理存活核验，不立即删除行。
- `surfaces-without-ax`：系统有窗口表面，但没有可发布的 AX 或已知缓存，便于定位冷启动 / 其他 Space 的发现问题。
- `inventory-changed`：某应用实际缓存行 ID 的前后变化；`window_ids_after=[]` 表示该次确实清除了它的缓存行。
- `switch-list`：一次 Cmd+Tab 初始快照中，各应用 PID、Bundle ID 和行数；可直接检查 DingTalk 是否被纳入本次手势，而非把“激活成功”误当作“列表仍存在”。

跨 Space 或从后台切换后，应检查同一物理窗口是否还在，以及 `inventory-changed`、`switch-list` 是否仍包含 DingTalk。不要只用 AX 对象 ID 的改变或 `on_screen=false` 判定窗口关闭。


ad hoc 重签名可能使系统窗口控制权限失效。若 Winlane 显示“先允许 Winlane 控制窗口”，需要用户在 Device Control and Data Access / 辅助功能中重新启用当前安装路径的应用；这会阻止所有窗口发现，并不是只缺少某一个应用。校验器此时应报告当前进程证据不足，不能拿旧进程的 `confirmed` 充当新版验证。
