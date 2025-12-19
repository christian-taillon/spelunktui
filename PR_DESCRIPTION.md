# Add Keyboard Shortcuts Helper

This PR adds a new "Help" modal to the application, accessible via `Ctrl + /`.

## Features
- **Keyboard Shortcuts Helper**: Pressing `Ctrl + /` opens a modal listing all available keyboard shortcuts, organized by category (General, Search Input, Results & Navigation, Pane Navigation).
- **Comprehensive List**: Includes hidden or less obvious shortcuts like `Ctrl + v` (View Mode), `Ctrl + x` (Editor), and Vim navigation keys.
- **Easy Dismissal**: The helper can be closed by pressing `Esc`, `Enter`, or `q`.

## Implementation Details
- Added `InputMode::Help` to the application state.
- Implemented `Ctrl + /` key binding in the main event loop.
- Added a new rendering block in `ui()` to display the shortcuts in a centered table.