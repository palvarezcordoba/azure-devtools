use std::{collections::HashMap, fmt::Debug, sync::RwLock};

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VarEntry {
    pub name: String,
    pub value: String,
    pub is_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VarGroup {
    pub name: String,
    pub variables: Vec<VarEntry>,
}

#[derive(Clone, Copy, Default, Debug)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
}

impl StatusMessage {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            text: message.into(),
            kind: StatusKind::Info,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            text: message.into(),
            kind: StatusKind::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppData {
    organization: String,
    project: String,
    pub groups: Vec<VarGroup>,
}

impl AppData {
    pub fn new(organization: String, project: String) -> Self {
        Self {
            organization,
            project,
            groups: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub view: View,
    pub is_fetching: bool,
    pub search: SearchState,
    pub status: Option<StatusMessage>,
}

#[derive(Debug, Clone)]
pub enum View {
    Groups {
        selected_idx: Option<usize>,
    },
    Vars {
        group_idx: usize,
        selected_var_idx: Option<usize>,
    },
}

impl View {
    pub fn is_vars(&self) -> bool {
        matches!(self, View::Vars { .. })
    }
}

impl Default for View {
    fn default() -> Self {
        View::Groups { selected_idx: None }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub status: SearchStatus,
    pub groups_query: String,
    vars_queries: HashMap<usize, String>,
}

impl SearchState {
    pub fn is_active(&self) -> bool {
        matches!(self.status, SearchStatus::Active(_))
    }

    pub fn active_target(&self) -> Option<SearchTarget> {
        match self.status {
            SearchStatus::Active(target) => Some(target),
            SearchStatus::Inactive => None,
        }
    }

    pub(super) fn activate(&mut self, target: SearchTarget) {
        self.status = SearchStatus::Active(target);
    }

    pub(super) fn deactivate(&mut self) {
        self.status = SearchStatus::Inactive;
    }

    pub fn groups_query(&self) -> &str {
        &self.groups_query
    }

    pub(super) fn groups_query_mut(&mut self) -> &mut String {
        &mut self.groups_query
    }

    pub fn vars_query(&self, group_idx: usize) -> &str {
        self.vars_queries
            .get(&group_idx)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub(super) fn vars_query_mut(&mut self, group_idx: usize) -> &mut String {
        self.vars_queries.entry(group_idx).or_default()
    }

    pub(super) fn clear_vars_query(&mut self, group_idx: usize) {
        self.vars_queries.remove(&group_idx);
    }
}

impl UiState {
    pub fn set_status(&mut self, status: StatusMessage) {
        self.status = Some(status);
    }

    pub fn clear_status(&mut self) {
        self.status = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchStatus {
    #[default]
    Inactive,
    Active(SearchTarget),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTarget {
    Groups,
    Vars,
}

#[derive(Debug, Clone, Default)]
struct FilterCache {
    groups: Option<CacheEntry>,
    vars: Option<VarCacheEntry>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    query: String,
    indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct VarCacheEntry {
    group_idx: usize,
    query: String,
    indices: Vec<usize>,
}

impl FilterCache {
    fn get_groups(&self, query: &str) -> Option<Vec<usize>> {
        self.groups.as_ref().and_then(|entry| {
            if entry.query == query {
                Some(entry.indices.clone())
            } else {
                None
            }
        })
    }

    fn store_groups(&mut self, query: &str, indices: Vec<usize>) {
        self.groups = Some(CacheEntry {
            query: query.to_string(),
            indices,
        });
    }

    fn get_vars(&self, group_idx: usize, query: &str) -> Option<Vec<usize>> {
        self.vars.as_ref().and_then(|entry| {
            if entry.group_idx == group_idx && entry.query == query {
                Some(entry.indices.clone())
            } else {
                None
            }
        })
    }

    fn store_vars(&mut self, group_idx: usize, query: &str, indices: Vec<usize>) {
        self.vars = Some(VarCacheEntry {
            group_idx,
            query: query.to_string(),
            indices,
        });
    }

    fn invalidate_groups(&mut self) {
        self.groups = None;
    }

    fn invalidate_vars(&mut self) {
        self.vars = None;
    }

    fn invalidate_all(&mut self) {
        self.invalidate_groups();
        self.invalidate_vars();
    }
}

pub struct State {
    pub data: AppData,
    pub ui: UiState,
    pub theme: Theme,
    pub matcher: SkimMatcherV2,
    filter_cache: RwLock<FilterCache>,
}

impl State {
    pub fn new(organization: String, project: String) -> Self {
        Self {
            data: AppData::new(organization, project),
            ui: UiState::default(),
            theme: Theme::default(),
            matcher: SkimMatcherV2::default(),
            filter_cache: RwLock::new(FilterCache::default()),
        }
    }

    pub fn organization(&self) -> &str {
        &self.data.organization
    }

    pub fn project(&self) -> &str {
        &self.data.project
    }

    pub fn groups(&self) -> &[VarGroup] {
        &self.data.groups
    }

    pub(super) fn set_groups(&mut self, groups: Vec<VarGroup>) {
        let prev_group = self.current_group().map(|g| g.name.clone());
        let prev_var = self.current_var().map(|v| v.name.clone());
        self.data.groups = groups;
        self.filter_cache.write().unwrap().invalidate_all();
        self.sync_selection_with_previous(prev_group, prev_var);
    }

    pub(super) fn invalidate_group_cache(&mut self) {
        self.filter_cache.write().unwrap().invalidate_groups();
    }

    pub(super) fn invalidate_var_cache(&mut self) {
        self.filter_cache.write().unwrap().invalidate_vars();
    }

    pub fn is_viewing_vars(&self) -> bool {
        self.ui.view.is_vars()
    }

    pub fn current_group_idx(&self) -> Option<usize> {
        match self.ui.view {
            View::Groups { selected_idx } => selected_idx,
            View::Vars { group_idx, .. } => Some(group_idx),
        }
    }

    pub fn current_group(&self) -> Option<&VarGroup> {
        self.current_group_idx()
            .and_then(|idx| self.data.groups.get(idx))
    }

    pub fn vars_group_idx(&self) -> Option<usize> {
        if let View::Vars { group_idx, .. } = self.ui.view {
            Some(group_idx)
        } else {
            None
        }
    }

    fn current_var_idx(&self) -> Option<usize> {
        match self.ui.view {
            View::Vars {
                selected_var_idx, ..
            } => selected_var_idx,
            _ => None,
        }
    }

    pub fn current_var(&self) -> Option<&VarEntry> {
        let group_idx = self.vars_group_idx()?;
        let var_idx = self.current_var_idx()?;
        self.data
            .groups
            .get(group_idx)
            .and_then(|group| group.variables.get(var_idx))
    }

    pub fn filtered_group_indices(&self) -> Vec<usize> {
        let query = self.ui.search.groups_query();
        if let Some(indices) = self.filter_cache.read().unwrap().get_groups(query) {
            return indices;
        }

        let mut ranked = self
            .data
            .groups
            .iter()
            .enumerate()
            .map(|(idx, group)| (self.matcher.fuzzy_match(&group.name, query), idx))
            .collect::<Vec<(Option<i64>, usize)>>();

        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        let indices = ranked.into_iter().map(|(_, idx)| idx).collect::<Vec<_>>();
        self.filter_cache
            .write()
            .unwrap()
            .store_groups(query, indices.clone());
        indices
    }

    pub fn filtered_var_indices_for(&self, group_idx: usize) -> Vec<usize> {
        let Some(group) = self.data.groups.get(group_idx) else {
            return Vec::new();
        };

        let query = self.ui.search.vars_query(group_idx);
        if let Some(indices) = self.filter_cache.read().unwrap().get_vars(group_idx, query) {
            return indices;
        }

        let mut vars_ranked = group
            .variables
            .iter()
            .enumerate()
            .filter_map(|(idx, var)| {
                let score = self
                    .matcher
                    .fuzzy_match(&var.name, query)
                    .or_else(|| self.matcher.fuzzy_match(&var.value, query));
                score.map(|s| (s, idx))
            })
            .collect::<Vec<(i64, usize)>>();

        vars_ranked.sort_by(|a, b| b.0.cmp(&a.0));
        let indices = vars_ranked
            .into_iter()
            .map(|(_, idx)| idx)
            .collect::<Vec<_>>();
        self.filter_cache
            .write()
            .unwrap()
            .store_vars(group_idx, query, indices.clone());
        indices
    }

    pub fn filtered_groups(&self) -> Vec<&VarGroup> {
        self.filtered_group_indices()
            .into_iter()
            .filter_map(|idx| self.data.groups.get(idx))
            .collect()
    }

    pub fn filtered_var_indices(&self) -> Vec<usize> {
        let Some(group_idx) = self.vars_group_idx() else {
            return Vec::new();
        };
        self.filtered_var_indices_for(group_idx)
    }

    pub fn filtered_vars(&self) -> Vec<&VarEntry> {
        let Some(group_idx) = self.vars_group_idx() else {
            return Vec::new();
        };

        self.filtered_var_indices_for(group_idx)
            .into_iter()
            .filter_map(|idx| {
                self.data
                    .groups
                    .get(group_idx)
                    .and_then(|group| group.variables.get(idx))
            })
            .collect()
    }

    pub fn sync_selection(&mut self) {
        self.sync_selection_with_previous(None, None);
    }

    fn sync_selection_with_previous(
        &mut self,
        prev_group: Option<String>,
        prev_var: Option<String>,
    ) {
        if self.data.groups.is_empty() {
            self.ui.view = View::Groups { selected_idx: None };
            return;
        }

        let prev_group_name = prev_group.as_deref();
        let prev_var_name = prev_var.as_deref();

        match &mut self.ui.view {
            View::Groups { selected_idx } => {
                let fallback = if prev_group_name.is_some() {
                    0
                } else {
                    selected_idx.unwrap_or(0).min(self.data.groups.len() - 1)
                };
                let next = prev_group_name
                    .and_then(|name| self.data.groups.iter().position(|g| g.name == name))
                    .unwrap_or(fallback);
                *selected_idx = Some(next);
                self.filter_cache.write().unwrap().invalidate_groups();
            }
            View::Vars {
                group_idx,
                selected_var_idx,
            } => {
                let fallback_group = if prev_group_name.is_some() {
                    0
                } else {
                    (*group_idx).min(self.data.groups.len() - 1)
                };
                let next_group = prev_group_name
                    .and_then(|name| self.data.groups.iter().position(|g| g.name == name))
                    .unwrap_or(fallback_group);
                *group_idx = next_group;

                let vars = &self.data.groups[next_group].variables;
                if vars.is_empty() {
                    *selected_var_idx = None;
                    self.filter_cache.write().unwrap().invalidate_vars();
                    return;
                }

                let fallback_var = if prev_var_name.is_some() {
                    0
                } else {
                    selected_var_idx.unwrap_or(0).min(vars.len() - 1)
                };
                let next_var = prev_var_name
                    .and_then(|name| vars.iter().position(|v| v.name == name))
                    .unwrap_or(fallback_var);
                *selected_var_idx = Some(next_var);
                self.filter_cache.write().unwrap().invalidate_vars();
            }
        }
    }

    pub fn search_query_for(&self, target: SearchTarget) -> String {
        match target {
            SearchTarget::Groups => self.ui.search.groups_query().to_string(),
            SearchTarget::Vars => self
                .vars_group_idx()
                .map(|idx| self.ui.search.vars_query(idx).to_string())
                .unwrap_or_default(),
        }
    }

    pub fn active_vars_query(&self) -> Option<&str> {
        self.vars_group_idx().and_then(|idx| {
            let query = self.ui.search.vars_query(idx);
            if query.is_empty() { None } else { Some(query) }
        })
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("organization", &self.data.organization)
            .field("project", &self.data.project)
            .field("groups", &self.data.groups.len())
            .field("view", &self.ui.view)
            .field("is_fetching", &self.ui.is_fetching)
            .field("search", &self.ui.search)
            .field("theme", &self.theme)
            .finish()
    }
}

impl Clone for State {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            ui: self.ui.clone(),
            theme: self.theme,
            matcher: SkimMatcherV2::default(),
            filter_cache: RwLock::new(FilterCache::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str, value: &str) -> VarEntry {
        VarEntry {
            name: name.to_string(),
            value: value.to_string(),
            is_secret: false,
        }
    }

    fn group(name: &str, vars: Vec<VarEntry>) -> VarGroup {
        VarGroup {
            name: name.to_string(),
            variables: vars,
        }
    }

    #[test]
    fn set_groups_initializes_selection() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![
            group("Alpha", vec![var("a", "1")]),
            group("Beta", vec![var("b", "2")]),
        ]);

        assert_eq!(
            state.current_group().map(|g| g.name.as_str()),
            Some("Alpha")
        );
        match state.ui.view {
            View::Groups { selected_idx } => assert_eq!(selected_idx, Some(0)),
            _ => panic!("expected to remain in group view"),
        }
    }

    #[test]
    fn filtered_vars_respect_search_query() {
        let mut state = State::new("org".into(), "proj".into());
        let vars = vec![var("alpha", "1"), var("beta", "2"), var("gamma", "3")];
        state.set_groups(vec![group("vars", vars)]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(0),
        };

        assert_eq!(state.filtered_vars().len(), 3);

        state.ui.search.vars_query_mut(0).push_str("zzz");
        state.invalidate_var_cache();
        assert!(state.filtered_vars().is_empty());
    }

    #[test]
    fn active_vars_query_reports_only_non_empty_queries() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![group("vars", vec![var("alpha", "1")])]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(0),
        };

        assert!(state.active_vars_query().is_none());

        state.ui.search.vars_query_mut(0).push_str("alp");
        assert_eq!(state.active_vars_query(), Some("alp"));
    }

    #[test]
    fn search_query_for_returns_current_queries() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![group("vars", vec![var("alpha", "1")])]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(0),
        };
        state.ui.search.groups_query_mut().push_str("be");
        state.ui.search.vars_query_mut(0).push_str("alp");

        assert_eq!(
            state.search_query_for(SearchTarget::Groups),
            "be".to_string()
        );
        assert_eq!(
            state.search_query_for(SearchTarget::Vars),
            "alp".to_string()
        );
    }

    #[test]
    fn set_groups_preserves_selected_group_by_name() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![
            group("Alpha", vec![]),
            group("Beta", vec![]),
            group("Gamma", vec![]),
        ]);
        state.ui.view = View::Groups {
            selected_idx: Some(1),
        };

        state.set_groups(vec![
            group("Gamma", vec![]),
            group("Beta", vec![]),
            group("Alpha", vec![]),
        ]);

        assert_eq!(state.current_group().map(|g| g.name.as_str()), Some("Beta"));
    }

    #[test]
    fn set_groups_selects_first_group_when_previous_missing() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![
            group("Alpha", vec![]),
            group("Beta", vec![]),
            group("Gamma", vec![]),
        ]);
        state.ui.view = View::Groups {
            selected_idx: Some(1),
        };

        state.set_groups(vec![group("Delta", vec![]), group("Epsilon", vec![])]);

        assert_eq!(
            state.current_group().map(|g| g.name.as_str()),
            Some("Delta")
        );
    }

    #[test]
    fn set_groups_preserves_selected_var_by_name() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![group(
            "Vars",
            vec![var("alpha", "1"), var("beta", "2"), var("gamma", "3")],
        )]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(1),
        };

        state.set_groups(vec![group(
            "Vars",
            vec![var("gamma", "3"), var("beta", "2"), var("alpha", "1")],
        )]);

        assert_eq!(state.current_var().map(|v| v.name.as_str()), Some("beta"));
    }

    #[test]
    fn set_groups_falls_back_when_selected_var_missing() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![group(
            "Vars",
            vec![var("alpha", "1"), var("beta", "2")],
        )]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(1),
        };

        state.set_groups(vec![group("Vars", vec![var("alpha", "1")])]);

        assert_eq!(state.current_var().map(|v| v.name.as_str()), Some("alpha"));
    }

    #[test]
    fn set_groups_handles_missing_group_in_var_view() {
        let mut state = State::new("org".into(), "proj".into());
        state.set_groups(vec![
            group("A", vec![var("alpha", "1")]),
            group("B", vec![var("beta", "2")]),
        ]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(0),
        };

        state.set_groups(vec![group("B", vec![var("beta", "2"), var("bravo", "3")])]);

        assert_eq!(state.current_group().map(|g| g.name.as_str()), Some("B"));
        assert_eq!(state.current_var().map(|v| v.name.as_str()), Some("beta"));
    }
}
