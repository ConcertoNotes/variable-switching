# 修复手机控制后台 Node.js 黑窗问题

## 问题描述

当启用 VarSwitch 手机控制功能（飞书/QQ/微信桥接）时，会弹出黑色的 Node.js 命令行窗口。关闭这个窗口后，手机连接就会断开。

## 根本原因

在 Windows 平台上，使用 `Command::new()` 启动子进程时，默认会创建一个可见的控制台窗口。VarSwitch 的三个手机控制功能都需要启动 Node.js 进程来运行桥接脚本：

1. **飞书消息桥** (`start_lark_bridge`) - 连接飞书 WebSocket 网关
2. **QQ 消息网关** (`start_qq_gateway`) - 连接 QQ 机器人网关
3. **QQ 扫码绑定** (`start_qq_qr_connect`) - 生成 QQ 扫码登录二维码

这些 Node.js 进程需要持续运行在后台，但因为有可见窗口，用户关闭窗口会直接终止进程。

## 解决方案

使用 Windows 特定的 `CREATE_NO_WINDOW` 标志来隐藏子进程窗口，让 Node.js 进程真正在后台运行。

### 修改内容

#### 1. 添加 Windows 平台导入和常量

```rust
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// Windows 常量：CREATE_NO_WINDOW 标志，用于隐藏子进程窗口
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
```

#### 2. 修改所有 Node.js 进程启动代码

将原来的链式调用改为先创建 `Command` 对象，然后在 Windows 平台上添加 `creation_flags`：

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

#### 3. 修改的具体位置

- **`start_lark_bridge` 函数** (约 4374 行) - 飞书消息桥启动
- **`start_qq_gateway` 函数** (约 4518 行) - QQ 网关启动
- **`start_qq_qr_connect` 函数** (约 6713 行) - QQ 扫码绑定启动

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

## 效果

修复后：
- ✅ 手机控制启动时不再弹出黑色命令窗口
- ✅ Node.js 进程完全在后台运行
- ✅ 关闭 VarSwitch 主窗口不影响手机连接
- ✅ 用户体验更流畅，不会被突然弹出的窗口打断

## 验证

编译测试：
```bash
cd src-tauri
cargo check
```

运行测试：
1. 启动 VarSwitch
2. 绑定飞书或 QQ 频道
3. 确认没有黑色命令窗口弹出
4. 手机发送消息，验证连接正常

## 相关代码

- `src-tauri/src/lib.rs` - 主要修改文件
- `lark_bridge_runner_text()` - 飞书桥接脚本
- `qq_gateway_runner_text()` - QQ 网关脚本
- `qq_qr_runner_text()` - QQ 扫码脚本

## 参考

- [Windows Process Creation Flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)
- [Rust std::os::windows::process::CommandExt](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html)
