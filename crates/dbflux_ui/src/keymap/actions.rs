use gpui::{KeyBinding, actions};

actions!(
    dbflux,
    [
        ToggleCommandPalette,
        NewQueryTab,
        CloseCurrentTab,
        NextTab,
        PrevTab,
        SwitchToTab1,
        SwitchToTab2,
        SwitchToTab3,
        SwitchToTab4,
        SwitchToTab5,
        SwitchToTab6,
        SwitchToTab7,
        SwitchToTab8,
        SwitchToTab9,
        FocusSidebar,
        FocusEditor,
        FocusResults,
        FocusBackgroundTasks,
        CycleFocusForward,
        CycleFocusBackward,
        RunQuery,
        RunQueryInNewTab,
        ExportResults,
        OpenConnectionManager,
        Disconnect,
        RefreshSchema,
        ToggleEditor,
        ToggleResults,
        ToggleTasks,
        ToggleSidebar,
        // List navigation
        SelectNext,
        SelectPrev,
        SelectFirst,
        SelectLast,
        Execute,
        ExpandCollapse,
        // Column navigation (Results)
        ColumnLeft,
        ColumnRight,
        // Directional panel navigation
        FocusLeft,
        FocusRight,
        FocusUp,
        FocusDown,
        // Saved queries / history actions
        Delete,
        ToggleFavorite,
        Rename,
        FocusSearch,
        SaveQuery,
        // Settings
        OpenSettings,
        // Item menu
        OpenItemMenu,
        // Results toolbar
        FocusToolbar,
        TogglePanel,
        // File operations
        OpenScriptFile,
        SaveFileAs,
    ]
);

/// Keybindings that shadow `gpui-component` input defaults inside the "Input"
/// context so that Ctrl+Enter / Ctrl+Shift+Enter run the active query instead of
/// inserting a newline.
///
/// `gpui-component` binds `secondary-enter` (== Ctrl+Enter on Linux/Windows,
/// Cmd+Enter on macOS) to its internal `Enter` action, which in multi-line mode
/// inserts a newline. Registering these bindings after `gpui_component::init`
/// makes them take precedence at the same context depth.
pub fn input_context_keybindings() -> Vec<KeyBinding> {
    let ctx = Some("Input");
    vec![
        KeyBinding::new("ctrl-enter", RunQuery, ctx),
        KeyBinding::new("ctrl-shift-enter", RunQueryInNewTab, ctx),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_module_compiles() {
        let _ = ToggleCommandPalette;
    }
}
