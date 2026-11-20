use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, StatefulWidget, Widget};

use crate::azure_vars::state::state::{
    SearchState, SearchTarget, StatusKind, StatusMessage, Theme, VarEntry, VarGroup,
};

pub struct SearchBar {
    query: String,
    target: SearchTarget,
}

impl SearchBar {
    pub fn new(query: String, target: SearchTarget) -> Self {
        Self { query, target }
    }
}

impl Widget for SearchBar {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let title = match self.target {
            SearchTarget::Groups => "Search Groups",
            SearchTarget::Vars => "Search Variables",
        };
        let search = Paragraph::new(format!("/{}", self.query))
            .block(Block::default().borders(Borders::ALL).title(title));
        search.render(area, buf);
    }
}

pub struct BreadCrumb {
    organization: String,
    project: String,
    group_name: String,
    viewing_vars: bool,
    theme: Theme,
}

impl BreadCrumb {
    pub fn new(
        organization: String,
        project: String,
        group_name: String,
        viewing_vars: bool,
        theme: Theme,
    ) -> Self {
        Self {
            theme,
            organization,
            project,
            group_name,
            viewing_vars,
        }
    }
}

impl Widget for BreadCrumb {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let muted = match self.theme {
            Theme::Dark => Color::DarkGray,
            Theme::Light => Color::Gray,
        };
        let breadcrumb = format!(
            "{} > {}{}",
            self.organization,
            self.project,
            if self.viewing_vars {
                format!(" > {}", self.group_name)
            } else {
                "".into()
            }
        );
        let header = Paragraph::new(breadcrumb)
            .style(Style::default().fg(muted))
            .block(Block::default());

        header.render(area, buf);
    }
}

pub struct VarGroupList {
    groups: Vec<VarGroup>,
    theme: Theme,
    search_query: String,
    selected_position: Option<usize>,
}

impl VarGroupList {
    pub fn new(
        groups: Vec<VarGroup>,
        theme: Theme,
        search_query: String,
        selected_position: Option<usize>,
    ) -> Self {
        Self {
            groups,
            theme,
            search_query,
            selected_position,
        }
    }
}

impl Widget for VarGroupList {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Variable Groups");
        if self.groups.is_empty() && !self.search_query.is_empty() {
            Paragraph::new("No matches").block(block).render(area, buf);
            return;
        }

        let accent = match self.theme {
            Theme::Dark => Color::Yellow,
            Theme::Light => Color::Blue,
        };
        let items: Vec<ListItem> = self
            .groups
            .iter()
            .map(|g| {
                ListItem::new(Line::from(vec![
                    Span::styled(&g.name, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!("  ({} vars)", g.variables.len())),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ");

        let mut state =
            ratatui::widgets::ListState::default().with_selected(self.selected_position);
        StatefulWidget::render(list, area, buf, &mut state);
    }
}

pub struct VarList {
    vars: Vec<VarEntry>,
    group_name: String,
    search_query: Option<String>,
    theme: Theme,
    selected_position: Option<usize>,
}

impl VarList {
    pub fn new(
        vars: Vec<VarEntry>,
        group_name: String,
        search_query: Option<String>,
        theme: Theme,
        selected_position: Option<usize>,
    ) -> Self {
        Self {
            vars,
            group_name,
            search_query,
            theme,
            selected_position,
        }
    }
}

impl Widget for VarList {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let title = format!(
            "{} ({} vars{})",
            self.group_name,
            self.vars.len(),
            self.search_query
                .as_deref()
                .map(|query| format!(", filter: '{query}'"))
                .unwrap_or_default()
        );
        let block = Block::default().borders(Borders::ALL).title(title);
        if self.vars.is_empty() && self.search_query.is_some() {
            Paragraph::new("No matches").block(block).render(area, buf);
            return;
        }

        let accent = match self.theme {
            Theme::Dark => Color::Yellow,
            Theme::Light => Color::Blue,
        };
        let muted = match self.theme {
            Theme::Dark => Color::DarkGray,
            Theme::Light => Color::Gray,
        };

        let items: Vec<ListItem> = self
            .vars
            .iter()
            .map(|v| {
                let val_color = if v.is_secret {
                    Color::Red
                } else if v.value == "<no value>" {
                    muted
                } else {
                    Color::Cyan
                };

                ListItem::new(Line::from(vec![
                    Span::styled(&v.name, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(": "),
                    Span::styled(&v.value, Style::default().fg(val_color)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ");

        let mut state =
            ratatui::widgets::ListState::default().with_selected(self.selected_position);
        StatefulWidget::render(list, area, buf, &mut state);
    }
}

pub struct HelpBar {
    theme: Theme,
    viewing_vars: bool,
    search: SearchState,
}

impl HelpBar {
    pub fn new(theme: Theme, viewing_vars: bool, search: SearchState) -> Self {
        Self {
            theme,
            viewing_vars,
            search,
        }
    }
}

impl Widget for HelpBar {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let text: String = if let Some(target) = self.search.active_target() {
            match target {
                SearchTarget::Groups => {
                    "Type to search groups | Enter=apply | Esc=cancel | Backspace=delete".into()
                }
                SearchTarget::Vars => {
                    "Type to search variables | Enter=apply | Esc=cancel | Backspace=delete".into()
                }
            }
        } else if self.viewing_vars {
            "← back | Navigation: ↑/↓, Prev/Next Page, Home, End | / search | R refresh | C copy | E export | T theme | q quit"
                .into()
        } else {
            "← back | Navigation: ↑/↓, Prev/Next Page, Home, End | / search | R refresh | T theme | q quit"
                .into()
        };

        Paragraph::new(text)
            .style(Style::default().fg(match self.theme {
                Theme::Dark => Color::DarkGray,
                Theme::Light => Color::Gray,
            }))
            .block(Block::default().borders(Borders::TOP))
            .render(area, buf);
    }
}

pub struct StatusBar {
    theme: Theme,
    message: StatusMessage,
}

impl StatusBar {
    pub fn new(theme: Theme, message: StatusMessage) -> Self {
        Self { theme, message }
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let color = match self.message.kind {
            StatusKind::Info => match self.theme {
                Theme::Dark => Color::Green,
                Theme::Light => Color::Blue,
            },
            StatusKind::Error => Color::Red,
        };

        Paragraph::new(self.message.text)
            .style(Style::default().fg(color))
            .block(Block::default().borders(Borders::TOP))
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(widget: impl Widget) -> String {
        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| frame.render_widget(widget, frame.area()))
            .unwrap();

        (0..5)
            .map(|y| {
                (0..40)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn group_list_renders_selected_position_and_empty_search_message() {
        let groups = vec![
            VarGroup {
                name: "Alpha".to_string(),
                variables: Vec::new(),
            },
            VarGroup {
                name: "Beta".to_string(),
                variables: Vec::new(),
            },
        ];
        let output = render(VarGroupList::new(
            groups,
            Theme::Dark,
            String::new(),
            Some(1),
        ));
        assert!(output.contains(">> Beta"));

        let output = render(VarGroupList::new(
            Vec::new(),
            Theme::Dark,
            "missing".into(),
            None,
        ));
        assert!(output.contains("No matches"));
    }

    #[test]
    fn variable_list_renders_selected_position_and_empty_search_message() {
        let vars = vec![
            VarEntry {
                name: "alpha".to_string(),
                value: "one".to_string(),
                is_secret: false,
            },
            VarEntry {
                name: "beta".to_string(),
                value: "two".to_string(),
                is_secret: false,
            },
        ];
        let output = render(VarList::new(
            vars,
            "Group".to_string(),
            None,
            Theme::Dark,
            Some(1),
        ));
        assert!(output.contains(">> beta"));

        let output = render(VarList::new(
            Vec::new(),
            "Group".to_string(),
            Some("missing".to_string()),
            Theme::Dark,
            None,
        ));
        assert!(output.contains("No matches"));
    }
}
