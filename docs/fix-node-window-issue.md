# 修复手机控制问题：黑窗 + 回复不完整

## 问题 1：黑色命令窗口闪现

### 问题描述

当启用 VarSwitch 手机控制功能（飞书/QQ/微信桥接）时，以及在安装软件过程中，会弹出或闪过黑色的命令行窗口。关闭这些窗口后，手机连接会断开。

### 根本原因

在 Windows 平台上，使用 `Command::new()` 启动子进程时，默认会创建一个可见的控制台窗口。VarSwitch 中有多处需要启动子进程：

**手机控制功能：**
1. **飞书消息桥** (`start_lark_bridge`) - 连接飞书 WebSocket 网关
2. **QQ 消息网关** (`start_qq_gateway`) - 连接 QQ 机器人网关
3. **QQ 扫码绑定** (`start_qq_qr_connect`) - 生成 QQ 扫码登录二维码

**依赖安装：**
4. **npm install** - 安装 Node.js 依赖包（ws、@tencent-connect/qqbot-connector、qrcode）

**其他系统操作：**
5. **PowerShell 查询** - 查询 Codex.exe 进程信息
6. **taskkill** - 关闭 Codex 进程
7. **cmd/explorer** - 打开文件、URL、协议链接
8. **Codex CLI 执行** - 调用 codex.exe 或 codex.cmd

这些进程需要持续运行在后台或短暂执行，但因为有可见窗口，用户体验很差。

### 解决方案

使用 Windows 特定的 `CREATE_NO_WINDOW` 标志来隐藏所有子进程窗口，让进程真正在后台静默运行。

### 修改内容

#### 1. 添加 Windows 平台导入和常量

```rust
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// Windows 常量：CREATE_NO_WINDOW 标志，用于隐藏子进程窗口
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
```

#### 2. 修改所有 Command 启动代码模式

**原代码模式：**
```rust
let mut child = Command::new(node)
    .arg(&runner)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
```

**新代码模式：**
```rust
let mut cmd = Command::new(node);
cmd.arg(&runner)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
#[cfg(target_os = "windows")]
cmd.creation_flags(CREATE_NO_WINDOW);
let mut child = cmd.spawn()
```

#### 3. 修改的具体位置（共 18 处）

**手机控制相关（3 处）：**
- `start_lark_bridge` (约 4374 行) - 飞书消息桥启动
- `start_qq_gateway` (约 4518 行) - QQ 网关启动
- `start_qq_qr_connect` (约 6713 行) - QQ 扫码绑定启动

**依赖安装（2 处）：**
- `ensure_lark_bridge_connector` (约 2598 行) - 飞书依赖安装
- `ensure_qq_qr_connector` (约 2648 行) - QQ 依赖安装

**系统工具（13 处）：**
- `command_available` (约 1757 行) - 命令可用性检查
- `codex_command` (约 3206 行) - Codex CLI 执行
- `codex_debug_port_from_process` (约 3248 行) - PowerShell 查询 Codex 进程
- `running_codex_exe_path` (约 3306 行) - PowerShell 查询 Codex 路径
- `relaunch_codex_with_debug_port` (约 3333、3342 行) - taskkill + Codex 重启
- `activate_codex_thread` (约 3815 行) - cmd start 打开协议链接
- `open_dir_in_explorer` (约 7604 行) - explorer 打开文件夹
- `open_with_system` (约 9366 行) - cmd start 打开文件/URL

---

## 问题 2：手机消息回复不完整

### 问题描述

通过手机（飞书/QQ/微信）发送消息给 Codex 后，只能收到第一段回复，无法获得完整的上下文内容。

### 根本原因

在注入到 Codex 桌面 App 的 JavaScript 脚本中（`codex_inject_send_script` 函数），获取助手回复时只提取了最后一个消息块：

```javascript
const current = after[after.length - 1] || "";  // ❌ 只获取最后一块
```

但 Codex 的回复可能被分成多个 markdown 内容块（`markdownContent`），每个块是一个独立的 DOM 元素。这导致只返回最后一个块的内容，丢失了前面的所有内容。

### 解决方案

修改脚本逻辑，获取发送消息后新增的**所有**助手回复块，并用双换行符连接成完整回复。

### 修改内容

**原代码（约 3774-3781 行）：**
```javascript
const baseLast = before[before.length - 1] || "";
let latest = "";
// ...
const after = assistantTexts();
const current = after[after.length - 1] || "";  // ❌ 只取最后一块
if ((after.length !== before.length || current !== baseLast) && current) {
  // ...
}
```

**新代码：**
```javascript
const beforeCount = before.length;  // ✅ 记录发送前的块数量
let latest = "";
// ...
const after = assistantTexts();
// ✅ 获取新增的所有助手回复块（跳过发送前已存在的块）
const newBlocks = after.slice(beforeCount);
const current = newBlocks.join("\n\n").trim();  // ✅ 合并所有新块
if (current && after.length > beforeCount) {  // ✅ 确认有新内容
  // ...
}
```

### 技术细节

1. **记录基准**：发送消息前记录 `beforeCount = before.length`
2. **提取新块**：使用 `after.slice(beforeCount)` 获取所有新增的回复块
3. **合并内容**：用 `join("\n\n")` 将多个块连接成完整文本
4. **判断条件**：改为 `after.length > beforeCount` 确认有新内容

---

## 技术细节

### CREATE_NO_WINDOW 标志

- **值**: `0x08000000`
- **作用**: 告诉 Windows 不要为新进程创建控制台窗口
- **平台**: 仅在 Windows 上有效，其他平台会忽略此标志

### 条件编译

使用 `#[cfg(target_os = "windows")]` 确保：
- 只在 Windows 平台编译相关代码
- 不影响 macOS/Linux 平台的构建
- 保持跨平台兼容性

---

## 效果

修复后：

**问题 1 - 黑窗问题：**
- ✅ 手机控制启动时不再弹出黑色命令窗口
- ✅ 安装依赖时不再闪现命令窗口
- ✅ Node.js 进程完全在后台运行
- ✅ 关闭 VarSwitch 主窗口不影响手机连接
- ✅ 所有系统操作（打开文件、重启 Codex 等）都静默执行

**问题 2 - 回复不完整：**
- ✅ 获取 Codex 的完整回复内容（所有段落）
- ✅ 多段落回复用双换行符分隔，格式清晰
- ✅ 不再丢失上下文和代码块
- ✅ 用户体验更完整流畅

---

## 验证

### 编译测试
```bash
cd src-tauri
cargo check        # 检查语法
cargo build --release  # Release 构建
```

### 功能测试

**测试 1 - 黑窗问题：**
1. 启动 VarSwitch
2. 绑定飞书或 QQ 频道（会安装依赖）
3. 确认没有黑色命令窗口弹出或闪现
4. 手机发送消息，验证连接正常
5. 尝试其他功能（打开文件夹、重启 Codex 等）

**测试 2 - 回复完整性：**
1. 通过手机发送一个需要多段落回复的问题
2. 例如："用中文解释什么是 Rust，并给出一个示例代码"
3. 确认收到完整的回复（包括文字说明 + 代码块）
4. 验证没有内容丢失

---

## 相关代码

- `src-tauri/src/lib.rs` - 主要修改文件
- `codex_inject_send_script()` - CDP 注入脚本生成器
- `lark_bridge_runner_text()` - 飞书桥接脚本
- `qq_gateway_runner_text()` - QQ 网关脚本
- `qq_qr_runner_text()` - QQ 扫码脚本

---

## 参考

- [Windows Process Creation Flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)
- [Rust std::os::windows::process::CommandExt](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html)
- [Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/)

