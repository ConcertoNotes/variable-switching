# Usage Help And Release Updater Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a first-run usage guide modal, a persistent help entry on the main page, a GitHub repository button, and a GitHub Releases based update flow that can download and open the current platform installer.

**Architecture:** Keep UI work in `public/index.html`, `public/style.css`, and `public/app.js`. Extend `AppSettings` in `src-tauri/src/lib.rs` to persist the "never remind again" choice, and add new Tauri commands for release checking, installer downloading, and opening external URLs/installers. Use GitHub Releases as the single source of truth for the latest version.

**Tech Stack:** Tauri v2, Rust, native HTML/CSS/JavaScript, reqwest blocking client, Node built-in test runner, Rust unit tests.

---

### Task 1: Persist guide preference and add release/update backend

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`

**Step 1: Write the failing Rust tests**

Add tests for:
- default `AppSettings` keeps the existing defaults and sets `neverShowUsageGuide` to `false`
- version parsing/comparison treats `v1.2.0` as newer than `1.1.9`
- installer asset selection prefers Windows `.msi` or `.exe` and macOS `.dmg`

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: FAIL because the new fields/helpers do not exist yet.

**Step 3: Write minimal implementation**

Implement:
- `AppSettings.never_show_usage_guide`
- release metadata structs/helpers
- command to check latest GitHub release against current app version
- command to download the matching installer to a temp/update directory
- command to open external URLs or downloaded installers

**Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS for the new helper tests and existing tests remain green.

### Task 2: Add usage guide UI and toolbar actions

**Files:**
- Modify: `public/index.html`
- Modify: `public/style.css`
- Modify: `public/app.js`

**Step 1: Write the failing frontend tests**

Create a small Node test file for pure helper behavior such as:
- deciding whether the usage guide should auto-open
- formatting update state labels
- selecting the correct button state for "check update" vs "update now"

**Step 2: Run test to verify it fails**

Run: `node --test public/app.test.js`
Expected: FAIL because the helper functions do not exist yet.

**Step 3: Write minimal implementation**

Implement:
- usage guide modal shown after splash completes
- buttons: close, never remind again
- main page buttons: usage guide, check update, GitHub repo
- update state handling in JS
- localized text for Chinese and English
- modal reuse so the same guide is available from the main page

**Step 4: Run test to verify it passes**

Run: `node --test public/app.test.js`
Expected: PASS.

### Task 3: Integrate, verify, and keep existing behavior intact

**Files:**
- Modify if needed: `src-tauri/capabilities/default.json`
- Review: `public/app.js`
- Review: `src-tauri/src/lib.rs`

**Step 1: Wire the init flow**

After splash fade-out:
- load settings
- decide whether to show the usage guide automatically
- keep manual guide access always available

**Step 2: Wire the update flow**

On button click:
- check GitHub latest release
- if newer, switch the same button to update mode
- on second click, download installer, then open it
- show clear success/error toasts and disable the button while work is in progress

**Step 3: Verify behavior manually**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `node --test public/app.test.js`

If runtime verification is possible, also run:
- `npm run tauri -- dev`

Expected:
- splash completes, then guide appears unless disabled
- close keeps reminder for next launch
- never remind again persists across launches
- help button reopens the same guide
- check update compares against GitHub Releases
- update downloads and opens the installer for the current platform
- GitHub button opens the repository page

