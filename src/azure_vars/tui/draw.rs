use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
};

use crate::azure_vars::{
    state::state::{SearchTarget, State},
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
        if let Some(selected_group) = state.current_group() {
            f.render_widget(
                VarGroupList::new(
                    state.filtered_groups().into_iter().cloned().collect(),
                    state.theme,
                    selected_group.clone(),
                ),
                chunks[2],
            );
        }
    } else if let (Some(selected_group), Some(selected_var)) =
        (state.current_group(), state.current_var())
    {
        let vars = state.filtered_vars().into_iter().cloned().collect();
        f.render_widget(
            VarList::new(
                vars,
                selected_group.name.clone(),
                state.active_vars_query().map(str::to_string),
                state.theme,
                selected_var.clone(),
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
