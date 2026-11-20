use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
};

use crate::azure_vars::{
    state::state::{SearchTarget, State, View},
    tui::widgets::{BreadCrumb, HelpBar, SearchBar, StatusBar, VarGroupList, VarList},
};

pub fn draw_ui(f: &mut Frame, state: &State) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(if state.ui.search.is_active() { 3 } else { 0 }),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(if state.ui.status.is_some() { 2 } else { 0 }),
            Constraint::Length(2),
        ])
        .split(size);

    if state.ui.is_fetching {
        f.render_widget(
            Paragraph::new("Loading variable groups...")
                .block(Block::default().borders(Borders::ALL).title("Please wait")),
            chunks[2],
        );
        return;
    }

    if state.ui.search.is_active() {
        let target = state
            .ui
            .search
            .active_target()
            .unwrap_or(if state.is_viewing_vars() {
                SearchTarget::Vars
            } else {
                SearchTarget::Groups
            });
        let query = state.search_query_for(target);
        f.render_widget(SearchBar::new(query, target), chunks[0]);
    }

    f.render_widget(
        BreadCrumb::new(
            state.organization().to_string(),
            state.project().to_string(),
            state
                .current_group()
                .map(|g| g.name.clone())
                .unwrap_or_default(),
            state.is_viewing_vars(),
            state.theme,
        ),
        chunks[1],
    );

    if !state.is_viewing_vars() {
        let group_indices = state.filtered_group_indices();
        let groups = group_indices
            .iter()
            .filter_map(|idx| state.groups().get(*idx).cloned())
            .collect();
        let selected_position = match &state.ui.view {
            View::Groups { selected_idx } => selected_idx
                .and_then(|selected| group_indices.iter().position(|idx| *idx == selected)),
            View::Vars { .. } => None,
        };
        f.render_widget(
            VarGroupList::new(
                groups,
                state.theme,
                state.ui.search.groups_query().to_string(),
                selected_position,
            ),
            chunks[2],
        );
    } else if let Some(selected_group) = state.current_group() {
        let var_indices = state.filtered_var_indices();
        let vars = var_indices
            .iter()
            .filter_map(|idx| selected_group.variables.get(*idx).cloned())
            .collect();
        let selected_position = match &state.ui.view {
            View::Vars {
                selected_var_idx, ..
            } => selected_var_idx
                .and_then(|selected| var_indices.iter().position(|idx| *idx == selected)),
            View::Groups { .. } => None,
        };
        f.render_widget(
            VarList::new(
                vars,
                selected_group.name.clone(),
                state.active_vars_query().map(str::to_string),
                state.theme,
                selected_position,
            ),
            chunks[2],
        );
    }

    if let Some(status) = state.ui.status.clone() {
        f.render_widget(StatusBar::new(state.theme, status), chunks[3]);
    }

    f.render_widget(
        HelpBar::new(
            state.theme,
            state.is_viewing_vars(),
            state.ui.search.clone(),
        ),
        chunks[4],
    );
}
