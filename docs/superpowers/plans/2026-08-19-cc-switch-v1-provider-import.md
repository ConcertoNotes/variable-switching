# CC Switch v1 Provider 导入兼容 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在保留现有 Profile/MCP/Grok 深链行为的同时，让 VarSwitch 完整支持 `varswitch://v1/import` 的 CC Switch v1 Provider 查询参数合同、确认预览和安全的同名处理。

**Architecture:** Rust 后端使用 `reqwest::Url` 统一解析自定义 URL，并将旧 Base64 payload 与新查询参数格式归一化为现有 `DeepLinkImport`。同名判断与最终写入决策都在后端完成，前端只展示脱敏确认信息并传递 `rename`/`overwrite` 选择；可独立测试的界面映射放进 `public/app-helpers.js`。

**Tech Stack:** Rust 2021、Tauri 2、`reqwest::Url`、Serde JSON、原生 JavaScript、Node.js `node:test`、Windows 自定义协议。

## Global Constraints

- 保留 `varswitch://import/profile` 和 `varswitch://import/mcp` 的既有合同与行为。
- v1 只接受 `resource=provider` 与 `app=claude|codex|gemini`；不扩展 Grok 或 MCP。
- v1 必填字段为 `name`、`endpoint`、`apiKey`、`model`、`homepage`、`enabled=true`。
- Endpoint 只能使用绝对 `http://` 或 `https://` URL。
- 查询参数只解码一次；支持 `+` 空格、UTF-8 中文和百分号编码。
- 未知参数忽略；未知版本、action、resource、app 必须拒绝。
- API Key 不得出现在日志、完整 URL 日志、确认框明文或测试使用的真实凭据中。
- `enabled=true` 不自动激活或切换 Provider。
- v1 同名时默认重命名；只有用户明确选择后才能覆盖。
- 覆盖保留原 Profile ID、创建时间、激活状态及协议未提供的应用专属设置。
- 不新增依赖、数据库或远端服务，不重构无关页面与 Provider 管理逻辑。
- 保留工作区现有未提交内容；每次暂存和提交只包含任务列出的文件。

---

### Task 1: 双协议解析与 v1 参数归一化

**Files:**
- Modify: `src-tauri/src/deeplink.rs:5-204`
- Test: `src-tauri/src/deeplink.rs:406-508`

**Interfaces:**
- Consumes: `reqwest::Url`、现有 `validate_profile_payload`、`decode_base64url`。
- Produces: `parse_deep_link_url(raw: &str) -> Result<DeepLinkImport, String>`；`DeepLinkImport.source: String`，值为 `legacy` 或 `cc_switch_v1`；v1 `data` 使用现有 `name/apiKey/baseUrl/model/haikuModel/sonnetModel/opusModel` 字段，并保留 `homepage/enabled`。

- [ ] **Step 1: 写入 v1 成功解析的失败测试**

在 `deep_link_tests` 中加入使用字面量期望值的测试：

```rust
#[test]
fn parse_cc_switch_v1_provider_urls() {
    let claude = parse_deep_link_url(
        "varswitch://v1/import?resource=provider&app=claude&name=Team+Claude&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test-claude&model=claude-sonnet-4&haikuModel=claude-haiku-4&sonnetModel=claude-sonnet-4&opusModel=claude-opus-4&homepage=https%3A%2F%2Fapi.example.com&enabled=true",
    ).expect("合法 Claude v1 深链应解析成功");
    assert_eq!(claude.source, "cc_switch_v1");
    assert_eq!(claude.kind, "profile");
    assert_eq!(claude.app, "claude");
    assert_eq!(claude.data["name"], "Team Claude");
    assert_eq!(claude.data["baseUrl"], "https://api.example.com/");
    assert_eq!(claude.data["apiKey"], "sk-test-claude");
    assert_eq!(claude.data["model"], "claude-sonnet-4");
    assert_eq!(claude.data["haikuModel"], "claude-haiku-4");
    assert_eq!(claude.data["sonnetModel"], "claude-sonnet-4");
    assert_eq!(claude.data["opusModel"], "claude-opus-4");
    assert_eq!(claude.data["homepage"], "https://api.example.com");
    assert_eq!(claude.data["enabled"], true);

    for app in ["codex", "gemini"] {
        let raw = format!(
            "varswitch://v1/import?resource=provider&app={app}&name=Provider&endpoint=https%3A%2F%2Fapi.example.com%2Fv1&apiKey=sk-test&model=model-1&homepage=https%3A%2F%2Fapi.example.com&enabled=true"
        );
        let import = parse_deep_link_url(&raw).expect("合法 v1 深链应解析成功");
        assert_eq!(import.source, "cc_switch_v1");
        assert_eq!(import.app, app);
        assert_eq!(import.data["baseUrl"], "https://api.example.com/v1");
    }
}
```

- [ ] **Step 2: 运行测试并确认因 v1 尚未支持而失败**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml parse_cc_switch_v1_provider_urls -- --nocapture
```

Expected: FAIL；当前解析器把位置识别为 `v1/import` 后返回“不支持的深链路径”，或编译阶段提示 `DeepLinkImport` 尚无 `source` 字段。

- [ ] **Step 3: 写入解码和合同拒绝测试**

```rust
#[test]
fn cc_switch_v1_decodes_form_query_once() {
    let import = parse_deep_link_url(
        "varswitch://v1/import?resource=provider&app=gemini&name=%E4%B8%AD%E6%96%87+%25+Provider&endpoint=https%3A%2F%2Fapi.example.com%2F%252Fkeep&apiKey=sk-test%2525key&model=gemini-2.5-pro&homepage=https%3A%2F%2Fapi.example.com&enabled=true",
    ).expect("form query 应正确解码");
    assert_eq!(import.data["name"], "中文 % Provider");
    assert_eq!(import.data["baseUrl"], "https://api.example.com/%2Fkeep");
    assert_eq!(import.data["apiKey"], "sk-test%25key");
}

#[test]
fn cc_switch_v1_rejects_invalid_contract_values() {
    let valid = "name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true";
    for prefix in [
        "varswitch://v2/import?resource=provider&app=claude",
        "varswitch://v1/export?resource=provider&app=claude",
        "varswitch://v1/import?resource=mcp&app=claude",
        "varswitch://v1/import?resource=provider&app=grok",
    ] {
        assert!(parse_deep_link_url(&format!("{prefix}&{valid}")).is_err());
    }
    assert!(parse_deep_link_url(
        "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint=file%3A%2F%2F%2FC%3A%2Fevil&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"
    ).unwrap_err().contains("HTTP"));
    assert!(parse_deep_link_url(
        "varswitch://v1/import?resource=provider&app=codex&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=false"
    ).unwrap_err().contains("enabled"));
}
```

缺失字段与未知参数使用以下表驱动测试，期望值不复用生产校验逻辑：

```rust
#[test]
fn cc_switch_v1_requires_every_contract_field_and_ignores_unknown_fields() {
    let cases = [
        ("name", "resource=provider&app=claude&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
        ("endpoint", "resource=provider&app=claude&name=n&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
        ("apiKey", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
        ("model", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&homepage=https%3A%2F%2Fapi.example.com&enabled=true"),
        ("homepage", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&enabled=true"),
        ("enabled", "resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com"),
    ];
    for (field, query) in cases {
        let error = parse_deep_link_url(&format!("varswitch://v1/import?{query}"))
            .expect_err("缺少必填参数必须拒绝");
        assert!(error.contains(field), "{field} 的错误应点明字段：{error}");
    }

    let accepted = parse_deep_link_url(
        "varswitch://v1/import?resource=provider&app=claude&name=n&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test&model=m&homepage=https%3A%2F%2Fapi.example.com&enabled=true&futureField=ignored",
    );
    assert!(accepted.is_ok(), "未知参数必须被忽略");
}
```

- [ ] **Step 4: 运行新增测试并确认预期失败**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml cc_switch_v1_ -- --nocapture
```

Expected: FAIL，失败原因来自缺少 v1 解析、`+` 解码或必填合同校验，不得是测试拼写错误。

- [ ] **Step 5: 用 `reqwest::Url` 实现统一 URL 解析和 v1 归一化**

将 `DeepLinkImport` 扩展为：

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeepLinkImport {
    pub(crate) kind: String,
    pub(crate) app: String,
    pub(crate) data: serde_json::Value,
    pub(crate) source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) conflict: Option<DeepLinkConflict>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeepLinkConflict {
    pub(crate) existing_name: String,
    pub(crate) suggested_name: String,
}
```

先令所有旧格式构造返回 `source: "legacy"` 与 `conflict: None`。新增查询收集、必填值和 Endpoint 校验函数：

```rust
fn query_params(url: &reqwest::Url) -> HashMap<String, String> {
    url.query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn required_param(params: &HashMap<String, String>, key: &str) -> Result<String, String> {
    let value = params.get(key).map(|value| value.trim()).unwrap_or("");
    if value.is_empty() {
        Err(format!("缺少必填参数 {key}"))
    } else {
        Ok(value.to_string())
    }
}

fn parse_http_endpoint(raw: &str) -> Result<String, String> {
    let endpoint = reqwest::Url::parse(raw)
        .map_err(|_| "endpoint 必须是合法的绝对 HTTP(S) URL".to_string())?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err("endpoint 必须使用 HTTP 或 HTTPS".into());
    }
    Ok(endpoint.to_string())
}
```

实现 `parse_cc_switch_v1_provider`，严格校验 `resource/app/enabled`，并构造：

```rust
serde_json::json!({
    "name": name,
    "apiKey": api_key,
    "baseUrl": endpoint,
    "model": model,
    "haikuModel": optional_param(&params, "haikuModel"),
    "sonnetModel": optional_param(&params, "sonnetModel"),
    "opusModel": optional_param(&params, "opusModel"),
    "homepage": homepage,
    "enabled": true,
})
```

`parse_deep_link_url` 先执行 `reqwest::Url::parse` 与 scheme 校验，再按 `(host_str(), path())` 分派：`("v1", "/import")` 进入新解析器，其余位置进入保留的旧解析器。对于 host 为 `v1` 但 path 不是 `/import`、或 host 形如 `v2` 的版本路径，直接返回版本/action 错误，不能落入旧协议。

- [ ] **Step 6: 运行 Rust 深链测试并保持旧格式通过**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml deep_link_tests -- --nocapture
```

Expected: PASS；原有 Profile、MCP、大小写、padding、非法 payload 和唯一名称测试全部保留并通过。

- [ ] **Step 7: 格式化并提交解析器任务**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git add -- src-tauri/src/deeplink.rs
git commit -m "feat: 兼容 CC Switch v1 Provider 深链"
```

Expected: `cargo fmt --check` exit 0；提交只包含 `src-tauri/src/deeplink.rs`。

---

### Task 2: 后端同名检测、重命名与安全覆盖

**Files:**
- Modify: `src-tauri/src/deeplink.rs:15-405`
- Test: `src-tauri/src/deeplink.rs` 的 `deep_link_tests`

**Interfaces:**
- Consumes: Task 1 的 `DeepLinkImport.source/conflict` 与归一化 `data`。
- Produces: `resolve_profile_import(existing, wanted, source, action) -> Result<ProfileImportResolution, String>`；`apply_deep_link_import(handle, kind, app, data, source, conflict_action)`；事件 payload 中可选 `conflict: { existingName, suggestedName }`。

- [ ] **Step 1: 写入冲突决策的失败测试**

```rust
#[test]
fn v1_conflict_defaults_to_unique_rename_and_requires_existing_overwrite_target() {
    let existing = vec!["Team".to_string(), "Team (2)".to_string()];
    assert_eq!(
        resolve_profile_import(&existing, "Team", "cc_switch_v1", Some("rename")).unwrap(),
        ProfileImportResolution::Add("Team (3)".into())
    );
    assert_eq!(
        resolve_profile_import(&existing, "Team", "cc_switch_v1", Some("overwrite")).unwrap(),
        ProfileImportResolution::Overwrite("Team".into())
    );
    assert!(resolve_profile_import(
        &["Other".into()], "Team", "cc_switch_v1", Some("overwrite")
    ).unwrap_err().contains("已变化"));
    assert!(resolve_profile_import(
        &existing, "Team", "legacy", Some("overwrite")
    ).is_err());
}

#[test]
fn conflict_metadata_exposes_only_names() {
    assert_eq!(
        detect_profile_conflict(&["Team".into(), "Team (2)".into()], "Team"),
        Some(DeepLinkConflict {
            existing_name: "Team".into(),
            suggested_name: "Team (3)".into(),
        })
    );
}
```

- [ ] **Step 2: 运行测试并确认缺少冲突接口**

```bash
cargo test --manifest-path src-tauri/Cargo.toml v1_conflict_defaults -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml conflict_metadata_exposes -- --nocapture
```

Expected: FAIL，编译器指出 `ProfileImportResolution`、`resolve_profile_import` 或 `detect_profile_conflict` 尚不存在。

- [ ] **Step 3: 实现纯冲突决策并接入事件准备阶段**

实现：

```rust
#[derive(Debug, PartialEq)]
pub(crate) enum ProfileImportResolution {
    Add(String),
    Overwrite(String),
}

pub(crate) fn detect_profile_conflict(
    existing: &[String],
    wanted: &str,
) -> Option<DeepLinkConflict> {
    existing.iter().any(|name| name == wanted).then(|| DeepLinkConflict {
        existing_name: wanted.to_string(),
        suggested_name: unique_import_name(existing, wanted),
    })
}
```

`resolve_profile_import` 的固定规则：

- 名称无冲突时只允许 `Add(wanted)`；若请求 overwrite，返回“同名配置已变化，请重新发起导入”。
- v1 冲突且 action 为 `overwrite` 时返回 `Overwrite(wanted)`。
- 其他情况（包括 action 缺失或 `rename`）返回 `Add(unique_import_name(...))`。
- `legacy + overwrite` 与未知 action 返回错误。

新增 `existing_profile_names(handle, app)`，分别调用 `read_profiles`、`read_codex_profiles`、`read_gemini_profiles`。`handle_deep_link_url` 解析成功后，对 `source == "cc_switch_v1" && kind == "profile"` 的导入附加冲突元数据，再 emit；日志仍只能使用问号前的 `visible`。

- [ ] **Step 4: 写入覆盖字段保留规则的失败测试**

为三个应用新增纯映射 helper，并用完整 Profile fixture 验证协议未提供字段不被清空。测试至少断言：

```rust
#[test]
fn overwrite_codex_fields_preserves_identity_activation_and_local_options() {
    let existing = CodexProfile {
        id: "id-1".into(),
        name: "Team".into(),
        api_key: "old-key".into(),
        base_url: "https://old.example.com/v1".into(),
        auth_mode: "api_key".into(),
        wire_api: "responses".into(),
        model: "old-model".into(),
        provider_name: "local-provider".into(),
        image_api_key: "image-key".into(),
        image_base_url: "https://images.example.com".into(),
        is_active: true,
        created_at: "2026-08-19 10:00:00".into(),
    };
    let updated = merge_codex_v1_profile(existing, &serde_json::json!({
        "name": "Team", "apiKey": "new-key",
        "baseUrl": "https://new.example.com/v1", "model": "gpt-5-codex"
    })).unwrap();
    assert_eq!(updated.id, "id-1");
    assert_eq!(updated.created_at, "2026-08-19 10:00:00");
    assert!(updated.is_active);
    assert_eq!(updated.api_key, "new-key");
    assert_eq!(updated.base_url, "https://new.example.com/v1");
    assert_eq!(updated.model, "gpt-5-codex");
    assert_eq!(updated.provider_name, "local-provider");
    assert_eq!(updated.image_api_key, "image-key");
}
```

Claude fixture还要断言 `api_format/proxy_failover/proxy_takeover` 保留，角色模型按导入值更新；Gemini fixture断言 ID、创建时间、激活状态保留。

具体测试体如下：

```rust
#[test]
fn overwrite_claude_fields_preserves_identity_activation_and_proxy_options() {
    let existing = Profile {
        id: "claude-id".into(), name: "Team".into(), api_key: "old".into(),
        base_url: "https://old.example.com".into(), model_id: "old-model".into(),
        sonnet_model: "old-sonnet".into(), opus_model: "old-opus".into(),
        haiku_model: "old-haiku".into(), api_format: "openai_chat".into(),
        proxy_failover: true, proxy_takeover: true, is_active: true,
        created_at: "2026-08-19 10:00:00".into(),
    };
    let updated = merge_claude_v1_profile(existing, &serde_json::json!({
        "name": "Team", "apiKey": "new", "baseUrl": "https://new.example.com/",
        "model": "primary", "sonnetModel": "sonnet", "opusModel": "opus",
        "haikuModel": "haiku"
    })).unwrap();
    assert_eq!(updated.id, "claude-id");
    assert_eq!(updated.created_at, "2026-08-19 10:00:00");
    assert!(updated.is_active);
    assert_eq!(updated.api_format, "openai_chat");
    assert!(updated.proxy_failover);
    assert!(updated.proxy_takeover);
    assert_eq!(updated.model_id, "primary");
    assert_eq!(updated.sonnet_model, "sonnet");
    assert_eq!(updated.opus_model, "opus");
    assert_eq!(updated.haiku_model, "haiku");
}

#[test]
fn overwrite_gemini_fields_preserves_identity_and_activation() {
    let existing = GeminiProfile {
        id: "gemini-id".into(), name: "Team".into(), api_key: "old".into(),
        base_url: "https://old.example.com".into(), model: "old-model".into(),
        is_active: true, created_at: "2026-08-19 10:00:00".into(),
    };
    let updated = merge_gemini_v1_profile(existing, &serde_json::json!({
        "name": "Team", "apiKey": "new", "baseUrl": "https://new.example.com/",
        "model": "gemini-2.5-pro"
    })).unwrap();
    assert_eq!(updated.id, "gemini-id");
    assert_eq!(updated.created_at, "2026-08-19 10:00:00");
    assert!(updated.is_active);
    assert_eq!(updated.api_key, "new");
    assert_eq!(updated.base_url, "https://new.example.com");
    assert_eq!(updated.model, "gemini-2.5-pro");
}
```

- [ ] **Step 5: 运行覆盖测试并确认 helper 尚不存在**

```bash
cargo test --manifest-path src-tauri/Cargo.toml overwrite_ -- --nocapture
```

Expected: FAIL，缺少 `merge_claude_v1_profile`、`merge_codex_v1_profile`、`merge_gemini_v1_profile`。

- [ ] **Step 6: 实现覆盖 helper 与写入分支**

三个 helper 接收现有 Profile 所有权和归一化 `data`，只替换协议字段。Claude 使用 `resolve_base_url_or_default` 并更新主模型与三个角色模型；Codex 使用 `resolve_base_url_or_default` 并更新主模型；Gemini trim Endpoint 尾部 `/` 的行为与 `add_gemini_profile` 保持一致。所有 helper 都保留 ID、`created_at`、`is_active` 和上一步测试列出的应用专属字段。

扩展命令签名：

```rust
#[tauri::command]
pub(crate) fn apply_deep_link_import(
    handle: tauri::AppHandle,
    kind: String,
    app: String,
    data: serde_json::Value,
    source: Option<String>,
    conflict_action: Option<String>,
) -> Result<String, String>
```

Profile 分支在写入前重新读取当前名称并调用 `resolve_profile_import`。`Add(name)` 继续复用现有 `add_*`；`Overwrite(name)` 再按精确名称定位现有 Profile，调用对应 merge helper，替换原 vector 项并调用现有 `write_*_profiles`。找不到目标时返回“同名配置已变化，请重新发起导入”，不得回退为新增或覆盖其他条目。

旧调用不传 `source/conflictAction` 时使用 `source="legacy"`，保持自动唯一命名新增。v1 在命令阶段重新验证 `homepage/enabled/model/endpoint` 已存在且有效，防止绕过事件解析直接 invoke。

- [ ] **Step 7: 运行后端测试与格式检查**

```bash
cargo test --manifest-path src-tauri/Cargo.toml deep_link_tests -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Expected: PASS，且测试输出不包含任何完整深链或非测试 API Key。

- [ ] **Step 8: 提交同名处理任务**

```bash
git add -- src-tauri/src/deeplink.rs
git commit -m "feat: 安全处理深链配置重名"
```

Expected: 提交只包含 `src-tauri/src/deeplink.rs`。

---

### Task 3: 确认弹窗字段、冲突选择与敏感状态清理

**Files:**
- Modify: `public/app-helpers.js`
- Modify: `public/app.test.js`
- Modify: `public/index.html:1474-1497`
- Modify: `public/app.js:10594-10703`
- Modify: `public/style.css:5252-5315`

**Interfaces:**
- Consumes: 后端事件 payload `{ kind, app, data, source, conflict? }`。
- Produces: `getDeepLinkImportView(payload)`、`buildDeepLinkApplyRequest(payload, conflictAction)`；Tauri invoke 参数 `{ kind, app, data, source, conflictAction }`。

- [ ] **Step 1: 为纯界面映射写失败测试**

把新 helper 加入 `public/app.test.js` 的 require 解构，并加入：

```javascript
test("CC Switch v1 deep link view masks secrets and exposes provider fields", () => {
  const view = getDeepLinkImportView({
    kind: "profile",
    app: "claude",
    source: "cc_switch_v1",
    conflict: { existingName: "Team", suggestedName: "Team (2)" },
    data: {
      name: "Team",
      apiKey: "sk-1234567890abcd",
      baseUrl: "https://api.example.com/",
      model: "claude-sonnet-4",
      haikuModel: "claude-haiku-4",
      sonnetModel: "claude-sonnet-4",
      opusModel: "claude-opus-4",
      homepage: "https://api.example.com",
    },
  });
  assert.equal(view.apiKeyMasked, "sk-123...abcd");
  assert.equal(view.showProviderDetails, true);
  assert.equal(view.showClaudeModels, true);
  assert.equal(view.showConflict, true);
  assert.equal(view.suggestedName, "Team (2)");
  assert.equal(view.defaultConflictAction, "rename");
});

test("legacy MCP deep link view keeps provider-only controls hidden", () => {
  const view = getDeepLinkImportView({
    kind: "mcp", app: "", source: "legacy",
    data: { name: "context7", config: { command: "npx" } },
  });
  assert.equal(view.showProviderDetails, false);
  assert.equal(view.showClaudeModels, false);
  assert.equal(view.showConflict, false);
});

test("deep link apply request permits only the visible v1 conflict choice", () => {
  const payload = {
    kind: "profile", app: "codex", source: "cc_switch_v1",
    conflict: { existingName: "Team", suggestedName: "Team (2)" },
    data: { name: "Team" },
  };
  assert.deepEqual(buildDeepLinkApplyRequest(payload, "overwrite"), {
    kind: "profile", app: "codex", data: { name: "Team" },
    source: "cc_switch_v1", conflictAction: "overwrite",
  });
  assert.equal(buildDeepLinkApplyRequest(payload, "invalid").conflictAction, "rename");
});
```

- [ ] **Step 2: 运行 Node 测试并确认新 helper 缺失**

```bash
node --test --test-name-pattern="deep link" public/app.test.js
```

Expected: FAIL，`getDeepLinkImportView` 或 `buildDeepLinkApplyRequest` 未定义。

- [ ] **Step 3: 在 `app-helpers.js` 实现并导出纯 helper**

移动现有 `maskDeepLinkSecret` 的规则到 helper，并让 `getDeepLinkImportView` 返回明确字段：

```javascript
{
  isMcp, appLabel, name, baseUrl, apiKeyMasked,
  model, haikuModel, sonnetModel, opusModel, homepage,
  configText, showProviderDetails, showClaudeModels,
  showHomepage, showConflict, existingName, suggestedName,
  defaultConflictAction: "rename"
}
```

`buildDeepLinkApplyRequest` 只有在 `source === "cc_switch_v1"` 且存在 `conflict` 时才允许 `overwrite`，其他输入统一发送 `rename` 或 `null`。不得复制或序列化原始 URL。

- [ ] **Step 4: 运行 helper 测试并确认通过**

```bash
node --test --test-name-pattern="deep link" public/app.test.js
```

Expected: PASS。

- [ ] **Step 5: 写入真实弹窗结构的失败测试**

读取 `index.html`，用每个唯一 ID 的存在性和表单语义保护用户可见行为：

```javascript
test("deep link confirmation dialog exposes v1 fields and an explicit conflict choice", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  for (const id of [
    "deeplinkModelRow", "deeplinkHaikuModelRow", "deeplinkSonnetModelRow",
    "deeplinkOpusModelRow", "deeplinkHomepageRow", "deeplinkConflictGroup",
  ]) {
    assert.match(html, new RegExp(`id="${id}"`));
  }
  assert.match(html, /name="deeplinkConflictAction"[^>]*value="rename"[^>]*checked/);
  assert.match(html, /name="deeplinkConflictAction"[^>]*value="overwrite"/);
});
```

- [ ] **Step 6: 运行结构测试并确认字段尚不存在**

```bash
node --test --test-name-pattern="deep link confirmation dialog" public/app.test.js
```

Expected: FAIL，缺少模型、Homepage 或冲突控件 ID。

- [ ] **Step 7: 扩展 HTML、局部样式和 `openDeepLinkModal`**

在现有 Base URL/API Key 行之间增加主模型、Claude 三个角色模型和 Homepage 行。冲突区使用 `<fieldset id="deeplinkConflictGroup" hidden>`，含两个同名 radio：默认 checked 的 `rename` 和 `overwrite`；文案必须明确重命名后的建议名称与覆盖风险。

`openDeepLinkModal` 通过 `helpers.getDeepLinkImportView(payload)` 渲染，使用 `textContent`/`setText`，不使用包含协议数据的 `innerHTML`。非 Claude 隐藏三个角色模型；旧 Profile 可显示已有主模型但隐藏 Homepage/冲突；MCP 保持现有 config preview。

`confirmDeepLinkImport` 从 checked radio 读取动作，调用：

```javascript
const request = helpers.buildDeepLinkApplyRequest(
  pendingDeepLinkImport,
  document.querySelector('input[name="deeplinkConflictAction"]:checked')?.value
);
const message = await invoke("apply_deep_link_import", request);
```

失败时保留弹窗供重试；成功、取消、关闭时执行 `pendingDeepLinkImport = null`，同时把所有展示值重置为 `--`、config preview 清空、radio 恢复 rename，降低敏感数据在 DOM 中残留的时间。

- [ ] **Step 8: 运行完整前端测试**

```bash
node --test public/app.test.js
```

Expected: 全部 PASS，0 failures；document ID 唯一性和 CSS 括号检查继续通过。

- [ ] **Step 9: 提交前端确认流程**

```bash
git add -- public/app-helpers.js public/app.test.js public/index.html public/app.js public/style.css
git commit -m "feat: 完善 Provider 深链导入确认"
```

Expected: 提交只包含上述五个前端文件。

---

### Task 4: 用户文档、完整回归与 Windows 实际协议验收

**Files:**
- Modify: `README.md:218-226`
- Verify: `src-tauri/tauri.conf.json:48-55`
- Verify: `src-tauri/src/lib.rs:1646-1709`
- Verify: `src-tauri/src/deeplink.rs`
- Verify: `public/app.js`

**Interfaces:**
- Consumes: Tasks 1-3 的完整双协议导入流程。
- Produces: 面向用户的两套协议说明，以及冷启动/运行中唤醒/取消/重命名/覆盖的验收证据。

- [ ] **Step 1: 更新 README 的 Deep Link 合同**

保留两条旧格式说明，并新增：

```markdown
- `varswitch://v1/import?resource=provider&app=<claude|codex|gemini>&name=...&endpoint=...&apiKey=...&model=...&homepage=...&enabled=true`：兼容 CC Switch v1 Provider Import 查询参数；Claude 还可传 `haikuModel`、`sonnetModel`、`opusModel`。
- v1 同名配置默认重命名后新增，也可在确认框中明确选择覆盖；无论 `enabled` 为 `true`，导入都不会自动激活或切换。
```

确认 README 继续说明 API Key 脱敏、导入前确认和旧 MCP 格式。

- [ ] **Step 2: 运行静态、单元和格式验证**

```bash
node --test public/app.test.js
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

Expected: 每条命令 exit 0；Node 与 Rust 测试均为 0 failures。

- [ ] **Step 3: 运行 Tauri debug 构建**

```bash
npm exec tauri build -- --debug --no-bundle
```

Expected: exit 0，并生成 `src-tauri/target/debug/varswitch.exe`；不得把生成物加入 Git。

- [ ] **Step 4: 启动开发应用并验证运行中深链**

先使用 `computer-use` 技能检查并操作实际 Tauri 窗口。用 Git Bash 启动开发链：

```bash
cmd.exe /c "dev.bat --skip-install --fast --no-pause"
```

应用窗口出现且日志显示协议注册完成后，在另一个 Git Bash 会话执行：

```bash
powershell.exe -NoProfile -Command "Start-Process 'varswitch://v1/import?resource=provider&app=claude&name=Protocol+Test&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-protocol-test&model=claude-sonnet-4&haikuModel=claude-haiku-4&homepage=https%3A%2F%2Fapi.example.com&enabled=true'"
```

Expected: 已有窗口被聚焦且没有第二个 VarSwitch 实例；确认框展示 `Protocol Test`、正确 Endpoint/模型/Homepage，API Key 只能显示为脱敏形式。先点取消，刷新 Claude 列表确认没有新增。

- [ ] **Step 5: 验证重命名与覆盖**

再次打开同一假密钥链接并确认导入，得到 `Protocol Test`。第三次打开同一链接，确认冲突区默认选择 `重命名后导入`，导入后得到 `Protocol Test (2)`。第四次将 URL 的 `model` 改为 `claude-opus-4`，选择 `覆盖现有配置`；确认 `Protocol Test` 的模型更新、ID/激活状态不变，`Protocol Test (2)` 未被修改。

测试完成后通过界面删除两条 `Protocol Test` 测试配置，避免污染用户数据；不得用脚本直接修改配置 JSON。

- [ ] **Step 6: 验证冷启动和单实例转发**

通过托盘正常退出 VarSwitch，然后执行同一条 `Start-Process` 命令。Expected: Windows 启动一个 VarSwitch 实例，前端就绪后出现确认框；取消后无写入。保持应用运行再次执行命令，任务管理器中仍只有一个 `varswitch.exe`，现有窗口收到新事件。

- [ ] **Step 7: 检查前端控制台和敏感日志**

在 Tauri 开发窗口用 `Ctrl+Shift+I` 打开 DevTools Console，确认上述交互期间没有新增 error/warning。通过设置页的“打开日志目录”定位实际 `varswitch.log`，然后在该目录运行：

```bash
rg -n 'sk-protocol-test|varswitch://v1/import\?' varswitch.log varswitch.log.old 2>/dev/null
```

Expected: 无匹配；允许出现不含查询参数的 `varswitch://v1/import` 位置日志，但不得出现 `?` 或测试 API Key。

- [ ] **Step 8: 检查最终变更边界并提交文档**

```bash
git diff --check
git status --short
git diff -- README.md
git add -- README.md
git commit -m "docs: 说明 v1 Provider 深链导入"
```

Expected: README 提交不包含 `VARSWITCH_PROTOCOL.md`、下载站文件、生成物或任何进入任务前已存在的改动。

- [ ] **Step 9: 最终需求逐项复核**

逐条对照 `docs/superpowers/specs/2026-08-19-cc-switch-v1-provider-import-design.md`：协议注册、双格式解析、字段映射、确认预览、取消不写入、同名重命名/覆盖、加密写入、不自动激活、单实例、日志脱敏、Node 测试、Rust 测试、debug 构建与实际 Windows 行为必须都有本次执行证据。任何未执行项都要在交付中明确列出，不能用其他检查替代。
