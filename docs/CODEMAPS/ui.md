# UI Module Codemap

**Last Updated:** 2025-01-26
**Location:** `src/ui/`

## Overview

The UI module provides terminal rendering using ratatui, implementing a tree-based workspace list with overlay dialogs.

## Structure

```
src/ui/
├── mod.rs              # Main render(), centered_rect() utility
├── workspace_list.rs   # Main workspace tree view
├── detail_view.rs      # Workspace detail overlay
├── help_view.rs        # Help overlay (keyboard shortcuts)
├── status_bar.rs       # Bottom status bar
├── input_dialog.rs     # Text input dialog
└── selection_dialog.rs # Selection list dialog
```

## Screen Layout

```
┌─────────────────────────────────────────────────────────────────┐
│                      Workspace List                              │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ > repo-name                                          [3]   │ │
│  │   ├─ main         ~/work/repo           [Working]         │ │
│  │   ├─ feature-a    ~/work/repo=feature-a [Idle]      [*]   │ │
│  │   └─ feature-b    (local branch)                          │ │
│  │ > another-repo                                       [1]   │ │
│  │   └─ main         ~/work/another                          │ │
│  └────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│ 5 workspaces | 2 active | View: Worktrees | Status message...   │
└─────────────────────────────────────────────────────────────────┘
```

## Key Functions

### mod.rs

```rust
/// Main render entry point
pub fn render(frame: &mut Frame, state: &AppState) {
    // 1. Render workspace_list (main area)
    // 2. Render status_bar (bottom)
    // 3. Render overlay based on ViewMode:
    //    - Help: help_view
    //    - Detail: detail_view
    //    - Input: input_dialog
    //    - Selection: selection_dialog
}

/// Utility for centered popup positioning
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect
```

### workspace_list.rs

Renders the main tree view with:
- Repository groups (expandable/collapsible)
- Worktree entries with status indicators
- Branch entries (local/remote)
- Visual indicators for open Zellij tabs

**Status Colors:**
| Status | Color |
|--------|-------|
| Working | Yellow |
| Idle | Gray |
| NeedsInput | Cyan |
| Error | Red |
| Disconnected | DarkGray |

**Tree Indicators:**
- `>` / `v` - Collapsed/expanded group
- `[n]` - Worktree count in group
- `[*]` - Tab is open in Zellij
- `(local)` / `(remote)` - Branch type

### status_bar.rs

Bottom bar showing:
- Total workspace count
- Active (connected) count
- Working count
- Current display mode
- Status message (if any)

### help_view.rs

Centered overlay showing keyboard shortcuts organized by category:
- Navigation
- Worktree Management
- Zellij Actions
- Other

### detail_view.rs

Centered overlay showing selected workspace details:
- Project path
- Repository name
- Branch name
- Status
- Session ID (if connected)
- Last updated time

### input_dialog.rs

```rust
pub struct InputDialog {
    pub kind: InputDialogKind,
    pub input: String,
    pub cursor_position: usize,
    pub error: Option<String>,
}

pub enum InputDialogKind {
    CreateWorktree,
    DeleteWorktree { path: String },
}
```

Used for:
- Creating new worktrees (enter branch name)
- Confirming worktree deletion (y/n)

### selection_dialog.rs

```rust
pub struct SelectionDialog {
    pub kind: SelectionDialogKind,
    pub items: Vec<String>,
    pub selected_index: usize,
    pub context: Option<SelectionContext>,
}

pub enum SelectionDialogKind {
    SelectSession,  // Choose Zellij session
    SelectLayout,   // Choose layout file
}

pub struct SelectionContext {
    pub workspace_path: String,
    pub repo_name: String,
    pub branch_name: String,
}
```

Used for:
- Selecting target Zellij session
- Selecting layout for new tab

## Rendering Flow

```
render(frame, state)
    │
    ├── Layout::vertical([Min(5), Length(1)])
    │       │
    │       ├── workspace_list::render(chunks[0])
    │       │       │
    │       │       └── Table with TreeItems
    │       │
    │       └── status_bar::render(chunks[1])
    │               │
    │               └── Paragraph with stats
    │
    └── match state.view_mode
            │
            ├── Help -> help_view::render(area)
            ├── Detail -> detail_view::render(area, workspace)
            ├── Input -> input_dialog::render(area, dialog)
            └── Selection -> selection_dialog::render(area, dialog)
```

## Exports

```rust
// From mod.rs
pub use input_dialog::InputDialog;
pub use selection_dialog::{SelectionDialog, SelectionDialogKind, SelectionContext};
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect;
pub fn render(frame: &mut Frame, state: &AppState);
```

## Related Modules

- [app](app.md) - Provides AppState for rendering
- Uses ratatui widgets: Table, Paragraph, Block, Clear
