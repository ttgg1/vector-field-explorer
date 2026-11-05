# Preset Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task‑by‑task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist camera and field presets between sessions so users can load predefined views.

**Architecture:** Add a simple JSON file format under `data/presets.json` containing an array of preset objects with name, camera position/scale, and any other relevant state. Provide API in `src/export.rs` to read/write presets, and UI controls in `src/ui.rs` to list, create, edit, delete, and load presets.

**Tech Stack:** Rust standard library, serde_json, egui for UI.

**Spec:** This plan implements the feature described in the roadmap: preset persistence (Task 9).

## Global Constraints
- Use existing `Camera` struct fields (`position`, `scale`).
- Persist to a file relative to the executable’s data directory.
- Ensure backward compatibility with existing export functionality.
- Tests must run on CI without external dependencies beyond crates in Cargo.toml.

## Review Focus
1. Loading presets restores exact camera state.
2. Deleting a preset removes it from the list and disk.
3. Creating a preset records current camera state.
4. Presets survive application restarts.
5. UI updates reflect changes immediately.
---

### Task 1: Define Preset Struct and JSON Serialization

**Files:**
- Modify: `src/export.rs`: add `Preset` struct, serde derives, read/write functions.
- Test: `tests/preset_test.rs`

**Interfaces:**
- Consumes: None (standalone).
- Produces: Functions `export::save_preset(name:&str)`, `export::load_presets() -> Vec<Preset>`.

- [ ] **Step 1: Write failing test**
```rust
#[test]
fn test_save_and_load_preset() {
    let tmp_dir = tempfile::tempdir().unwrap();
    // Set camera state
    let cam = Camera { position: [1.0,2.0], scale: 3.0 };
    export::save_preset(tmp_dir.path(), "test", &cam).expect("save" );
    let presets = export::load_presets(tmp_dir.path()).expect("load");
    assert_eq!(presets.len(),1);
    assert_eq!(presets[0].name, "test");
    assert_eq!(presets[0].position, [1.0,2.0]);
    assert_eq!(presets[0].scale, 3.0);
}
```
- [ ] **Step 2: Run test to verify it fails**
- [ ] **Step 3: Implement `Preset` struct and serialization in `export.rs`.**
- [ ] **Step 4: Run test to verify it passes**
- [ ] **Step 5: Commit**

### Task 2: UI Integration for Preset Management

**Files:**
- Modify: `src/ui.rs`: add panel with list, buttons for create/delete/load.
- Test: `tests/ui_preset_test.rs` (integration test using egui backend).

**Interfaces:**
- Consumes: `export::save_preset`, `export::load_presets`.
- Produces: UI actions that modify camera via callbacks.

- [ ] **Step 1: Write failing UI test**
```rust
#[test]
fn ui_can_create_and_load_preset() {
    // Setup egui context, simulate button presses, assert camera state changes.
}
```
- [ ] **Step 2: Run test to verify it fails**
- [ ] **Step 3: Implement UI callbacks and state updates.**
- [ ] **Step 4: Run test to verify it passes**
- [ ] **Step 5: Commit**

### Task 3: Persist Preset File Path Configuration

**Files:**
- Modify: `src/app.rs`: determine data directory, pass path to export functions.
- Test: `tests/config_test.rs`.

- [ ] **Steps similar to above for configuration persistence.**
