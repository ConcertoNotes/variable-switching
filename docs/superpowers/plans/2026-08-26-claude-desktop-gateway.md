# Claude Desktop Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The user selected inline execution; do not dispatch subagents.

**Goal:** Add a first-class Claude Desktop provider surface that can write and restore Desktop 3P profiles, route third-party API traffic through VarSwitch's authenticated local gateway, map Claude role models, and support independent provider management.

**Architecture:** Store Claude Desktop providers in their own encrypted profile file and project the active provider into Claude Desktop's 3P config library. Reuse the existing `127.0.0.1:25789` process but add a `/claude-desktop` namespace, a separate runtime target/failover pool, a generated local bearer token, and role-model mapping so Claude Code and Desktop never overwrite one another.

**Tech Stack:** Rust 2021, Tauri 2 commands, `tiny_http`, `reqwest::blocking`, `serde_json`, Windows/macOS filesystem integration, static HTML/CSS/JavaScript, Node's built-in test runner.

## Global Constraints

- Preserve all unrelated working-tree changes. `public/*.js`, `public/index.html`, and several Rust files already contain user-owned CRLF changes; edit with small patches and review with `git diff --ignore-space-at-eol`.
- Do not add dependencies, databases, production deployment changes, or destructive filesystem operations.
- Keep the built-in Claude Desktop Official entry stable and non-deletable.
- Gateway authentication uses a generated `vsd-<uuid>` token; never treat `PROXY_MANAGED` or the third-party API key as the local gateway credential.
- Gateway listens only on the existing loopback address and port `127.0.0.1:25789`.
- Gateway supports the Claude formats already implemented by VarSwitch: `anthropic` and `openai_chat`. It does not add Responses or Gemini Native adapters.
- Windows and macOS write Desktop 3P profiles; Linux stores providers but returns a clear unsupported-platform error when enabling one.
- Never overwrite damaged JSON, unknown top-level keys, unrelated profiles, MCP configuration, shortcuts, or other Desktop settings.
- Every successful provider switch tells the user to fully quit and restart Claude Desktop. Gateway mode additionally tells the user to keep VarSwitch running.
- Run Cargo checks from `H:/variable-switching/src-tauri`.

## File Structure

- Create `src-tauri/src/claude_desktop_provider.rs`: provider types, encrypted persistence, validation, CRUD/import/order, 3P path resolution, profile projection, snapshots/rollback, status, Tauri commands.
- Create `src-tauri/src/claude_desktop_gateway.rs`: independent Desktop runtime state, gateway token validation, role model catalog/mapping, health facade, and adapters into the existing proxy forwarding functions.
- Modify `src-tauri/src/claude_proxy.rs`: route dispatch for Desktop models/messages and minimal shared forwarding visibility required by `claude_desktop_gateway`.
- Modify `src-tauri/src/secret_store.rs`: include `claude_desktop_profiles.json` in plaintext-secret migration.
- Modify `src-tauri/src/lib.rs`: register new modules/commands and restore active Desktop runtime state at startup.
- Modify `public/index.html`: add sidebar route, page, status area, provider list, and provider dialog.
- Modify `public/app.js`: state, localization, rendering, commands, form logic, imports, switching, sorting, and refresh lifecycle.
- Modify `public/style.css`: only Claude Desktop page/form/status styles that cannot reuse existing provider classes.
- Modify `public/app.test.js`: static UI and command-contract regressions.
- Modify `README.md`: describe Claude Desktop Gateway, restart requirement, paths, and supported formats.

---

### Task 1: Claude Desktop provider model and encrypted persistence

**Files:**
- Create: `src-tauri/src/claude_desktop_provider.rs`
- Modify: `src-tauri/src/secret_store.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/claude_desktop_provider.rs`

**Interfaces:**
- Produces: `ClaudeDesktopProfile`, `ClaudeDesktopProfilesData`, `ClaudeDesktopConnectionMode`, `claude_desktop_profiles_path`, `read_claude_desktop_profiles`, `write_claude_desktop_profiles`, CRUD/import/order Tauri commands.
- Consumes: `data_dir`, `encrypt_secret`, `decrypt_secret_or_keep`, `ensure_secret_usable`, `write_file_atomic`, `chrono_now`, and Claude `Profile` data from `crate::claude`.

- [ ] **Step 1: Write persistence and validation tests first**

Add tests that exercise path-level helpers without a real Tauri window:

```rust
#[test]
fn desktop_profiles_round_trip_encrypts_api_keys() {
    let path = temp_profiles_path("round-trip");
    let data = ClaudeDesktopProfilesData { profiles: vec![fixture_gateway_profile()] };
    write_profiles_to_path(&path, &data).unwrap();
    let raw = fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("sk-desktop-secret"));
    let loaded = read_profiles_from_path(&path);
    assert_eq!(loaded.profiles[0].api_key, "sk-desktop-secret");
}

#[test]
fn gateway_requires_one_resolvable_model() {
    let mut profile = fixture_gateway_profile();
    profile.model_id.clear();
    profile.sonnet_model.clear();
    profile.opus_model.clear();
    profile.haiku_model.clear();
    assert!(validate_profile(&profile).unwrap_err().contains("模型"));
}

#[test]
fn direct_mode_rejects_openai_chat() {
    let mut profile = fixture_gateway_profile();
    profile.connection_mode = ClaudeDesktopConnectionMode::Direct;
    profile.api_format = "openai_chat".into();
    assert!(validate_profile(&profile).is_err());
}
```

- [ ] **Step 2: Run the focused tests and confirm RED**

Run from `src-tauri`:

```bash
cargo test claude_desktop_provider --lib
```

Expected: compilation fails because the module/types/functions do not exist.

- [ ] **Step 3: Implement provider types and path-level persistence**

Define the stable contract:

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClaudeDesktopConnectionMode { Gateway, Direct, Official }

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeDesktopProfile {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub connection_mode: ClaudeDesktopConnectionMode,
    pub api_format: String,
    pub model_id: String,
    pub sonnet_model: String,
    pub opus_model: String,
    pub haiku_model: String,
    pub proxy_failover: bool,
    pub is_active: bool,
    pub created_at: String,
}

pub(crate) const CLAUDE_DESKTOP_OFFICIAL_ID: &str = "claude-desktop-official";
```

Implement encrypted read/write mirroring Claude profiles, with `write_file_atomic` and no double encryption. Add `claude_desktop_profiles.json` to `migrate_plaintext_secrets` and its `file_has_plaintext_secret` scan.

- [ ] **Step 4: Implement CRUD, unique-name import, and stable ordering**

Add commands with explicit backend validation:

```rust
get_claude_desktop_profiles(app: AppHandle) -> ClaudeDesktopProfilesData
add_claude_desktop_profile(app: AppHandle, input: ClaudeDesktopProfileInput) -> Result<ClaudeDesktopProfile, String>
update_claude_desktop_profile(app: AppHandle, id: String, input: ClaudeDesktopProfileInput) -> Result<ClaudeDesktopProfile, String>
delete_claude_desktop_profile(app: AppHandle, id: String) -> Result<(), String>
reorder_claude_desktop_profiles(app: AppHandle, ids: Vec<String>) -> Result<ClaudeDesktopProfilesData, String>
import_claude_profiles_to_desktop(app: AppHandle) -> Result<ClaudeDesktopProfilesData, String>
```

The official entry is synthesized at read time, remains first, and rejects edit/delete. Import copies all existing Claude profiles, maps `proxy_takeover || api_format == "openai_chat"` to Gateway, maps other Anthropic profiles to Direct, and generates `Name (2)` style unique names.

- [ ] **Step 5: Run provider tests and formatting**

```bash
cargo test claude_desktop_provider --lib
cargo fmt --check
```

Expected: focused tests pass and formatting reports no diff.

- [ ] **Step 6: Review the semantic diff before proceeding**

```bash
git diff --ignore-space-at-eol -- src-tauri/src/claude_desktop_provider.rs src-tauri/src/secret_store.rs src-tauri/src/lib.rs
```

Do not stage or commit pre-existing line-ending changes. Commit only if the staged diff contains exactly this task's semantic changes.

---

### Task 2: Transactional Claude Desktop 3P profile projection

**Files:**
- Modify: `src-tauri/src/claude_desktop_provider.rs`
- Test: `src-tauri/src/claude_desktop_provider.rs`

**Interfaces:**
- Consumes: `ClaudeDesktopProfile`, `ClaudeDesktopConnectionMode` from Task 1.
- Produces: `ClaudeDesktopPaths`, `ClaudeDesktopStatus`, `apply_active_profile_at_paths`, `restore_official_at_paths`, `get_claude_desktop_status`, `switch_claude_desktop_profile`, `sync_claude_desktop_profile`.

- [ ] **Step 1: Write path, profile, preservation, and rollback tests**

```rust
#[test]
fn gateway_projection_writes_local_url_and_generated_token() {
    let paths = temp_desktop_paths("gateway");
    let profile = fixture_gateway_profile();
    apply_profile_at_paths(&paths, &profile, "vsd-test-token").unwrap();
    let json: Value = read_json(&paths.profile_path).unwrap();
    assert_eq!(json["inferenceGatewayBaseUrl"], "http://127.0.0.1:25789/claude-desktop");
    assert_eq!(json["inferenceGatewayApiKey"], "vsd-test-token");
    assert!(!json.to_string().contains("sk-desktop-secret"));
}

#[test]
fn direct_projection_writes_upstream_credentials() {
    let paths = temp_desktop_paths("direct");
    let mut profile = fixture_gateway_profile();
    profile.connection_mode = ClaudeDesktopConnectionMode::Direct;
    profile.api_format = "anthropic".into();
    apply_profile_at_paths(&paths, &profile, "unused").unwrap();
    let json: Value = read_json(&paths.profile_path).unwrap();
    assert_eq!(json["inferenceGatewayBaseUrl"], profile.base_url);
    assert_eq!(json["inferenceGatewayApiKey"], profile.api_key);
}

#[test]
fn failed_profile_write_rolls_back_every_file() {
    let paths = temp_desktop_paths("rollback");
    seed_existing_desktop_files(&paths);
    fs::create_dir_all(&paths.profile_path).unwrap();
    let before = snapshot_paths(&paths).unwrap();
    assert!(apply_profile_at_paths(&paths, &fixture_gateway_profile(), "vsd-test").is_err());
    assert_snapshots_restored(&before);
}
```

- [ ] **Step 2: Run focused tests and confirm RED**

```bash
cargo test claude_desktop_provider --lib
```

Expected: new projection tests fail because the path/profile functions are absent.

- [ ] **Step 3: Implement platform paths and profile JSON**

Use one stable VarSwitch profile ID. Implement Windows paths under `LOCALAPPDATA`, macOS paths under `~/Library/Application Support`, and a testable `paths_from_base` helper. Gateway `inferenceModels` must expose only stable Claude-safe names; Direct must reject OpenAI Chat.

```rust
pub(crate) const VARSWITCH_DESKTOP_PROFILE_ID: &str = "00000000-0000-4000-8000-000000257890";

fn build_gateway_profile(profile: &ClaudeDesktopProfile, token: &str) -> Result<Value, String>;
fn build_direct_profile(profile: &ClaudeDesktopProfile) -> Result<Value, String>;
```

- [ ] **Step 4: Implement snapshot/rollback and official restoration**

Snapshot file bytes plus non-existence for both deployment configs, `_meta.json`, and the VarSwitch profile. Preserve all unrelated JSON keys. Official restoration sets deployment mode to `1p`, removes only the VarSwitch profile and its applied ID, and leaves all other profiles untouched.

- [ ] **Step 5: Implement gateway token persistence and switch commands**

Store one `vsd-<uuid>` in `claude_desktop_gateway.json` using private atomic write. `switch_claude_desktop_profile` validates provider, ensures the proxy listener for Gateway mode, projects files transactionally, then updates `is_active`; projection failure must leave the previous active flag unchanged.

- [ ] **Step 6: Run projection tests**

```bash
cargo test claude_desktop_provider --lib
cargo fmt --check
```

Expected: Gateway, Direct, Official, preservation, rollback, and active-state tests pass.

---

### Task 3: Authenticated Desktop Gateway routes and independent runtime

**Files:**
- Create: `src-tauri/src/claude_desktop_gateway.rs`
- Modify: `src-tauri/src/claude_proxy.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/claude_desktop_gateway.rs`
- Test: `src-tauri/src/claude_proxy.rs`

**Interfaces:**
- Consumes: active `ClaudeDesktopProfile`, token, model fields from Tasks 1-2; existing `claude_proxy::ProxyTarget` and conversion functions.
- Produces: `set_desktop_runtime`, `clear_desktop_runtime`, `desktop_gateway_status`, `validate_gateway_authorization`, `model_list_response`, `map_request_model`, `handle_desktop_gateway_request`.

- [ ] **Step 1: Write model mapping and auth tests**

```rust
#[test]
fn gateway_auth_accepts_only_matching_bearer() {
    assert!(validate_authorization(Some("Bearer vsd-good"), "vsd-good").is_ok());
    assert!(validate_authorization(Some("Bearer PROXY_MANAGED"), "vsd-good").is_err());
    assert!(validate_authorization(None, "vsd-good").is_err());
}

#[test]
fn role_mapping_prefers_role_then_default() {
    let runtime = fixture_runtime();
    assert_eq!(map_model("claude-opus-4-6", &runtime).unwrap(), "upstream-opus");
    assert_eq!(map_model("claude-haiku-4-5-20251001", &runtime).unwrap(), "upstream-default");
}

#[test]
fn desktop_runtime_does_not_replace_claude_runtime() {
    claude_proxy::set_upstream(Some(fixture_claude_upstream()));
    set_desktop_runtime(Some(fixture_runtime()));
    assert_eq!(claude_proxy::current_upstream().unwrap().model, "claude-code-model");
    assert_eq!(current_desktop_runtime().unwrap().profile.model_id, "upstream-default");
}
```

- [ ] **Step 2: Run gateway tests and confirm RED**

```bash
cargo test claude_desktop_gateway --lib
```

Expected: compile failure for the new module/interfaces.

- [ ] **Step 3: Implement independent Desktop runtime and model catalog**

```rust
pub(crate) struct ClaudeDesktopRuntime {
    pub profile: ClaudeDesktopProfile,
    pub token: String,
    pub primary: claude_proxy::ProxyTarget,
    pub failover: Vec<claude_proxy::ProxyTarget>,
}
```

Expose `claude-sonnet-*`, `claude-opus-*`, and `claude-haiku-*` role entries only when each role resolves. Unknown non-Claude routes return 400; dated role variants fall back by role keyword.

- [ ] **Step 4: Add request dispatch to the existing loopback server**

Extend `handle_request` in `claude_proxy.rs`:

```rust
match (request.method(), request.url().split('?').next().unwrap_or("")) {
    (&Method::Get, "/claude-desktop/v1/models") => claude_desktop_gateway::handle_models(request),
    (&Method::Post, "/claude-desktop/v1/messages") => claude_desktop_gateway::handle_messages(request),
    // existing Claude Code routes remain unchanged
}
```

Authenticate before reading/forwarding the body. For messages, rewrite the model and invoke the existing target/failover engine with the Desktop-specific pool.

- [ ] **Step 5: Ensure upstream authentication replaces the gateway token**

The outgoing request must use `ProxyUpstream.api_key`; it must never forward the incoming `Authorization: Bearer vsd-*`. Anthropic mode emits the existing upstream auth behavior; OpenAI Chat uses Bearer auth and existing conversion/streaming behavior.

- [ ] **Step 6: Add HTTP-level route tests**

Start the tiny_http server on the fixed port only when available; otherwise exercise handler helpers directly. Assert:

```text
GET /claude-desktop/v1/models without auth -> 401
GET /claude-desktop/v1/models with token -> 200 and safe ids
POST /claude-desktop/v1/messages with unknown model -> 400
```

- [ ] **Step 7: Run focused and existing proxy tests**

```bash
cargo test claude_desktop_gateway --lib
cargo test claude_proxy --lib
```

Expected: new Gateway tests pass and existing Claude Code proxy tests remain green.

---

### Task 4: Tauri command registration, startup restore, and status contracts

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/claude_desktop_provider.rs`
- Modify: `src-tauri/src/claude_desktop_gateway.rs`
- Test: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: commands/runtime from Tasks 1-3.
- Produces: frontend-callable command names and startup recovery of active Gateway state.

- [ ] **Step 1: Add a registration contract test**

Extend the existing Rust/static contract tests to assert all command identifiers exist exactly once:

```text
get_claude_desktop_profiles
add_claude_desktop_profile
update_claude_desktop_profile
delete_claude_desktop_profile
reorder_claude_desktop_profiles
import_claude_profiles_to_desktop
switch_claude_desktop_profile
sync_claude_desktop_profile
get_claude_desktop_provider_status
get_claude_desktop_gateway_health
claude_desktop_gateway_reset_breaker
```

- [ ] **Step 2: Run the contract test and confirm RED**

```bash
cargo test command_registration --lib
```

Expected: missing new command names.

- [ ] **Step 3: Register commands and restore runtime in setup**

During Tauri setup, read the active Desktop profile. For active Gateway profiles, configure the independent Desktop runtime and start the existing proxy server. For Direct/Official profiles, clear the Desktop runtime. Startup errors are logged with redacted context and surfaced in status, not allowed to crash unrelated applications.

- [ ] **Step 4: Implement a stable status response**

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeDesktopProviderStatus {
    pub installed: bool,
    pub supported: bool,
    pub mode: String,
    pub active_profile_id: Option<String>,
    pub active_profile_name: Option<String>,
    pub profile_path: Option<String>,
    pub gateway_running: bool,
    pub gateway_url: String,
    pub warning: Option<String>,
}
```

- [ ] **Step 5: Run registration and startup tests**

```bash
cargo test command_registration --lib
cargo test claude_desktop --lib
```

Expected: commands are callable, missing Desktop installation does not crash startup, and active Gateway state restores independently.

---

### Task 5: Claude Desktop page, navigation, and form markup

**Files:**
- Modify: `public/index.html`
- Modify: `public/style.css`
- Modify: `public/app.test.js`

**Interfaces:**
- Produces DOM IDs consumed by Task 6: `pageClaudeDesktop`, `claudeDesktopNav`, `claudeDesktopStatusGrid`, `claudeDesktopProfilesList`, `claudeDesktopAddBtn`, `claudeDesktopImportBtn`, `claudeDesktopSyncBtn`, `claudeDesktopProfileOverlay`, and form controls prefixed `claudeDesktopProfile`.

- [ ] **Step 1: Add failing static UI tests**

```javascript
test("Claude Desktop has a first-class provider page and gateway form", () => {
  const html = fs.readFileSync(require.resolve("./index.html"), "utf8");
  assert.match(html, /id="claudeDesktopNav"[^>]*data-console-page="claude-desktop"/);
  assert.match(html, /id="pageClaudeDesktop"[^>]*data-console-page-panel="claude-desktop"/);
  assert.match(html, /id="claudeDesktopImportBtn"/);
  assert.match(html, /id="claudeDesktopProfileConnectionMode"/);
  assert.match(html, /value="gateway"/);
  assert.match(html, /value="direct"/);
});
```

- [ ] **Step 2: Run the frontend test and confirm RED**

```bash
node --test public/app.test.js --test-name-pattern="Claude Desktop has"
```

Expected: assertion failure for missing DOM.

- [ ] **Step 3: Add the sidebar route and page using existing provider components**

Place Claude Desktop immediately after Claude Code. Reuse `.page-header`, `.status-grid`, `.profiles-grid`, `.profile-card`, `.empty-state`, and existing button classes. The built-in official card must have a product icon and no edit/delete buttons.

- [ ] **Step 4: Add the provider dialog**

Use existing modal/form classes. Include name, connection mode, API format, Base URL, API Key, default/Sonnet/Opus/Haiku model fields, and Gateway failover checkbox. Use semantic labels and no inline secrets in examples.

- [ ] **Step 5: Add only local styles**

Add styles for the Desktop mode badge, Gateway dependency warning, and compact connection-mode selector only when existing classes cannot express them. Verify dark-theme inheritance and narrow-width wrapping.

- [ ] **Step 6: Run the static frontend test**

```bash
node --test public/app.test.js --test-name-pattern="Claude Desktop has"
```

Expected: the new markup contract passes.

---

### Task 6: Frontend state, provider actions, localization, and refresh behavior

**Files:**
- Modify: `public/app.js`
- Modify: `public/app.test.js`
- Test: `public/app.test.js`

**Interfaces:**
- Consumes: Task 4 Tauri commands and Task 5 DOM IDs.
- Produces: `loadClaudeDesktopPage`, `renderClaudeDesktopStatus`, `renderClaudeDesktopProfiles`, form open/save handlers, import/switch/delete/reorder/sync actions.

- [ ] **Step 1: Add failing command and behavior contract tests**

```javascript
test("Claude Desktop frontend wires independent provider commands", () => {
  const app = fs.readFileSync(require.resolve("./app.js"), "utf8");
  for (const command of [
    "get_claude_desktop_profiles",
    "get_claude_desktop_provider_status",
    "switch_claude_desktop_profile",
    "import_claude_profiles_to_desktop",
  ]) assert.match(app, new RegExp(`invoke\\(["']${command}["']`));
  assert.match(app, /function renderClaudeDesktopProfiles/);
  assert.match(app, /function updateClaudeDesktopProfileFormState/);
});
```

- [ ] **Step 2: Run focused tests and confirm RED**

```bash
node --test public/app.test.js --test-name-pattern="Claude Desktop frontend"
```

- [ ] **Step 3: Add Chinese and English copy**

Add keys for page title/subtitle, status labels, installed/unsupported warnings, Official/Direct/Gateway modes, import result, restart requirement, keep-running requirement, validation errors, Gateway health, and destructive confirmations. Keep exact visible label `Claude Desktop`.

- [ ] **Step 4: Implement page loading and rendering**

Load status and profiles concurrently, render the official entry first, mask API keys using the existing helper, and show active/mode/Gateway badges. Selecting the sidebar route triggers a fresh load without affecting Claude Code state.

- [ ] **Step 5: Implement form state and backend actions**

Gateway mode permits both API formats and shows failover. Direct mode forces `anthropic` and hides failover. Save sends a single camelCase input object matching Task 1. Switching displays the returned restart instruction; Gateway activation refreshes proxy health.

- [ ] **Step 6: Implement import, delete, sorting, and sync**

Import confirms copying existing Claude providers and reports the count. Delete refuses the official or active entry before invoking the backend. Sorting excludes the official entry and uses the existing drag/reorder pattern. Sync re-projects the active profile and reports rollback-safe failures.

- [ ] **Step 7: Bind navigation and dialog events once**

Use the existing `on` helper and container-level delegated actions. Ensure repeated page refreshes do not add duplicate listeners.

- [ ] **Step 8: Run JS syntax and full frontend tests**

```bash
node --check public/app.js
node --test public/app.test.js
```

Expected: syntax passes and the full frontend suite is green.

---

### Task 7: Documentation, integration verification, and real UI behavior

**Files:**
- Modify: `README.md`
- Verify: all files from Tasks 1-6

**Interfaces:**
- Consumes the complete feature.
- Produces verification evidence and a user-facing operation guide.

- [ ] **Step 1: Document exact usage and limits**

Add a Claude Desktop section with:

```text
Claude Desktop -> add/import provider -> choose Gateway or Direct -> enable -> fully quit and restart Claude Desktop
Gateway URL: http://127.0.0.1:25789/claude-desktop
Supported Gateway upstreams: Anthropic Messages, OpenAI Chat Completions
Gateway requires VarSwitch to remain running; Direct does not
```

List Windows/macOS profile roots and Linux's unsupported activation state without exposing a token or API Key.

- [ ] **Step 2: Run focused Rust tests**

```bash
cargo test claude_desktop_provider --lib
cargo test claude_desktop_gateway --lib
cargo test claude_proxy --lib
```

- [ ] **Step 3: Run the complete Rust verification**

```bash
cargo fmt --check
cargo test
cargo check
```

Expected: all commands exit 0. Report exact failing test names if the existing environment blocks any command.

- [ ] **Step 4: Run complete frontend and diff verification**

From the repository root:

```bash
node --check public/app.js
node --test public/app.test.js
git diff --check
git diff --ignore-space-at-eol --stat
```

Review every semantic diff and confirm no unrelated line-ending churn was introduced.

- [ ] **Step 5: Build the Tauri application**

From the repository root:

```bash
npm exec tauri build -- --debug --no-bundle
```

Expected: debug executable builds successfully.

- [ ] **Step 6: Verify Profile projection in isolated directories**

Use Rust tests or a checked test helper with explicit temporary `LOCALAPPDATA`/HOME roots. Verify Gateway, Direct, Official, rollback, unrelated-key preservation, and no real user Desktop file changes.

- [ ] **Step 7: Verify the local HTTP gateway**

With a local deterministic stub upstream, verify models auth, unknown model rejection, Anthropic forwarding, OpenAI Chat conversion, and that upstream receives the real fixture key instead of `vsd-*`.

- [ ] **Step 8: Verify actual page behavior**

Launch the Tauri debug app, open `Claude Desktop`, and check desktop and narrow widths. Exercise add/edit form mode changes, import confirmation, status refresh, and non-destructive test switching against isolated paths. Confirm the browser console contains no new errors.

- [ ] **Step 9: Report external end-to-end limits honestly**

If the machine lacks Claude Desktop or a disposable third-party API Key, do not modify the user's real Desktop state and do not claim a completed external conversation. Report that automated profile/Gateway/stub verification passed and name the unavailable real-world check.

## Plan Self-Review

- Spec coverage: independent page, CRUD/import/order, Gateway/Direct/Official, paths, token, models/messages, mapping, failover, rollback, platform behavior, status, docs, automated tests, build, and UI verification are each assigned to a task.
- Placeholder scan: no deferred implementation markers or unspecified error-handling steps remain.
- Type consistency: frontend camelCase fields correspond to the Rust `#[serde(rename_all = "camelCase")]` input/status contracts; Gateway runtime consumes the profile types defined in Task 1.
- Scope consistency: only Anthropic Messages and OpenAI Chat are added to Desktop because those are the existing VarSwitch Claude adapters; Responses/Gemini Native remain explicit non-goals.
