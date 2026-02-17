# VarSwitch

一个轻量级的环境变量配置管理工具，专为 Claude Code / API 用户设计。通过可视化界面一键切换 API Key 和 Base URL，同步写入系统环境变量、VSCode 配置和 Claude 配置文件。

## 功能特性

- **多配置管理** — 创建、编辑、删除多套 API 配置（API Key + Base URL），随时切换
- **一键同步** — 切换配置时自动写入系统环境变量、VSCode settings.json、Claude 配置文件
- **实时状态** — 首页展示当前各位置（系统环境变量 / VSCode / Claude）的配置状态
- **导入导出** — 支持配置的备份与恢复
- **Skills 管理** — 浏览、安装、编辑 Claude Code 自定义 Skills，支持从 GitHub 仓库发现
- **Prompts 编辑** — 直接编辑 `~/.claude/CLAUDE.md` 提示词文件，内置模板库
- **MCP Server 管理** — 管理 `~/.claude.json` 中的 MCP Server 配置，支持预设搜索
- **系统托盘** — 最小化到托盘，支持开机自启和静默启动
- **中英双语** — 界面支持中文 / English 切换
- **亮暗主题** — 支持 Light / Dark 主题切换

## 技术栈

- **前端**: 原生 HTML + CSS + JavaScript（无框架依赖）
- **后端**: [Tauri v2](https://v2.tauri.app/) + Rust
- **平台**: Windows / macOS

## 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- Tauri v2 系统依赖（参考 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)）

## 开发

```bash
# 安装依赖
npm install

# 启动开发模式
npm run tauri dev
```

Windows 用户也可以使用：

```bash
# 开发模式
dev.bat

# 构建
build.bat
```

## 构建

```bash
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`，Windows 生成 `.msi` 和 `.exe` 安装包，macOS 生成 `.dmg`。

## CI/CD

项目配置了 GitHub Actions 自动构建，推送 `v*` 标签时触发，支持：

- macOS (aarch64 + x86_64)
- Windows (x86_64)

## 项目结构

```
├── public/              # 前端资源
│   ├── index.html       # 主页面
│   ├── app.js           # 应用逻辑
│   ├── style.css        # 样式
│   └── app-icon.png     # 应用图标
├── src-tauri/           # Tauri / Rust 后端
│   ├── src/lib.rs       # 核心逻辑（配置读写、环境变量同步）
│   ├── Cargo.toml       # Rust 依赖
│   └── capabilities/    # Tauri 权限配置
├── .github/workflows/   # CI 构建配置
├── dev.bat              # Windows 开发脚本
└── build.bat            # Windows 构建脚本
```

## 许可证

MIT
