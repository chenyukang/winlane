# Winlane

用 Rust 和 AppKit 实现的 macOS 窗口搜索工具。按下 **Control + Option + Space（⌃⌥Space）**，输入应用名或窗口标题，再按 Enter 切换到选中的窗口。

当前版本提供搜索面板和菜单栏入口，支持按多个词、不连续字母以及单词首字母匹配。普通窗口、对话框和最小化窗口会在应用支持辅助功能接口时出现在列表中。运行中的普通应用即使没有可用窗口，也会保留在列表中，只显示图标与应用名称，按 Enter 激活该应用。能读取到窗口标题时，显示应用名与窗口标题；底部显示匹配项总数。没有可操作窗口的应用不能执行窗口最小化或复制标题。

## 构建与启动

需要 macOS、Rust 工具链和 Xcode Command Line Tools。目标系统为 macOS 14 或更新版本；当前在 Apple Silicon 机器上构建 ARM64 版本，尚未验证 macOS 14 实机兼容性或提供通用二进制。

在项目目录运行：

```sh
./scripts/build-app.sh
```

首次构建前，先完成下面的“开发签名”设置。脚本执行锁定依赖的 release 构建，生成 `dist/Winlane.app`，并使用固定证书签名。它不会安装、启动应用或申请辅助功能权限。需要调试构建时使用：

```sh
./scripts/build-app.sh --debug
```

构建后，退出正在运行的 Winlane，将 `dist/Winlane.app` 复制到固定位置 `~/Applications/Winlane.app`，再打开该位置的应用。后续构建始终更新同一位置，避免同时运行多个副本。运行期间菜单栏会显示 Winlane 入口；退出请使用菜单中的“退出 Winlane”。

应用名称为 Winlane；为沿用更名前的授权与设置，内部应用标识仍固定为 `app.windowlane.desktop`，设置存储键和签名证书也保持不变。从旧版升级时，退出 Windowlane，用 `Winlane.app` 替换已安装的旧应用，避免保留两个运行副本。如果存在 `resources/AppIcon.icns`，打包时会自动带上图标。产物为当前 Rust 工具链的主机架构版本，没有公证。

## 开发签名

ad hoc 签名（`codesign --sign -`）默认用可执行文件的哈希标识应用，重新编译可能导致辅助功能授权失效。开发时改用同一张证书签名，让 macOS 通过稳定的签名要求识别后续版本。debug 和 release 构建使用相同证书和应用标识，可以共用授权。

本机开发可以使用自签名的代码签名证书，无需购买 Apple Developer 会员。按 Apple 的 [Code Signing Guide](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html) 创建一次：

1. 打开 **Keychain Access（钥匙串访问）**，选择 **Keychain Access → Certificate Assistant → Create a Certificate…**。
2. Name 填 `Windowlane Development`，Identity Type 选 **Self Signed Root**，Certificate Type 选 **Code Signing**。
3. 勾选 **Let me override defaults**，将有效期设为合适的开发周期，例如 3650 天；其余保持默认，保存到 **login** 钥匙串。不需要将它设为所有用途的 Always Trust。
4. 运行 `./scripts/build-app.sh`。第一次使用私钥时，系统可能询问是否允许 `/usr/bin/codesign` 使用它；核对证书名和请求程序后按需允许。

证书与私钥留在钥匙串中，不存入项目，也不要每次构建重新创建。构建脚本会查找这个名称对应的唯一签名身份；缺少证书或存在同名歧义时，在编译前报错，不自动退回 ad hoc 签名。也可使用已有的 Apple Development 证书：

```sh
security find-identity -p codesigning
WINLANE_SIGNING_IDENTITY='证书的完整名称或 40 位 SHA-1 指纹' ./scripts/build-app.sh --debug
```

旧环境变量 `WINDOWLANE_SIGNING_IDENTITY` 仍兼容，新变量优先。

从旧 ad hoc 版本迁移到证书签名后，需要为固定安装位置的新版本重新授权一次。之后保留同一证书、应用标识和安装位置，正常重编译应沿用授权。更换证书、清除系统隐私设置等仍可能要求重新授权。自签名证书适合本机开发，公开分发应另行配置 Developer ID 签名和公证。

仅需临时构建、且接受重新授权时，可显式运行 `./scripts/build-app.sh --adhoc`；不要用这个产物覆盖已授权的日常开发版本。

## 辅助功能授权

读取其他应用的窗口标题并切换窗口需要 macOS 的“辅助功能”权限，请手动授权：

1. 打开“系统设置 → 隐私与安全性 → 辅助功能”。在 macOS 27 中，该权限页面名称为 “Device Control and Data Access”。
2. 点击添加按钮，选择实际运行位置的 `Winlane.app`，并打开开关。
3. 重新呼出 Winlane 搜索面板，自动读取窗口列表。已授权后不再显示“刷新”按钮。

授权对象应是固定安装位置的 `.app`。直接运行 `cargo run` 或可执行文件不会经过上述证书签名流程，系统也可能把权限归到启动它的终端。更换签名身份后，如果旧授权失效，请在系统设置里移除旧项，再添加当前的 `.app`。

未授权时可以点击“查看演示”，用示例窗口体验搜索和选择。演示模式中的 Enter 和鼠标选择只更新提示，不读取、激活或修改真实窗口；点击“返回真实窗口”退出演示。

## 操作

| 按键或操作 | 功能 |
| --- | --- |
| Control + Option + Space（默认，可自定义） | 打开或收起搜索面板 |
| Command + Tab（在设置中启用） | 呼出搜索；面板打开时选择下一项 |
| Command + Shift + Tab（启用 Command + Tab 后） | 反向选择 |
| 输入应用名、标题或多个关键词 | 筛选窗口 |
| ↓ / Tab | 选择下一项 |
| ↑ / Shift + Tab | 选择上一项 |
| Enter / 点击结果 | 切换到所选窗口；最小化窗口会尝试恢复 |
| Esc | 关闭面板 |
| Command + 1…9 | 直接切换到当前结果中的第 1…9 个窗口 |
| Command + M | 最小化或恢复所选窗口 |
| Command + H | 隐藏所选应用；之后选择它的窗口并按 Enter 可重新打开 |
| Command + Shift + C | 复制所选窗口标题到剪贴板 |
| Command + R | 手动重新读取窗口列表；每次呼出面板也会自动更新 |
| Command + , / 设置 | 打开原生设置窗口 |
| 仅当前应用 | 只显示呼出面板前正在使用的应用；演示模式以 Safari 为例 |

窗口操作也可以从搜索框下方的“窗口操作”菜单执行。操作快捷键只在搜索面板可见且有结果时启用，在设置界面中不会操作其他应用。Command + W 可关闭当前窗口。

呼出后搜索框直接接收输入；松开 Command 后可继续搜索，仍按住 Command 时也可用 ↑ / ↓ 选择。中文输入法正在组合文字时，方向键、Tab、Enter 和 Esc 交由输入法处理。候选列表只接收 Accessibility 返回的标准窗口（`AXWindow` / `AXStandardWindow`），不添加应用占位项，也不把窗口菜单项当作窗口。工具面板、对话框、标签页以及类型无法确认的对象不会单独列出。同一应用的不同真实窗口分别保留，即使标题相同；可按 VS Code 项目名等窗口标题搜索。最小化窗口由设置中的“显示最小化窗口”控制。

再次输入相同搜索词时，之前通过 Winlane 选择过的窗口会获得排序优先级。默认排序下，空搜索也优先显示通过 Winlane 选择过的窗口。这个记录只覆盖应用内的选择，不追踪从 Dock、鼠标或其他工具进行的切换，因此不是系统全局的最近使用顺序（MRU）。

## 设置与登录启动

设置窗口支持自定义全局快捷键、按最近切换/应用名/标题排序、跟随系统/浅色/深色外观、是否显示最小化窗口，以及排除指定应用。排除规则按列表中显示的完整应用名匹配，忽略大小写，用中英文逗号或换行分隔。

支持将呼出快捷键设为 Command + Tab：只勾选 Command，按键选择 Tab，再保存。其他快捷键需包含 Control 或 Option，可搭配 Shift、Command 和所列按键。新组合注册成功后才替换旧组合；被占用或不合法时保留原设置。修改列表与快捷键后点击“保存设置”，下次启动仍然有效。“恢复默认设置”先填入默认值，保存后才应用。

启用 Command + Tab 需要先授权 Winlane。它会在运行期间接管该组合，松开 Command 后保持面板打开，方便继续输入搜索；Enter 切换，Esc 取消。退出 Winlane 或改回其他快捷键后，会移除这项接管，不修改系统快捷键配置。若 Contexts 等工具也接管同一组合，需要避免同时启用；安全输入或更早拦截按键的其他工具可能影响触发。设置中会显示是否成功启用；已经保存 Command + Tab 的用户重新授权后，应用会自动尝试恢复监听。

“登录时自动启动”独立于保存按钮，默认关闭，点击后立即通过 macOS 的 [SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice) 更新。若系统要求允许，界面会显示等待状态，点击“管理登录项”自行完成。已获得辅助功能权限时，应用启动后只保留菜单栏入口；未授权时才自动显示权限提示。按全局快捷键、点击菜单栏入口或再次打开正在运行的应用，仍可显示搜索面板。旧版本保存的“启动时显示搜索面板”设置不再生效。恢复默认设置不会更改系统的登录启动状态。

## 数据与权限

窗口标题、搜索词和选择记录仅保存在进程内存中，退出后清空，不写入历史文件或上传。主动使用“复制窗口标题”时，所选标题会写入系统剪贴板。快捷键、外观、筛选等设置通过 NSUserDefaults 保存在应用的本机偏好中。应用通过 Accessibility 读取窗口。普通快捷键使用 Carbon；Command + Tab 使用需要辅助功能授权的 CoreGraphics 事件过滤，只比较按键和修饰键，不记录输入内容，也不采集窗口截图。事件处理仅拦截 Command + Tab、Command + Shift + Tab，以及这次呼出过程中按下的 Command + Esc；其他按键原样传递。

## 当前边界

- 窗口枚举同时读取应用的窗口列表、主窗口和焦点窗口，并与系统窗口清单核对，补齐其他 Space 上的独立窗口。普通列表仍未返回的窗口，通过可选的 macOS AX 私有接口查找真实窗口对象，每个应用每轮扫描约 250 毫秒，刷新时可继续扫描；系统接口变化时这一补充能力可能不可用。不会通过隐藏应用或切换桌面来枚举窗口，也不把菜单项或标签页补成候选。
- 跨 Space 切换受 macOS“桌面与程序坞 → 调度中心 → 切换到某个应用程序时，切换到包含该应用程序打开窗口的空间”设置及目标应用行为影响。Winlane 不移动窗口所属的 Space。可参考 [Apple 的多空间说明](https://support.apple.com/guide/mac-help/mh14112/mac)。
- 部分应用没有完整实现 Accessibility 接口，可能不提供窗口、标题或恢复与聚焦操作。读取超时会延长等待并重试一次，扫描在最多四个后台线程中进行。仍无法读取或无法确认类型的窗口不会进入列表；仅扫描 macOS 标记为普通应用（Regular）的进程，排除菜单栏附件与后台辅助进程。窗口关闭后需要刷新列表。
- 目前没有常驻侧栏，也不跟踪系统全局的窗口使用顺序。

## 参考项目

- [Contexts](https://contexts.co/)：窗口搜索和键盘切换的产品参考。
- [青简 Qingjian](https://github.com/qingjian-team/qingjian)：以 Rust 实现原生桌面能力的项目参考；青简本身是输入法。

Winlane 的实现为独立编写，没有复制青简的源码。

## 开发检查

```sh
RUSTC_WRAPPER= cargo test --locked
RUSTC_WRAPPER= cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

搜索测试使用内存中的样例数据。真实窗口枚举、最小化恢复和跨 Space 切换需要在授权后的 macOS 桌面环境中人工验证。
