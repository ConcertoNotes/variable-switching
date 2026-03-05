# Editor Detection And Path Overrides Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve mainstream VS Code-like editor detection and let users edit, browse, reset, and open each editor's settings path from the Settings panel.

**Architecture:** Move editor path resolution behind shared Rust helpers that merge built-in defaults with user overrides from app settings. Use those helpers consistently for detection, status reads, switching, snapshots, restore, and settings UI data. Keep the frontend simple by rendering editor path rows from structured backend data and persisting changes through existing app settings commands.

**Tech Stack:** Tauri v2, Rust, native HTML/CSS/JavaScript, Node built-in test runner, Rust unit tests.

---

### Task 1: Model editor path overrides and detection rules

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`

**Step 1: Write the failing Rust tests**

Add tests for:
- old `AppSettings` still deserialize when `editorPaths` is missing
- supported editor list includes `vscodium`
- custom editor path overrides the built-in default
- installed detection returns true when a custom path exists or known install/config markers exist

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: FAIL because the new helpers/fields do not exist yet.

**Step 3: Write minimal implementation**

Implement:
- `editor_paths` field in `AppSettings`
- richer `EditorDef` metadata with default settings path candidates and install markers
- helpers for resolved editor settings path, detection, and settings row data
- update all editor-related commands to use the shared helpers

**Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

### Task 2: Expose structured editor path data to the frontend

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `public/app.js`
- Modify: `public/index.html`
- Modify: `public/style.css`

**Step 1: Write the failing frontend tests**

Extend frontend helper tests for:
- deriving row mode from detected/manual/default path metadata
- validating empty path input before save

**Step 2: Run test to verify it fails**

Run: `node --test public/app.test.js`
Expected: FAIL because the new helper behavior does not exist yet.

**Step 3: Write minimal implementation**

Implement:
- settings rows for all supported editors, not just detected ones
- path input field per editor
- actions: browse file, save path, reset to default, open containing folder
- refresh detection/status after saving an override

**Step 4: Run test to verify it passes**

Run: `node --test public/app.test.js`
Expected: PASS.

### Task 3: Verify end-to-end editor behavior

**Files:**
- Review: `src-tauri/src/lib.rs`
- Review: `public/app.js`

**Step 1: Verify supported editor behavior**

Confirm:
- built-in support: `VS Code`, `VS Code Insiders`, `Cursor`, `Windsurf`, `Trae`, `VSCodium`
- settings page always shows these editors
- custom path is used everywhere once saved

**Step 2: Verify UI refresh**

After saving or resetting a path:
- settings row updates
- editor detection updates
- status grid uses the resolved path

**Step 3: Run verification**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `node --test public/app.test.js`
- `node --check public/app.js`

If runtime verification is possible, also run:
- `dev.bat`

