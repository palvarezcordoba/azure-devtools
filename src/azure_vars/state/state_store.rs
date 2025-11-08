use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex, MutexGuard},
};

use crate::azure_vars::state::state::*;
use arboard::Clipboard;
use async_trait::async_trait;
use azure_devops_rust_api::distributed_task::variablegroups;
use log::{info, warn};
use tokio::{
    sync::{
        Mutex as AsyncMutex,
        mpsc::{Receiver, Sender},
    },
    task::spawn_blocking,
};

use super::action::Action;

#[derive(Clone)]
struct SharedClipboard(Arc<StdMutex<Clipboard>>);

impl SharedClipboard {
    fn new(inner: Clipboard) -> Self {
        Self(Arc::new(StdMutex::new(inner)))
    }

    fn lock(&self) -> Result<MutexGuard<'_, Clipboard>, arboard::Error> {
        self.0.lock().map_err(|_| arboard::Error::Unknown {
            description: "Clipboard mutex poisoned".to_string(),
        })
    }
}

#[async_trait]
pub trait VariableGroupsClient: Send {
    async fn get_variable_groups(
        &self,
        organization: &str,
        project: &str,
    ) -> anyhow::Result<Vec<VarGroup>>;
}

pub struct AzureApiVariableGroupsClient {
    client: variablegroups::Client,
}

impl AzureApiVariableGroupsClient {
    pub fn new(client: variablegroups::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl VariableGroupsClient for AzureApiVariableGroupsClient {
    async fn get_variable_groups(
        &self,
        organization: &str,
        project: &str,
    ) -> anyhow::Result<Vec<VarGroup>> {
        let groups_resp = self
            .client
            .get_variable_groups(organization.to_string(), project.to_string())
            .await?;

        let groups = groups_resp
            .value
            .into_iter()
            .filter_map(|g| {
                let name = g.name.clone()?;
                let vars = g.variables.as_ref()?.as_object()?;
                let variables = vars
                    .iter()
                    .map(|(k, v)| {
                        let is_secret =
                            v.get("isSecret").and_then(|b| b.as_bool()).unwrap_or(false);
                        let value = if is_secret {
                            "<secret value hidden>".to_string()
                        } else {
                            v.get("value")
                                .and_then(|vv| vv.as_str())
                                .unwrap_or("<no value>")
                                .to_string()
                        };
                        VarEntry {
                            name: k.clone(),
                            value,
                            is_secret,
                        }
                    })
                    .collect();
                Some(VarGroup { name, variables })
            })
            .collect::<Vec<_>>();

        Ok(groups)
    }
}

pub struct StateStore<C: VariableGroupsClient> {
    var_groups_client: C,
    state: State,
    state_tx: Sender<State>,
    clipboard: AsyncMutex<Option<SharedClipboard>>,
}

impl<C: VariableGroupsClient> StateStore<C> {
    pub fn new(state: State, state_tx: Sender<State>, var_groups_client: C) -> Self {
        Self {
            state_tx,
            var_groups_client,
            state,
            clipboard: AsyncMutex::new(None),
        }
    }

    pub async fn main_loop(mut self, action_rx: Receiver<Action>) {
        let mut action_rx = action_rx;

        while let Some(action) = action_rx.recv().await {
            info!("Received action: {action:?}");
            match action {
                Action::RefreshVarGroups => {
                    self.state.ui.is_fetching = true;
                    self.state_tx.send(self.state.clone()).await.unwrap();
                    match self.fetch_var_groups().await {
                        Ok(groups) => {
                            self.state.set_groups(groups);
                            self.state.ui.clear_status();
                        }
                        Err(error) => {
                            warn!("Failed to fetch variable groups: {error}");
                            self.state.ui.set_status(StatusMessage::error(format!(
                                "Failed to load variable groups: {error}"
                            )));
                        }
                    }
                    self.state.ui.is_fetching = false;
                }
                Action::EnterSearchMode => {
                    assert!(!self.state.ui.search.is_active());
                    let target = if self.state.is_viewing_vars() {
                        SearchTarget::Vars
                    } else {
                        SearchTarget::Groups
                    };
                    self.state.ui.search.activate(target);
                }
                Action::ExitSearchMode => {
                    assert!(self.state.ui.search.is_active());
                    self.state.ui.search.deactivate();
                }
                Action::SearchInsertChar { ch } => {
                    assert!(self.state.ui.search.is_active());
                    if let Some(target) = self.state.ui.search.active_target() {
                        match target {
                            SearchTarget::Groups => {
                                self.state.ui.search.groups_query_mut().push(ch);
                                self.state.invalidate_group_cache();
                                self.apply_search_selection(SearchTarget::Groups);
                            }
                            SearchTarget::Vars => {
                                if let Some(group_idx) = self.state.vars_group_idx() {
                                    let query = self.state.ui.search.vars_query_mut(group_idx);
                                    query.push(ch);
                                    self.state.invalidate_var_cache();
                                    self.apply_search_selection(SearchTarget::Vars);
                                }
                            }
                        }
                    }
                }
                Action::SearchBackspace => {
                    assert!(self.state.ui.search.is_active());
                    if let Some(target) = self.state.ui.search.active_target() {
                        match target {
                            SearchTarget::Groups => {
                                let query = self.state.ui.search.groups_query_mut();
                                query.pop();
                                self.state.invalidate_group_cache();
                                self.apply_search_selection(SearchTarget::Groups);
                            }
                            SearchTarget::Vars => {
                                if let Some(group_idx) = self.state.vars_group_idx() {
                                    let should_remove = {
                                        let query = self.state.ui.search.vars_query_mut(group_idx);
                                        query.pop();
                                        query.is_empty()
                                    };
                                    if should_remove {
                                        self.state.ui.search.clear_vars_query(group_idx);
                                    }
                                    self.state.invalidate_var_cache();
                                    self.apply_search_selection(SearchTarget::Vars);
                                }
                            }
                        }
                    }
                }
                Action::SubmitSearch => {
                    assert!(self.state.ui.search.is_active());
                    if let Some(target) = self.state.ui.search.active_target() {
                        self.apply_search_selection(target);
                    }
                    self.state.ui.search.deactivate();
                }
                Action::EnterViewVarGroup { index } => {
                    assert!(!self.state.is_viewing_vars());
                    if index >= self.state.groups().len() {
                        continue;
                    }
                    let (selected_var_idx, group_name) = {
                        let group = &self.state.groups()[index];
                        (
                            if group.variables.is_empty() {
                                None
                            } else {
                                Some(0)
                            },
                            group.name.clone(),
                        )
                    };
                    self.state.ui.view = View::Vars {
                        group_idx: index,
                        selected_var_idx,
                    };
                    info!("Entering variable group view: {group_name}");
                    self.state.ui.search.deactivate();
                }
                Action::ExitViewVarGroup => {
                    assert!(self.state.is_viewing_vars());
                    let selected_idx = self.state.current_group_idx();
                    self.state.ui.view = View::Groups { selected_idx };
                    self.state.ui.search.deactivate();
                }
                Action::ToggleTheme => {
                    self.toggle_theme();
                }
                Action::CopySelectedVar => {
                    self.copy_selected_var().await;
                }
                Action::ExportCurrentGroup => {
                    if let Err(error) = self.export_current_group(Option::<String>::None) {
                        eprintln!("Failed to export variable group: {error}");
                        continue;
                    };
                }
                Action::MoveSelectionUp => {
                    self.move_selection(-1);
                }
                Action::MoveSelectionDown => {
                    self.move_selection(1);
                }
                Action::MoveSelectionTop => {
                    let len = if self.state.is_viewing_vars() {
                        self.state.filtered_var_indices().len()
                    } else {
                        self.state.filtered_group_indices().len()
                    } as isize;
                    self.move_selection(-len);
                }
                Action::MoveSelectionBottom => {
                    let len = if self.state.is_viewing_vars() {
                        self.state.filtered_var_indices().len()
                    } else {
                        self.state.filtered_group_indices().len()
                    } as isize;
                    self.move_selection(len);
                }
                Action::MoveSelectionPageUp => {
                    self.move_selection(-10);
                }
                Action::MoveSelectionPageDown => {
                    self.move_selection(10);
                }
            }
            self.state_tx.send(self.state.clone()).await.unwrap();
        }
    }

    fn toggle_theme(&mut self) {
        self.state.theme = match self.state.theme {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        };
    }

    fn export_current_group(&self, path: Option<impl Into<PathBuf>>) -> anyhow::Result<()> {
        let group = self
            .state
            .current_group()
            .ok_or_else(|| anyhow::anyhow!("No variable group selected to export"))?;
        let path = path
            .map(Into::into)
            .unwrap_or_else(|| format!("{}_variables.json", group.name.replace(' ', "_")).into());
        let json = serde_json::to_string_pretty(&group)?;
        fs::write(path, json)?;
        Ok(())
    }

    async fn copy_selected_var(&mut self) {
        let Some(var) = self.state.current_var() else {
            return;
        };
        let text = format!("{}={}", var.name, var.value);
        if text.is_empty() {
            return;
        }
        info!("Copying selected variable to clipboard");
        let clipboard = match self.clipboard_handle().await {
            Ok(handle) => handle,
            Err(message) => {
                self.state.ui.set_status(StatusMessage::error(message));
                return;
            }
        };
        let payload = text;
        match spawn_blocking(move || -> Result<(), arboard::Error> {
            let mut guard = clipboard.lock()?;
            guard.set_text(payload)
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                warn!("Failed to copy variable to clipboard: {err}");
                self.state.ui.set_status(StatusMessage::error(format!(
                    "Failed to copy variable to clipboard: {err}"
                )));
                self.clear_clipboard_handle().await;
            }
            Err(err) => {
                warn!("Clipboard task failed: {err}");
                self.state.ui.set_status(StatusMessage::error(format!(
                    "Clipboard task failed: {err}"
                )));
                self.clear_clipboard_handle().await;
            }
        }
    }

    async fn clipboard_handle(&self) -> Result<SharedClipboard, String> {
        if let Some(existing) = {
            let guard = self.clipboard.lock().await;
            guard.clone()
        } {
            return Ok(existing);
        }

        let clipboard = match spawn_blocking(Clipboard::new).await {
            Ok(Ok(cb)) => cb,
            Ok(Err(err)) => {
                warn!("Failed to initialize clipboard: {err}");
                return Err(format!("Failed to initialize clipboard: {err}"));
            }
            Err(err) => {
                warn!("Clipboard init task failed: {err}");
                return Err(format!("Clipboard init task failed: {err}"));
            }
        };

        let handle = SharedClipboard::new(clipboard);
        let mut guard = self.clipboard.lock().await;
        if let Some(existing) = guard.as_ref() {
            return Ok(existing.clone());
        }
        *guard = Some(handle.clone());
        Ok(handle)
    }

    async fn clear_clipboard_handle(&self) {
        let mut guard = self.clipboard.lock().await;
        guard.take();
    }

    fn move_selection(&mut self, delta: isize) {
        if self.state.is_viewing_vars() {
            let Some(group_idx) = self.state.vars_group_idx() else {
                return;
            };
            let filtered = self.state.filtered_var_indices_for(group_idx);
            if let View::Vars {
                selected_var_idx, ..
            } = &mut self.state.ui.view
            {
                Self::shift_selection(delta, filtered, selected_var_idx);
            }
        } else {
            let filtered = self.state.filtered_group_indices();
            if let View::Groups { selected_idx } = &mut self.state.ui.view {
                Self::shift_selection(delta, filtered, selected_idx);
            }
        }
    }

    fn shift_selection(delta: isize, filtered: Vec<usize>, selected_idx: &mut Option<usize>) {
        if filtered.is_empty() {
            *selected_idx = None;
            return;
        }

        let current_position = selected_idx
            .and_then(|idx| filtered.iter().position(|candidate| *candidate == idx))
            .unwrap_or(0);
        let next =
            (current_position as isize + delta).clamp(0, (filtered.len() - 1) as isize) as usize;
        *selected_idx = Some(filtered[next]);
    }

    fn apply_search_selection(&mut self, target: SearchTarget) {
        match target {
            SearchTarget::Groups => {
                let next = self.state.filtered_group_indices().first().copied();
                match next {
                    Some(idx) => {
                        let first_var = self.state.filtered_var_indices_for(idx).first().copied();
                        match &mut self.state.ui.view {
                            View::Groups { selected_idx } => {
                                *selected_idx = Some(idx);
                            }
                            View::Vars {
                                group_idx,
                                selected_var_idx,
                            } => {
                                *group_idx = idx;
                                *selected_var_idx = first_var;
                            }
                        }
                    }
                    None => {
                        self.state.ui.view = View::Groups { selected_idx: None };
                    }
                }
            }
            SearchTarget::Vars => {
                let Some(group_idx) = self.state.vars_group_idx() else {
                    return;
                };
                let filtered = self.state.filtered_var_indices_for(group_idx);
                if let View::Vars {
                    selected_var_idx, ..
                } = &mut self.state.ui.view
                {
                    *selected_var_idx = filtered.first().copied();
                }
            }
        }
    }

    async fn fetch_var_groups(&mut self) -> anyhow::Result<Vec<VarGroup>> {
        self.var_groups_client
            .get_variable_groups(self.state.organization(), self.state.project())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    use tempfile::tempdir;
    use tokio::task::JoinHandle;

    mock! {
        pub VarClient {}

        #[async_trait]
        impl VariableGroupsClient for VarClient {
            async fn get_variable_groups(
                &self,
                organization: &str,
                project: &str,
            ) -> anyhow::Result<Vec<VarGroup>>;
        }
    }

    type TestStore = StateStore<MockVarClient>;

    fn build_store_with_client(state: State, client: MockVarClient) -> TestStore {
        let (state_tx, _rx) = tokio::sync::mpsc::channel(1);
        StateStore::new(state, state_tx, client)
    }

    fn build_store(state: State) -> TestStore {
        build_store_with_client(state, MockVarClient::new())
    }

    fn sample_var(name: &str, value: &str) -> VarEntry {
        VarEntry {
            name: name.to_string(),
            value: value.to_string(),
            is_secret: false,
        }
    }

    fn sample_group(name: &str, vars: Vec<VarEntry>) -> VarGroup {
        VarGroup {
            name: name.to_string(),
            variables: vars,
        }
    }

    #[test]
    fn move_selection_clamps_within_group_bounds() {
        let mut state = State::new("org".to_string(), "project".to_string());
        state.set_groups(vec![
            sample_group("Alpha", vec![]),
            sample_group("Beta", vec![]),
            sample_group("Gamma", vec![]),
        ]);

        let mut store = build_store(state);

        let second_expected = store.state.filtered_groups()[1].name.clone();
        store.move_selection(1);
        assert_eq!(
            store.state.current_group().map(|g| &g.name),
            Some(&second_expected)
        );

        let last_expected = store.state.filtered_groups().last().unwrap().name.clone();
        store.move_selection(10);
        assert_eq!(
            store.state.current_group().map(|g| &g.name),
            Some(&last_expected)
        );

        let first_expected = store.state.filtered_groups()[0].name.clone();
        store.move_selection(-10);
        assert_eq!(
            store.state.current_group().map(|g| &g.name),
            Some(&first_expected)
        );
    }

    #[test]
    fn move_selection_updates_variable_focus() {
        let mut state = State::new("org".to_string(), "project".to_string());
        let vars = vec![
            sample_var("alpha", "1"),
            sample_var("beta", "2"),
            sample_var("gamma", "3"),
        ];
        let group = sample_group("vars", vars);
        state.set_groups(vec![group]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(0),
        };

        let mut store = build_store(state);

        let filtered = store.state.filtered_vars();
        assert!(filtered.len() >= 3);
        let second_expected = filtered[1].name.clone();
        store.move_selection(1);
        assert_eq!(
            store.state.current_var().map(|v| &v.name),
            Some(&second_expected)
        );

        let last_expected = store.state.filtered_vars().last().unwrap().name.clone();
        store.move_selection(10);
        assert_eq!(
            store.state.current_var().map(|v| &v.name),
            Some(&last_expected)
        );

        let first_expected = store.state.filtered_vars()[0].name.clone();
        store.move_selection(-10);
        assert_eq!(
            store.state.current_var().map(|v| &v.name),
            Some(&first_expected)
        );
    }

    #[test]
    fn toggle_theme_switches_between_variants() {
        let mut state = State::new("org".to_string(), "project".to_string());
        state.theme = Theme::Dark;
        let mut store = build_store(state);

        store.toggle_theme();
        assert!(matches!(store.state.theme, Theme::Light));

        store.toggle_theme();
        assert!(matches!(store.state.theme, Theme::Dark));
    }

    #[test]
    fn export_current_group_persists_pretty_json() {
        let mut state = State::new("org".to_string(), "project".to_string());
        let group = sample_group(
            "My Group",
            vec![
                sample_var("key", "value"),
                VarEntry {
                    name: "secret".to_string(),
                    value: "<secret value hidden>".to_string(),
                    is_secret: true,
                },
            ],
        );
        state.set_groups(vec![group.clone()]);
        state.ui.view = View::Groups {
            selected_idx: Some(0),
        };

        let store = build_store(state);
        let dir = tempdir().unwrap();
        let export_path = dir.path().join("custom_group.json");

        store
            .export_current_group(Some(export_path.clone()))
            .expect("export should succeed");

        let contents = fs::read_to_string(&export_path).expect("exported file readable");
        let expected = serde_json::to_string_pretty(&group).unwrap();
        assert_eq!(contents, expected);
    }

    #[tokio::test]
    async fn main_loop_refresh_var_groups_updates_state() {
        let initial_state = State::new("org".to_string(), "project".to_string());
        let groups = vec![sample_group(
            "A",
            vec![sample_var("alpha", "1"), sample_var("beta", "2")],
        )];
        let mut client = MockVarClient::new();
        let groups_clone = groups.clone();
        client
            .expect_get_variable_groups()
            .return_once(move |_, _| Ok(groups_clone));
        let (state_tx, state_rx) = tokio::sync::mpsc::channel(4);
        let store = StateStore::new(initial_state, state_tx, client);

        let (action_tx, action_rx) = tokio::sync::mpsc::channel(4);

        let collector: JoinHandle<Vec<State>> = tokio::spawn(async move {
            let mut collected = Vec::new();
            let mut rx = state_rx;
            while let Some(state) = rx.recv().await {
                collected.push(state);
            }
            collected
        });

        let main_loop = tokio::spawn(store.main_loop(action_rx));

        action_tx.send(Action::RefreshVarGroups).await.unwrap();
        drop(action_tx);

        main_loop.await.unwrap();
        let states = collector.await.unwrap();

        assert!(
            states.len() >= 2,
            "expected at least two state broadcasts, got {}",
            states.len()
        );

        let final_state = states.last().unwrap();
        assert_eq!(final_state.data.groups, groups);
        assert_eq!(
            final_state.current_group().map(|g| g.name.as_str()),
            Some("A")
        );
        assert!(!final_state.is_viewing_vars());
    }

    #[tokio::test]
    async fn main_loop_handles_search_actions() {
        let mut initial_state = State::new("org".to_string(), "project".to_string());
        initial_state.set_groups(vec![sample_group(
            "Group",
            vec![sample_var("alpha", "1"), sample_var("beta", "2")],
        )]);
        initial_state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(0),
        };
        let client = MockVarClient::new();
        let (state_tx, state_rx) = tokio::sync::mpsc::channel(8);
        let store = StateStore::new(initial_state, state_tx, client);
        let (action_tx, action_rx) = tokio::sync::mpsc::channel(8);

        let collector: JoinHandle<Vec<State>> = tokio::spawn(async move {
            let mut collected = Vec::new();
            let mut rx = state_rx;
            while let Some(state) = rx.recv().await {
                collected.push(state);
            }
            collected
        });

        let main_loop = tokio::spawn(store.main_loop(action_rx));
        action_tx.send(Action::EnterSearchMode).await.unwrap();
        for ch in "alpha".chars() {
            action_tx
                .send(Action::SearchInsertChar { ch })
                .await
                .unwrap();
        }
        action_tx.send(Action::SubmitSearch).await.unwrap();
        action_tx.send(Action::EnterSearchMode).await.unwrap();
        action_tx.send(Action::ExitSearchMode).await.unwrap();
        drop(action_tx);

        main_loop.await.unwrap();
        let states = collector.await.unwrap();
        let final_state = states.last().cloned().expect("state updates should exist");
        assert!(!final_state.ui.search.is_active());
        assert_eq!(
            final_state.current_var().map(|v| v.name.as_str()),
            Some("alpha")
        );
    }

    #[tokio::test]
    async fn refresh_failures_update_status_and_success_clears() {
        let initial_state = State::new("org".to_string(), "project".to_string());
        let desired_groups = vec![sample_group(
            "Recovered",
            vec![sample_var("alpha", "1"), sample_var("beta", "2")],
        )];
        let responses = Arc::new(Mutex::new(VecDeque::from([
            Err(anyhow::anyhow!("network error")),
            Ok(desired_groups.clone()),
        ])));

        let mut client = MockVarClient::new();
        let responses_clone = Arc::clone(&responses);
        client
            .expect_get_variable_groups()
            .times(2)
            .returning(move |_, _| {
                responses_clone
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("response available")
            });

        let (state_tx, state_rx) = tokio::sync::mpsc::channel(8);
        let store = StateStore::new(initial_state, state_tx, client);
        let (action_tx, action_rx) = tokio::sync::mpsc::channel(8);

        let collector: JoinHandle<Vec<State>> = tokio::spawn(async move {
            let mut collected = Vec::new();
            let mut rx = state_rx;
            while let Some(state) = rx.recv().await {
                collected.push(state);
            }
            collected
        });

        let main_loop = tokio::spawn(store.main_loop(action_rx));
        action_tx.send(Action::RefreshVarGroups).await.unwrap();
        action_tx.send(Action::RefreshVarGroups).await.unwrap();
        drop(action_tx);

        main_loop.await.unwrap();
        let states = collector.await.unwrap();

        assert!(
            states
                .iter()
                .any(|state| matches!(&state.ui.status, Some(status) if status.text.contains("Failed to load variable groups"))),
            "expected at least one state with an error status",
        );

        let final_state = states.last().expect("state updates should exist");
        assert_eq!(final_state.data.groups, desired_groups);
        assert!(
            final_state.ui.status.is_none(),
            "status should be cleared after successful refresh"
        );
    }
}
