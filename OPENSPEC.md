# OpenSpec: spelunktui

## Project Overview
`spelunktui` is a terminal user interface for the Splunk Platform. It allows users to perform searches, view job status, and manage Splunk jobs directly from the terminal.

## Key Features
- **SPL Search Editor:**
  - Multiline editing support.
  - Auto-resizing window.
  - External editor support (`Ctrl + x` in editing mode).
  - Saved searches management (`Ctrl + s` load, `Ctrl + w` save).
  - Vim Mode (`Ctrl + v` to toggle): Supports `Normal` (h/j/k/l navigation) and `Insert` modes.
  - Standard Mode: Supports `Shift + Enter` for newlines.
- **Job Management:**
  - Create search jobs.
  - View job status and results.
  - Kill jobs (`Ctrl + k`).
  - Open job URL in browser (`Shift + e`).
- **Theming:**
  - Three themes: Default, ColorPop, Splunk (brand colors).
  - Toggle with `t`.

## Architecture
- **Language:** Rust
- **UI Library:** `ratatui` (with `crossterm` backend)
- **Async Runtime:** `tokio`
- **HTTP Client:** `reqwest`
- **Config:** `config.toml` or environment variables.

## Recent Changes (Feature: Search Editor Improvements)
1.  **Saved Searches:**
    - Implemented `SavedSearchManager` in `src/utils/saved_searches.rs`.
    - Stores searches as `.spl` files in `~/.config/spelunktui/saved_searches` (or local fallback).
    - Added UI for loading (`Ctrl+s`) and saving (`Ctrl+w`).
    - Added overwrite confirmation.
2.  **Vim Mode:**
    - Added `EditorMode` enum (`Standard`, `Vim(Normal/Insert)`).
    - Implemented basic Vim navigation (`h`, `j`, `k`, `l`) and mode switching (`i`, `Esc`).
    - Added visual cursor support using `SetCursorStyle`.
3.  **Editor Enhancements:**
    - Added manual cursor management (`move_cursor_*`, `insert_char`, `delete_char`) to support random access editing.
    - Implemented `Shift + Enter` for newlines in Standard mode.
    - Removed `x` keybinding for "Kill Job" (use `Ctrl + k` instead).
4.  **Open URL:**
    - Added ability to open search job URL in browser with `Shift + e`.

## Future Work
- Implement Vim Visual mode.
- Add syntax highlighting for SPL.
