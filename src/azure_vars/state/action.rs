#[derive(Debug, Clone)]
pub enum Action {
    RefreshVarGroups,

    // Search
    EnterSearchMode,
    ExitSearchMode,
    SearchInsertChar { ch: char },
    SearchBackspace,
    SubmitSearch,

    // View Toggle
    EnterViewVarGroup { index: usize },
    ExitViewVarGroup,

    // Actions
    ToggleTheme,
    CopySelectedVar,
    ExportCurrentGroup,

    // Navigation
    MoveSelectionUp,
    MoveSelectionDown,
    MoveSelectionTop,
    MoveSelectionBottom,
    MoveSelectionPageUp,
    MoveSelectionPageDown,
}
