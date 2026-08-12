//! Skills 管理：本地技能增删改查、目录扫描、URL / ZIP 安装、仓库与 GitHub 搜索（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillInfo {
    pub(crate) name: String,
    pub(crate) content: String,
    /// "command" = ~/.claude/commands/, "skill" = ~/.claude/skills/
    pub(crate) source_type: String,
    /// 从 SKILL.md frontmatter 中解析的描述
    pub(crate) description: String,
}

pub(crate) fn claude_commands_dir() -> PathBuf {
    home_dir().join(".claude").join("commands")
}

pub(crate) fn claude_skills_dir() -> PathBuf {
    home_dir().join(".claude").join("skills")
}

/// 从 SKILL.md 的 YAML frontmatter 中解析 description
pub(crate) fn parse_skill_description(content: &str) -> String {
    if !content.starts_with("---") {
        return String::new();
    }
    // 找到第二个 "---"
    if let Some(end) = content[3..].find("---") {
        let frontmatter = &content[3..3 + end];
        for line in frontmatter.lines() {
            let line = line.trim();
            if line.starts_with("description:") {
                return line["description:".len()..].trim().to_string();
            }
        }
    }
    String::new()
}

/// 收集 ~/.claude/skills/ 下的 SKILL.md 文件
pub(crate) fn collect_skills_from_skills_dir(skills: &mut Vec<SkillInfo>) {
    let dir = claude_skills_dir();
    if !dir.exists() {
        return;
    }
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || name == "README.md" {
            continue;
        }
        let content = fs::read_to_string(&skill_md).unwrap_or_default();
        let description = parse_skill_description(&content);
        skills.push(SkillInfo {
            name,
            content,
            source_type: "skill".into(),
            description,
        });
    }
}

// ── Skills Commands ──────────────────────────────────

/// Recursively collect .md skill files from a directory.
/// Files in subdirectories get names like "subfolder:filename".
pub(crate) fn collect_skills_recursive(base: &PathBuf, current: &PathBuf, skills: &mut Vec<SkillInfo>) {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skills_recursive(base, &path, skills);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            // Build relative prefix from base dir (e.g. "subfolder:command")
            let parent = path.parent().unwrap_or(base);
            let name = if parent != base.as_path() {
                if let Ok(rel) = parent.strip_prefix(base) {
                    let prefix = rel.to_string_lossy().replace(['/', '\\'], ":");
                    format!("{}:{}", prefix, stem)
                } else {
                    stem
                }
            } else {
                stem
            };
            let content = fs::read_to_string(&path).unwrap_or_default();
            skills.push(SkillInfo {
                name,
                content,
                source_type: "command".into(),
                description: String::new(),
            });
        }
    }
}

#[tauri::command]
pub(crate) fn get_skills() -> Result<Vec<SkillInfo>, String> {
    let mut skills = Vec::new();

    // 扫描 ~/.claude/commands/ (斜杠命令)
    let cmd_dir = claude_commands_dir();
    if cmd_dir.exists() {
        collect_skills_recursive(&cmd_dir, &cmd_dir, &mut skills);
    }

    // 扫描 ~/.claude/skills/ (自动加载技能)
    collect_skills_from_skills_dir(&mut skills);

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// 校验技能名称，拒绝任何可能造成路径穿越的输入。
/// 名称中唯一允许的层级分隔符是命令使用的冒号（`:`），
/// 每个片段都不允许为空、为 `.`/`..`，也不允许包含路径分隔符或 NUL。
pub(crate) fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("技能名称不能为空".into());
    }
    for segment in name.split(':') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains('/')
            || segment.contains('\\')
            || segment.contains('\0')
        {
            return Err("技能名称包含非法字符".into());
        }
    }
    Ok(())
}

/// Convert a skill name like "subfolder:command" to a file path (commands dir)
pub(crate) fn skill_name_to_path(name: &str) -> PathBuf {
    let dir = claude_commands_dir();
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() > 1 {
        let mut path = dir;
        for part in &parts[..parts.len() - 1] {
            path = path.join(part);
        }
        path.join(format!("{}.md", parts.last().unwrap()))
    } else {
        dir.join(format!("{}.md", name))
    }
}

/// 根据 sourceType 获取技能文件路径
pub(crate) fn skill_path_by_type(name: &str, source_type: &str) -> PathBuf {
    if source_type == "skill" {
        claude_skills_dir().join(name).join("SKILL.md")
    } else {
        skill_name_to_path(name)
    }
}

#[tauri::command]
pub(crate) fn save_skill(name: String, content: String, source_type: Option<String>) -> Result<(), String> {
    validate_skill_name(&name)?;
    let st = source_type.as_deref().unwrap_or("command");
    let path = skill_path_by_type(&name, st);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_file_atomic(&path, &content)
}

#[tauri::command]
pub(crate) fn delete_skill(name: String, source_type: Option<String>) -> Result<(), String> {
    validate_skill_name(&name)?;
    let st = source_type.as_deref().unwrap_or("command");
    if st == "skill" {
        // 删除整个技能目录
        let dir = claude_skills_dir().join(&name);
        if dir.exists() && dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    } else {
        let path = skill_name_to_path(&name);
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}

// ── Skills Discovery ─────────────────────────────────

/// A skill available in the curated catalog or from GitHub search
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogSkill {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) description_zh: String,
    /// GitHub raw URL to download the SKILL.md / command .md
    pub(crate) download_url: String,
    /// Source repo label e.g. "anthropics/skills"
    pub(crate) source: String,
    /// Category tag
    pub(crate) category: String,
    /// Whether this skill is installed locally
    pub(crate) installed: bool,
    /// GitHub stars count (0 for catalog items)
    pub(crate) stars: u64,
    /// GitHub repo URL for linking
    pub(crate) repo_url: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillRepo {
    pub(crate) url: String,
    pub(crate) branch: String,
    pub(crate) enabled: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct SkillReposData {
    pub(crate) repos: Vec<SkillRepo>,
}

// ── Skills Discovery Helpers ─────────────────────────

pub(crate) fn skill_repos_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("skill_repos.json")
}

pub(crate) fn read_skill_repos(app: &tauri::AppHandle) -> SkillReposData {
    let path = skill_repos_path(app);
    if !path.exists() {
        return SkillReposData::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn write_skill_repos(app: &tauri::AppHandle, data: &SkillReposData) -> Result<(), String> {
    let path = skill_repos_path(app);
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    write_file_atomic(&path, &json)
}

pub(crate) fn collect_skill_names_recursive(base: &PathBuf, current: &PathBuf, names: &mut Vec<String>) {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skill_names_recursive(base, &path, names);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let parent = path.parent().unwrap_or(base);
            let name = if parent != base.as_path() {
                if let Ok(rel) = parent.strip_prefix(base) {
                    let prefix = rel.to_string_lossy().replace(['/', '\\'], ":");
                    format!("{}:{}", prefix, stem)
                } else {
                    stem
                }
            } else {
                stem
            };
            names.push(name);
        }
    }
}

pub(crate) fn get_installed_skill_names() -> Vec<String> {
    let mut names = Vec::new();

    // 从 commands 目录收集
    let cmd_dir = claude_commands_dir();
    if cmd_dir.exists() {
        collect_skill_names_recursive(&cmd_dir, &cmd_dir, &mut names);
    }

    // 从 skills 目录收集（目录名即技能名）
    let skills_dir = claude_skills_dir();
    if skills_dir.exists() {
        if let Ok(entries) = fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("SKILL.md").exists() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }

    names.sort();
    names.dedup();
    names
}

/// Build the curated catalog of skills with install status
pub(crate) fn build_catalog() -> Vec<CatalogSkill> {
    let installed = get_installed_skill_names();
    let mut catalog = vec![
        // ── anthropics/skills (official) ──
        CatalogSkill {
            name: "pdf".into(),
            description: "PDF processing: read, merge, split, rotate, watermark, encrypt, OCR".into(),
            description_zh: "PDF 处理：读取、合并、拆分、旋转、水印、加密、OCR".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/pdf/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "docx".into(),
            description: "Word document creation and manipulation with python-docx".into(),
            description_zh: "使用 python-docx 创建和操作 Word 文档".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/docx/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "xlsx".into(),
            description: "Excel spreadsheet creation and data processing with openpyxl".into(),
            description_zh: "使用 openpyxl 创建 Excel 电子表格和数据处理".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/xlsx/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "pptx".into(),
            description: "PowerPoint presentation creation with python-pptx".into(),
            description_zh: "使用 python-pptx 创建 PowerPoint 演示文稿".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/pptx/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "document".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "frontend-design".into(),
            description: "Create production-grade frontend interfaces with modern web technologies".into(),
            description_zh: "使用现代 Web 技术创建生产级前端界面".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/frontend-design/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "canvas-design".into(),
            description: "Create interactive HTML5 Canvas visualizations and animations".into(),
            description_zh: "创建交互式 HTML5 Canvas 可视化和动画".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/canvas-design/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "algorithmic-art".into(),
            description: "Generate algorithmic and generative art using code".into(),
            description_zh: "使用代码生成算法艺术和生成艺术".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/algorithmic-art/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "theme-factory".into(),
            description: "Create consistent design themes and color systems".into(),
            description_zh: "创建一致的设计主题和颜色系统".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/theme-factory/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "mcp-builder".into(),
            description: "Build Model Context Protocol servers and tools".into(),
            description_zh: "构建 MCP (Model Context Protocol) 服务器和工具".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/mcp-builder/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "skill-creator".into(),
            description: "Create new Claude skills with proper structure and metadata".into(),
            description_zh: "创建具有正确结构和元数据的新 Claude 技能".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/skill-creator/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "web-artifacts-builder".into(),
            description: "Build interactive web artifacts and single-page applications".into(),
            description_zh: "构建交互式 Web 工件和单页应用".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/web-artifacts-builder/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "webapp-testing".into(),
            description: "Automated web application testing with Playwright and other tools".into(),
            description_zh: "使用 Playwright 等工具进行自动化 Web 应用测试".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/webapp-testing/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "testing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "doc-coauthoring".into(),
            description: "Collaborative document writing and editing assistance".into(),
            description_zh: "协作文档写作和编辑辅助".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/doc-coauthoring/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "writing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "brand-guidelines".into(),
            description: "Create and maintain brand identity guidelines".into(),
            description_zh: "创建和维护品牌识别指南".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/brand-guidelines/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "internal-comms".into(),
            description: "Draft internal communications, memos, and announcements".into(),
            description_zh: "起草内部通信、备忘录和公告".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/internal-comms/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "writing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "slack-gif-creator".into(),
            description: "Create animated GIFs for Slack and messaging platforms".into(),
            description_zh: "为 Slack 和消息平台创建动画 GIF".into(),
            download_url: "https://raw.githubusercontent.com/anthropics/skills/main/skills/slack-gif-creator/SKILL.md".into(),
            source: "anthropics/skills".into(),
            category: "design".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        // ── Community skills ──
        CatalogSkill {
            name: "git-commit-message".into(),
            description: "Generate conventional commit messages following best practices".into(),
            description_zh: "按照最佳实践生成规范的 Git 提交信息".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "code-review".into(),
            description: "Thorough code review with security, performance, and style checks".into(),
            description_zh: "全面的代码审查，包括安全、性能和风格检查".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "docker-compose".into(),
            description: "Generate and optimize Docker Compose configurations".into(),
            description_zh: "生成和优化 Docker Compose 配置".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "devops".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "api-docs-generator".into(),
            description: "Generate OpenAPI/Swagger documentation from code".into(),
            description_zh: "从代码生成 OpenAPI/Swagger 文档".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "sql-optimizer".into(),
            description: "Analyze and optimize SQL queries for better performance".into(),
            description_zh: "分析和优化 SQL 查询以提高性能".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "database".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "regex-builder".into(),
            description: "Build and test regular expressions with explanations".into(),
            description_zh: "构建和测试正则表达式并提供解释".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "terraform-generator".into(),
            description: "Generate Terraform IaC configurations for cloud resources".into(),
            description_zh: "为云资源生成 Terraform 基础设施即代码配置".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "devops".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "unit-test-writer".into(),
            description: "Generate comprehensive unit tests for functions and classes".into(),
            description_zh: "为函数和类生成全面的单元测试".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "testing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "readme-generator".into(),
            description: "Generate professional README.md files for projects".into(),
            description_zh: "为项目生成专业的 README.md 文件".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "writing".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "ci-cd-pipeline".into(),
            description: "Generate GitHub Actions / GitLab CI pipeline configurations".into(),
            description_zh: "生成 GitHub Actions / GitLab CI 流水线配置".into(),
            download_url: "".into(),
            source: "community".into(),
            category: "devops".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "codebase-explorer".into(),
            description: "Map unfamiliar repositories, identify entry points, data flow, and high-risk modules".into(),
            description_zh: "快速梳理陌生仓库，识别入口、数据流和高风险模块".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "bug-root-cause".into(),
            description: "Debug production bugs with reproduction steps, fault isolation, and regression tests".into(),
            description_zh: "按复现、隔离、回归测试的流程定位生产 Bug 根因".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "debugging".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "migration-planner".into(),
            description: "Plan framework, database, or API migrations with compatibility and rollback checks".into(),
            description_zh: "规划框架、数据库或 API 迁移，包含兼容性和回滚检查".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "architecture".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "performance-profiler".into(),
            description: "Find bottlenecks, propose measurable optimizations, and define before/after benchmarks".into(),
            description_zh: "定位性能瓶颈，提出可度量优化，并定义优化前后基准".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "performance".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "api-integration".into(),
            description: "Integrate third-party APIs with auth, retries, error mapping, and test fixtures".into(),
            description_zh: "集成第三方 API，覆盖认证、重试、错误映射和测试夹具".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "development".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
        CatalogSkill {
            name: "tauri-rust-desktop".into(),
            description: "Build and debug Tauri desktop features across Rust commands, frontend state, and packaging".into(),
            description_zh: "开发和调试 Tauri 桌面功能，覆盖 Rust 命令、前端状态和打包".into(),
            download_url: "".into(),
            source: "recommended".into(),
            category: "desktop".into(),
            installed: false,
            stars: 0,
            repo_url: String::new(),
        },
    ];

    // Mark installed skills
    for skill in &mut catalog {
        skill.installed = installed.contains(&skill.name);
    }

    catalog
}

// ── Skills Discovery Commands ────────────────────────

#[tauri::command]
pub(crate) fn get_catalog_skills() -> Vec<CatalogSkill> {
    build_catalog()
}

#[tauri::command]
pub(crate) fn get_skill_repos(app: tauri::AppHandle) -> Vec<SkillRepo> {
    read_skill_repos(&app).repos
}

#[tauri::command]
pub(crate) fn add_skill_repo(app: tauri::AppHandle, url: String, branch: String) -> Result<(), String> {
    let url = url.trim().to_string();
    let branch = if branch.trim().is_empty() {
        "main".to_string()
    } else {
        branch.trim().to_string()
    };
    let mut data = read_skill_repos(&app);
    if data.repos.iter().any(|r| r.url == url) {
        return Err("Repository already exists".into());
    }
    data.repos.push(SkillRepo {
        url,
        branch,
        enabled: true,
    });
    write_skill_repos(&app, &data)
}

#[tauri::command]
pub(crate) fn remove_skill_repo(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let mut data = read_skill_repos(&app);
    data.repos.retain(|r| r.url != url);
    write_skill_repos(&app, &data)
}

/// 通过 GitHub Tree API 查找仓库中 SKILL.md 的实际路径
pub(crate) fn find_skill_md_in_repo(
    client: &reqwest::blocking::Client,
    full_name: &str,
    branch: &str,
) -> Result<String, String> {
    let tree_url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        full_name, branch
    );
    let resp = client
        .get(&tree_url)
        .send()
        .map_err(|e| format!("GitHub Tree API error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub Tree API returned {}", resp.status()));
    }
    let body: serde_json::Value = resp
        .json()
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let mut skill_paths: Vec<String> = Vec::new();
    if let Some(tree) = body.get("tree").and_then(|v| v.as_array()) {
        for item in tree {
            if let Some(path) = item.get("path").and_then(|v| v.as_str()) {
                if path.ends_with("SKILL.md")
                    && item.get("type").and_then(|v| v.as_str()) == Some("blob")
                {
                    skill_paths.push(path.to_string());
                }
            }
        }
    }

    if skill_paths.is_empty() {
        return Err("No SKILL.md found in repository".into());
    }

    // 优先选择 .claude/skills/ 下的，其次选最短路径
    skill_paths.sort_by(|a, b| {
        let a_pref = a.contains(".claude/skills/");
        let b_pref = b.contains(".claude/skills/");
        b_pref.cmp(&a_pref).then(a.len().cmp(&b.len()))
    });

    let path = &skill_paths[0];
    Ok(format!(
        "https://raw.githubusercontent.com/{}/{}/{}",
        full_name, branch, path
    ))
}

/// 尝试下载 URL，失败时尝试镜像
pub(crate) fn download_with_fallback(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    // 尝试原始 URL
    match client.get(url).send() {
        Ok(resp) if resp.status().is_success() => {
            return resp.text().map_err(|e| format!("Read failed: {}", e));
        }
        _ => {}
    }

    // 尝试 GitHub 镜像
    if url.contains("raw.githubusercontent.com") || url.contains("github.com") {
        let mirror_url = format!("https://ghfast.top/{}", url);
        if let Ok(resp) = client.get(&mirror_url).send() {
            if resp.status().is_success() {
                return resp.text().map_err(|e| format!("Read failed: {}", e));
            }
        }
    }

    Err(format!("Download failed: {}", url))
}

pub(crate) fn recommended_skill_content(name: &str) -> String {
    let (description, body) = match name {
        "codebase-explorer" => (
            "Map unfamiliar repositories and identify high-risk areas",
            "Use this skill when entering an unfamiliar repository. Start by identifying the app type, package manager, entry points, config files, persistence layer, and test commands. Summarize the architecture, then list the files most likely to matter for the requested change. Prefer evidence from local files over assumptions.",
        ),
        "bug-root-cause" => (
            "Debug bugs with reproduction, isolation, and regression tests",
            "Use this skill for failures, crashes, incorrect behavior, or flaky tests. Capture the observed symptom, expected behavior, likely execution path, and the smallest reproduction. Inspect logs and call sites before editing. Fix the narrow cause, then add or update a regression test when practical.",
        ),
        "migration-planner" => (
            "Plan framework, database, or API migrations safely",
            "Use this skill before migrations or compatibility upgrades. Inventory current versions and integration points, identify breaking changes, plan staged rollout and rollback, and separate mechanical edits from behavior changes. Validate with focused tests after each stage.",
        ),
        "performance-profiler" => (
            "Find bottlenecks and define measurable optimizations",
            "Use this skill for slow UI, slow commands, expensive queries, high memory use, or request latency. Establish a baseline, identify the hot path, avoid speculative rewrites, and propose changes that can be measured before and after.",
        ),
        "api-integration" => (
            "Integrate third-party APIs with robust error handling",
            "Use this skill when adding or debugging external API integrations. Verify current docs, model authentication and rate limits, handle retries and timeouts explicitly, normalize provider errors, and add fixtures or mocks for tests.",
        ),
        "tauri-rust-desktop" => (
            "Build and debug Tauri desktop features across Rust and frontend",
            "Use this skill for Tauri commands, file system access, tray behavior, frontend invoke calls, packaging, and Windows/macOS/Linux path issues. Keep Rust command contracts stable, validate frontend payload names, and test both JS behavior and Rust command registration.",
        ),
        _ => (
            "Installed from catalog",
            "Use this skill as a starting point. Edit this file to add project-specific instructions, examples, and constraints.",
        ),
    };

    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\n{body}\n")
}

/// Download a skill from a URL and install it to ~/.claude/skills/
#[tauri::command]
pub(crate) async fn install_skill_from_url(name: String, url: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name is required".into());
    }

    let content = if url.is_empty() {
        // No URL — create a placeholder skill
        recommended_skill_content(&name)
    } else {
        let url_clone = url.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let client = build_http_client(30)?;

            // 先尝试直接下载
            if let Ok(text) = download_with_fallback(&client, &url_clone) {
                return Ok(text);
            }

            // 直接下载失败（可能 SKILL.md 不在根目录），尝试用 Tree API 查找真实路径
            // 从 URL 中提取 full_name 和 branch
            // URL 格式: https://raw.githubusercontent.com/{owner}/{repo}/{branch}/SKILL.md
            if url_clone.contains("raw.githubusercontent.com") {
                let parts: Vec<&str> = url_clone
                    .trim_start_matches("https://raw.githubusercontent.com/")
                    .splitn(4, '/')
                    .collect();
                if parts.len() >= 3 {
                    let full_name = format!("{}/{}", parts[0], parts[1]);
                    let branch = parts[2];
                    if let Ok(real_url) = find_skill_md_in_repo(&client, &full_name, branch) {
                        return download_with_fallback(&client, &real_url);
                    }
                }
            }

            Err(format!("Download failed: {}", url_clone))
        })
        .await
        .map_err(|e| format!("Task failed: {}", e))??
    };

    // 安装到 ~/.claude/skills/<name>/SKILL.md
    let skill_dir = claude_skills_dir().join(&name);
    fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    let path = skill_dir.join("SKILL.md");
    write_file_atomic(&path, &content)?;
    Ok(())
}

// ── Skills ZIP 安装（对标 cc-switch：本地 ZIP 一键安装，可多应用落盘）──

/// Codex 的 skills 目录：~/.codex/skills/
pub(crate) fn codex_skills_dir() -> PathBuf {
    codex_config_dir().join("skills")
}

/// skill 名称清洗：只保留 [A-Za-z0-9._-]，防止路径注入。
/// 清洗后为空、或全部由 '.' 组成（"." / ".." 会指向当前/父目录）时视为无效，返回空串。
pub(crate) fn sanitize_skill_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    // all() 对空串返回 true，顺带覆盖了清洗后为空的情况
    if cleaned.chars().all(|c| c == '.') {
        return String::new();
    }
    cleaned
}

/// 在 ZIP 文件条目名（已规范化为 '/' 分隔、不含目录条目）中定位 skill 根前缀。
/// 返回 "" 表示档案根，否则形如 "dir/sub/"（带尾部斜杠）。
/// 优先级：根目录 SKILL.md → 唯一顶层目录内的 SKILL.md → 档案中第一个 */SKILL.md 的父目录。
pub(crate) fn locate_skill_zip_root(names: &[String]) -> Result<String, String> {
    if names.iter().any(|n| n == "SKILL.md") {
        return Ok(String::new());
    }
    let tops: HashSet<&str> = names.iter().filter_map(|n| n.split('/').next()).collect();
    if tops.len() == 1 {
        let top = tops.iter().next().copied().unwrap_or_default();
        let candidate = format!("{top}/SKILL.md");
        if names.iter().any(|n| n == &candidate) {
            return Ok(format!("{top}/"));
        }
    }
    for n in names {
        if let Some(prefix) = n.strip_suffix("SKILL.md") {
            if prefix.ends_with('/') {
                return Ok(prefix.to_string());
            }
        }
    }
    Err("ZIP 内未找到 SKILL.md".to_string())
}

/// 校验并解压 ZIP 中的 skill 根目录内容到 dest，返回清洗后的 skill 名。
/// - zip-slip 防护：每个条目都必须通过 enclosed_name() 校验（统一处理 '\'、'..'、绝对路径与盘符），
///   任一条目越界即整体拒绝；
/// - 跳过 __MACOSX/ 目录与 .DS_Store 文件；
/// - fallback_name 为 ZIP 文件名去扩展名，作为最后的命名兜底。
pub(crate) fn extract_skill_zip<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    fallback_name: &str,
    dest: &Path,
) -> Result<String, String> {
    // 第一遍：安全校验 + 收集有效文件条目（index + 规范化路径）
    let mut entries: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {e}"))?;
        let Some(safe_path) = entry.enclosed_name() else {
            return Err(format!("ZIP 内含不安全路径，已拒绝安装: {}", entry.name()));
        };
        if entry.is_dir() || entry.is_symlink() {
            continue;
        }
        // 用校验后的组件重建 '/' 分隔的规范化路径，后续定位、裁剪、写盘都基于它
        let normalized = safe_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if normalized.is_empty()
            || normalized == "__MACOSX"
            || normalized.starts_with("__MACOSX/")
            || normalized.rsplit('/').next() == Some(".DS_Store")
        {
            continue;
        }
        entries.push((i, normalized));
    }

    let names: Vec<String> = entries.iter().map(|(_, n)| n.clone()).collect();
    let root = locate_skill_zip_root(&names)?;

    // 读取 SKILL.md 内容，取 frontmatter 中的 name
    let skill_md_name = format!("{root}SKILL.md");
    let skill_md_idx = entries
        .iter()
        .find(|(_, n)| n == &skill_md_name)
        .map(|(i, _)| *i)
        .ok_or_else(|| "ZIP 内未找到 SKILL.md".to_string())?;
    let mut skill_md_content = String::new();
    {
        let mut f = archive
            .by_index(skill_md_idx)
            .map_err(|e| format!("读取 SKILL.md 失败: {e}"))?;
        f.read_to_string(&mut skill_md_content)
            .map_err(|e| format!("读取 SKILL.md 失败: {e}"))?;
    }

    // 命名优先级：frontmatter name → skill 根目录名 → ZIP 文件名去扩展名
    let raw_name = yaml_front_matter_value(&skill_md_content, "name").unwrap_or_else(|| {
        if root.is_empty() {
            fallback_name.to_string()
        } else {
            root.trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(fallback_name)
                .to_string()
        }
    });
    let name = sanitize_skill_name(&raw_name);
    if name.is_empty() {
        return Err(format!("技能名称无效: {raw_name}"));
    }

    // 第二遍：把 skill 根目录内的文件解压到 dest
    fs::create_dir_all(dest).map_err(|e| format!("创建临时目录失败: {e}"))?;
    for (idx, normalized) in &entries {
        let Some(rel) = normalized.strip_prefix(root.as_str()) else {
            continue; // skill 根之外的文件（如仓库其余内容）不安装
        };
        if rel.is_empty() {
            continue;
        }
        let out_path = dest.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut entry = archive
            .by_index(*idx)
            .map_err(|e| format!("读取 ZIP 条目失败: {e}"))?;
        let mut out = fs::File::create(&out_path).map_err(|e| format!("写入 {rel} 失败: {e}"))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("写入 {rel} 失败: {e}"))?;
    }
    Ok(name)
}

/// 递归复制目录内容（把临时目录中的 skill 复制到各应用的 skills/ 下）
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建目录失败: {e}"))?;
    let entries = fs::read_dir(src).map_err(|e| format!("读取目录失败: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录失败: {e}"))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| format!("复制文件失败: {e}"))?;
        }
    }
    Ok(())
}

/// 弹出系统文件选择框选择 skill ZIP，返回所选路径（用户取消时为 None）
#[tauri::command]
pub(crate) async fn pick_skill_zip(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    // blocking_pick_file 不能在主线程调用，放到阻塞线程池执行
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("ZIP", &["zip"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?;
    match picked {
        Some(file_path) => {
            let path = file_path
                .simplified()
                .into_path()
                .map_err(|e| format!("解析所选文件路径失败: {e}"))?;
            Ok(Some(path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// 从本地 ZIP 安装 Skill，可同时安装到 Claude（~/.claude/skills/）与 Codex（~/.codex/skills/）。
/// apps 形如 { "claude": bool, "codex": bool }；目标已存在且未允许覆盖时返回 Err("EXISTS:<name>")，
/// 由前端确认后带 overwrite=true 重试。成功返回 { "name": ..., "installedTo": [...] }。
#[tauri::command]
pub(crate) async fn install_skill_from_zip(
    path: String,
    apps: serde_json::Value,
    overwrite: Option<bool>,
) -> Result<serde_json::Value, String> {
    let to_claude = apps.get("claude").and_then(|v| v.as_bool()).unwrap_or(false);
    let to_codex = apps.get("codex").and_then(|v| v.as_bool()).unwrap_or(false);
    if !to_claude && !to_codex {
        return Err("请至少选择一个目标应用".into());
    }
    let overwrite = overwrite.unwrap_or(false);

    tauri::async_runtime::spawn_blocking(move || {
        let zip_path = PathBuf::from(&path);
        let fallback_name = zip_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("skill")
            .to_string();
        let file = fs::File::open(&zip_path).map_err(|e| format!("打开 ZIP 失败: {e}"))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 ZIP 失败: {e}"))?;

        // 先解压到临时目录，全部校验通过后再落盘到各应用，避免写入半成品
        let staging =
            std::env::temp_dir().join(format!("varswitch-skill-{}", uuid::Uuid::new_v4()));
        let result = (|| {
            let name = extract_skill_zip(&mut archive, &fallback_name, &staging)?;

            let mut targets: Vec<(&str, PathBuf)> = Vec::new();
            if to_claude {
                targets.push(("claude", claude_skills_dir().join(&name)));
            }
            if to_codex {
                targets.push(("codex", codex_skills_dir().join(&name)));
            }

            // 任一目标已存在且未允许覆盖 → EXISTS:<name>，前端确认后带 overwrite=true 重试
            if !overwrite && targets.iter().any(|(_, dir)| dir.exists()) {
                return Err(format!("EXISTS:{name}"));
            }

            let mut installed_to: Vec<String> = Vec::new();
            for (app_key, target) in &targets {
                if target.exists() {
                    fs::remove_dir_all(target).map_err(|e| format!("清空已有技能目录失败: {e}"))?;
                }
                copy_dir_recursive(&staging, target)?;
                installed_to.push((*app_key).to_string());
            }

            Ok(serde_json::json!({ "name": name, "installedTo": installed_to }))
        })();
        let _ = fs::remove_dir_all(&staging);
        result
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[cfg(test)]
mod skill_zip_tests {
    use super::*;
    use std::io::Cursor;

    /// 用 zip crate 现场构造内存 ZIP
    fn build_zip(entries: &[(&str, &str)]) -> zip::ZipArchive<Cursor<Vec<u8>>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        let cursor = writer.finish().unwrap();
        zip::ZipArchive::new(cursor).unwrap()
    }

    fn temp_dest(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("varswitch-zip-test-{tag}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn zip_slip_entry_is_rejected() {
        let mut archive = build_zip(&[
            ("SKILL.md", "---\nname: ok\n---\nbody"),
            ("../evil.txt", "pwned"),
        ]);
        let dest = temp_dest("slip");
        let result = extract_skill_zip(&mut archive, "fallback", &dest);
        assert!(result.is_err(), "zip-slip 条目必须被整体拒绝");
        assert!(result.unwrap_err().contains("不安全路径"));
        assert!(!dest.exists(), "拒绝时不应产生任何落盘");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn skill_md_located_at_archive_root() {
        let mut archive = build_zip(&[
            ("SKILL.md", "---\nname: root-skill\ndescription: d\n---\nbody"),
            ("scripts/run.ps1", "echo hi"),
        ]);
        let dest = temp_dest("root");
        let name = extract_skill_zip(&mut archive, "fallback", &dest).unwrap();
        assert_eq!(name, "root-skill");
        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("scripts").join("run.ps1").is_file());
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn skill_md_located_in_unique_top_dir() {
        let mut archive = build_zip(&[
            ("my-skill/SKILL.md", "no frontmatter here"),
            ("my-skill/extra.txt", "x"),
        ]);
        let dest = temp_dest("topdir");
        let name = extract_skill_zip(&mut archive, "fallback", &dest).unwrap();
        // 无 frontmatter name → 用 skill 根目录名
        assert_eq!(name, "my-skill");
        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("extra.txt").is_file());
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn skill_md_located_in_nested_dir_takes_parent_as_root() {
        let mut archive = build_zip(&[
            ("repo/README.md", "readme"),
            ("repo/skills/foo/SKILL.md", "---\ndescription: no name\n---\n"),
            ("repo/skills/foo/assets/a.txt", "a"),
        ]);
        let dest = temp_dest("nested");
        let name = extract_skill_zip(&mut archive, "fallback", &dest).unwrap();
        assert_eq!(name, "foo", "应取第一个 */SKILL.md 的父目录名");
        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("assets").join("a.txt").is_file());
        assert!(
            !dest.join("README.md").exists(),
            "skill 根之外的文件不应被解压"
        );
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn locate_root_prefers_archive_root_over_subdir() {
        let names = vec!["SKILL.md".to_string(), "sub/SKILL.md".to_string()];
        assert_eq!(locate_skill_zip_root(&names).unwrap(), "");
    }

    #[test]
    fn missing_skill_md_is_an_error() {
        let mut archive = build_zip(&[("readme.txt", "hi")]);
        let dest = temp_dest("missing");
        let err = extract_skill_zip(&mut archive, "fallback", &dest).unwrap_err();
        assert!(err.contains("未找到 SKILL.md"));
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn skill_name_is_sanitized() {
        assert_eq!(sanitize_skill_name("my-skill_1.2"), "my-skill_1.2");
        assert_eq!(sanitize_skill_name("  spaced name  "), "spacedname");
        assert_eq!(sanitize_skill_name("a/../b"), "a..b");
        assert_eq!(sanitize_skill_name("中文名"), "");
        assert_eq!(
            sanitize_skill_name(".."),
            "",
            "纯点名必须视为无效，防父目录注入"
        );
        assert_eq!(sanitize_skill_name(""), "");
    }

    #[test]
    fn frontmatter_name_with_illegal_chars_is_sanitized() {
        let mut archive = build_zip(&[("SKILL.md", "---\nname: my skill!@#\n---\n")]);
        let dest = temp_dest("sanitize");
        let name = extract_skill_zip(&mut archive, "fallback", &dest).unwrap();
        assert_eq!(name, "myskill");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn name_falls_back_to_zip_file_name_at_archive_root() {
        let mut archive = build_zip(&[("SKILL.md", "plain body, no frontmatter")]);
        let dest = temp_dest("fallback");
        let name = extract_skill_zip(&mut archive, "My Skill (v2)", &dest).unwrap();
        // ZIP 文件名兜底同样要过 sanitize
        assert_eq!(name, "MySkillv2");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn macos_junk_entries_are_skipped() {
        let mut archive = build_zip(&[
            ("__MACOSX/._SKILL.md", "junk"),
            (".DS_Store", "junk"),
            ("my-skill/.DS_Store", "junk"),
            ("my-skill/SKILL.md", "---\nname: clean\n---\n"),
        ]);
        let dest = temp_dest("macjunk");
        let name = extract_skill_zip(&mut archive, "fallback", &dest).unwrap();
        assert_eq!(name, "clean");
        assert!(!dest.join(".DS_Store").exists());
        let _ = fs::remove_dir_all(&dest);
    }
}

// ── Skills Commands ──────────────────────────────────

/// Search GitHub for skills repositories
#[tauri::command]
pub(crate) async fn search_github_skills(query: String) -> Result<Vec<CatalogSkill>, String> {
    let installed = get_installed_skill_names();
    let query_clone = query.clone();

    let results = tauri::async_runtime::spawn_blocking(move || {
        let client = build_http_client(15)?;

        let search_query = if query_clone.is_empty() {
            "claude+skills+SKILL.md".to_string()
        } else {
            format!("claude+skills+{}", query_clone.replace(' ', "+"))
        };

        let url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&per_page=20",
            search_query
        );

        let resp = client
            .get(&url)
            .send()
            .map_err(|e| format!("GitHub API error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned {}", resp.status()));
        }

        let body: serde_json::Value = resp
            .json::<serde_json::Value>()
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let mut skills = Vec::new();
        if let Some(items) = body
            .get("items")
            .and_then(|v: &serde_json::Value| v.as_array())
        {
            for item in items {
                let full_name = item
                    .get("full_name")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("");
                let desc = item
                    .get("description")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("");
                let stars = item
                    .get("stargazers_count")
                    .and_then(|v: &serde_json::Value| v.as_u64())
                    .unwrap_or(0);
                let default_branch = item
                    .get("default_branch")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("main");

                if full_name.is_empty() {
                    continue;
                }

                let html_url = item
                    .get("html_url")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("");
                // 使用 raw.githubusercontent.com 直接下载 SKILL.md
                let raw_url = format!(
                    "https://raw.githubusercontent.com/{}/{}/SKILL.md",
                    full_name, default_branch
                );
                skills.push(CatalogSkill {
                    name: full_name.split('/').last().unwrap_or(full_name).to_string(),
                    description: format!("{} ({}★)", desc, stars),
                    description_zh: format!("{} ({}★)", desc, stars),
                    download_url: raw_url,
                    source: full_name.to_string(),
                    category: "github".into(),
                    installed: false,
                    stars,
                    repo_url: html_url.to_string(),
                });
            }
        }

        Ok::<_, String>(skills)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    // Mark installed
    let mut results = results;
    for skill in &mut results {
        skill.installed = installed.contains(&skill.name);
    }

    Ok(results)
}
