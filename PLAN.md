# VarSwitch 设置页面实现方案

## 概述

为 VarSwitch 添加设置页面，以覆盖面板（overlay panel）形式呈现，包含通用设置、目录设置、导入/导出备份三个区域。

## 功能范围

### 1. 通用设置（General）
- **语言切换**：中文/英文（复用现有 localStorage 逻辑，从 toolbar 移入设置面板）
- **主题切换**：亮色/暗色（复用现有逻辑，从 toolbar 移入设置面板）
- **开机自启**：启动时自动运行 VarSwitch（新增 Rust 命令）
- **静默启动**：启动时最小化到托盘（新增 Rust 命令）
- **关闭时最小化到托盘**：点击关闭按钮时隐藏而非退出（当前已硬编码实现，改为可配置）

### 2. 目录设置（Directories）
- 显示当前配置文件存储路径（只读展示 `app_data_dir`）
- 显示 Claude 配置目录路径（`~/.claude/`）
- 显示 VSCode 配置路径
- 提供"打开文件夹"按钮

### 3. 导入/导出备份（Backup）
- **导出**：将 profiles.json 导出到用户选择的文件路径
- **导入**：从用户选择的文件导入 profiles.json，合并或替换现有配置

## 技术实现

### Rust 后端新增

#### 数据结构
```rust
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    language: String,           // "en" | "zh"
    theme: String,              // "light" | "dark"
    launch_on_startup: bool,
    silent_startup: bool,
    minimize_to_tray_on_close: bool,
}
```

设置文件存储在 `data_dir(app).join("settings.json")`。

#### 新增 Tauri 命令（6个）
1. `get_app_settings` — 读取设置
2. `save_app_settings` — 保存设置（含开机自启注册表操作）
3. `get_app_paths` — 返回各配置目录路径
4. `open_folder` — 用系统文件管理器打开指定目录
5. `export_profiles` — 导出 profiles.json 到指定路径
6. `import_profiles` — 从指定路径导入 profiles

#### 开机自启实现（Windows）
通过注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 添加/删除应用路径。

### 前端实现

#### HTML
在 `index.html` 中添加设置面板 overlay（复用 `mgmt-overlay` / `mgmt-panel` 样式体系）。

面板内部使用分组卡片布局：
- 通用设置组：语言、主题用 seg-control；开机自启、静默启动、最小化到托盘用 toggle switch
- 目录设置组：路径展示 + 打开按钮
- 备份组：导出/导入按钮

#### CSS
- 新增 toggle switch 组件样式（`.settings-toggle`）
- 新增设置分组样式（`.settings-group`、`.settings-row`）
- 复用现有 `mgmt-overlay`、`mgmt-panel` 基础样式

#### JS（app.js）
- 新增 `openSettingsPanel()` / `closeSettingsPanel()` 函数
- 新增 `loadAppSettings()` / `saveAppSettings()` 函数
- 设置变更时实时预览（语言/主题立即生效）
- 在 toolbar 添加设置按钮（齿轮图标）

#### Toolbar 调整
- 语言和主题的 seg-control 保留在 toolbar（快捷切换），同时在设置面板中也可配置
- 新增齿轮图标按钮打开设置面板

### I18N 新增键
```
settingsTitle, settingsGeneral, settingsDirectories, settingsBackup,
launchOnStartup, silentStartup, minimizeToTray,
configDataDir, claudeConfigDir, vscodeConfigDir, openFolder,
exportProfiles, importProfiles, exportSuccess, importSuccess,
importConfirm, settingsBtn
```

## 文件修改清单

| 文件 | 变更 |
|------|------|
| `src-tauri/src/lib.rs` | 新增 AppSettings 结构体、6个 Tauri 命令、注册到 invoke_handler |
| `src-tauri/capabilities/default.json` | 添加 `dialog:allow-open`、`dialog:allow-save` 权限 |
| `public/index.html` | 添加设置面板 HTML、toolbar 齿轮按钮 |
| `public/style.css` | 添加 toggle switch、settings-group 等样式 |
| `public/app.js` | 添加设置相关 JS 逻辑、I18N 键 |

## 实现顺序

1. Rust 后端：AppSettings 结构体 + 读写命令 + 开机自启 + 路径/导入导出命令
2. 前端 CSS：toggle switch + settings 分组样式
3. 前端 HTML：设置面板结构
4. 前端 JS：I18N 键 + 设置面板逻辑 + toolbar 按钮
5. Tauri capabilities 更新
