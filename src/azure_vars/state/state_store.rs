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

#[async_trait]
pub trait ClipboardAdapter: Send + Sync {
    async fn set_text(&self, text: String) -> anyhow::Result<()>;
}

#[derive(Default)]
pub struct SystemClipboard {
    clipboard: AsyncMutex<Option<SharedClipboard>>,
}

impl SystemClipboard {
    async fn clipboard_handle(&self) -> anyhow::Result<SharedClipboard> {
        if let Some(existing) = {
            let guard = self.clipboard.lock().await;
            guard.clone()
        } {
            return Ok(existing);
        }

        let clipboard = match spawn_blocking(Clipboard::new).await {
            Ok(Ok(clipboard)) => clipboard,
            Ok(Err(error)) => {
                warn!("Failed to initialize clipboard: {error}");
                return Err(anyhow::anyhow!("Failed to initialize clipboard: {error}"));
            }
            Err(error) => {
                warn!("Clipboard init task failed: {error}");
                return Err(anyhow::anyhow!("Clipboard init task failed: {error}"));
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
        self.clipboard.lock().await.take();
    }
}

#[async_trait]
impl ClipboardAdapter for SystemClipboard {
    async fn set_text(&self, text: String) -> anyhow::Result<()> {
        let clipboard = self.clipboard_handle().await?;
        let result = spawn_blocking(move || -> Result<(), arboard::Error> {
            let mut guard = clipboard.lock()?;
            guard.set_text(text)
        })
        .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                warn!("Failed to copy variable to clipboard: {error}");
                self.clear_clipboard_handle().await;
                Err(anyhow::anyhow!(
                    "Failed to copy variable to clipboard: {error}"
                ))
            }
            Err(error) => {
                warn!("Clipboard task failed: {error}");
                self.clear_clipboard_handle().await;
                Err(anyhow::anyhow!("Clipboard task failed: {error}"))
            }
        }
    }
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

pub struct StateStore<C: VariableGroupsClient, B: ClipboardAdapter = SystemClipboard> {
    var_groups_client: C,
    state: State,
    state_tx: Sender<State>,
    clipboard: B,
}

impl<C: VariableGroupsClient> StateStore<C, SystemClipboard> {
    pub fn new(state: State, state_tx: Sender<State>, var_groups_client: C) -> Self {
        Self::with_clipboard(
            state,
            state_tx,
            var_groups_client,
            SystemClipboard::default(),
        )
    }
}

impl<C: VariableGroupsClient, B: ClipboardAdapter> StateStore<C, B> {
    pub fn with_clipboard(
        state: State,
        state_tx: Sender<State>,
        var_groups_client: C,
        clipboard: B,
    ) -> Self {
        Self {
            state_tx,
            var_groups_client,
            state,
            clipboard,
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
                    self.state.insert_search_char(ch);
                }
                Action::SearchBackspace => {
                    assert!(self.state.ui.search.is_active());
                    self.state.backspace_search();
                }
                Action::SubmitSearch => {
                    assert!(self.state.ui.search.is_active());
                    if let Some(target) = self.state.ui.search.active_target() {
                        self.state.sync_search_selection(target);
                    }
                    self.state.ui.search.deactivate();
                }
                Action::EnterViewVarGroup { index } => {
                    assert!(!self.state.is_viewing_vars());
                    if index >= self.state.groups().len() {
                        continue;
                    }
                    let group_name = self.state.groups()[index].name.clone();
                    let selected_var_idx =
                        self.state.filtered_var_indices_for(index).first().copied();
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
                    self.state.sync_selection();
                    self.state.ui.search.deactivate();
                }
                Action::ToggleTheme => {
                    self.toggle_theme();
                }
                Action::CopySelectedVar => {
                    self.copy_selected_var().await;
                }
                Action::ExportCurrentGroup => {
                    match self.export_current_group(Option::<PathBuf>::None) {
                        Ok(path) => {
                            info!("Exported variable group to {}", path.display());
                            self.state.ui.set_status(StatusMessage::info(format!(
                                "Exported variable group to {}",
                                path.display()
                            )));
                        }
                        Err(error) => {
                            warn!("Failed to export variable group: {error}");
                            self.state.ui.set_status(StatusMessage::error(format!(
                                "Failed to export variable group: {error}"
                            )));
                        }
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

    fn export_current_group(&self, path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
        let mut group = self
            .state
            .current_group()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No variable group selected to export"))?;
        for variable in &mut group.variables {
            if variable.is_secret {
                variable.value = "<secret value hidden>".to_string();
            }
        }
        let path = path
            .unwrap_or_else(|| format!("{}_variables.json", group.name.replace(' ', "_")).into());
        let json = serde_json::to_string_pretty(&group)?;
        fs::write(&path, json)?;
        Ok(path)
    }

    async fn copy_selected_var(&mut self) {
        let Some(var) = self.state.current_var() else {
            self.state
                .ui
                .set_status(StatusMessage::error("No variable selected to copy"));
            return;
        };
        let text = format!("{}={}", var.name, var.value);
        info!("Copying selected variable to clipboard");
        match self.clipboard.set_text(text).await {
            Ok(()) => self
                .state
                .ui
                .set_status(StatusMessage::info("Copied variable to clipboard")),
            Err(error) => {
                warn!("Failed to copy variable to clipboard: {error}");
                self.state.ui.set_status(StatusMessage::error(format!(
                    "Failed to copy variable to clipboard: {error}"
                )));
            }
        }
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

    struct TestClipboard {
        result: Result<(), String>,
        copied: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl ClipboardAdapter for TestClipboard {
        async fn set_text(&self, text: String) -> anyhow::Result<()> {
            self.copied.lock().unwrap().push(text);
            self.result
                .as_ref()
                .map(|_| ())
                .map_err(|error| anyhow::anyhow!("{error}"))
        }
    }

    async fn run_action<B: ClipboardAdapter + 'static>(
        state: State,
        clipboard: B,
        action: Action,
    ) -> State {
        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(8);
        let store = StateStore::with_clipboard(state, state_tx, MockVarClient::new(), clipboard);
        let (action_tx, action_rx) = tokio::sync::mpsc::channel(2);
        let main_loop = tokio::spawn(store.main_loop(action_rx));

        action_tx.send(action).await.unwrap();
        drop(action_tx);
        main_loop.await.unwrap();

        let mut latest = None;
        while let Some(state) = state_rx.recv().await {
            latest = Some(state);
        }
        latest.expect("action should emit a state")
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
                    value: "should never be exported".to_string(),
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
        let mut expected_group = group;
        expected_group.variables[1].value = "<secret value hidden>".to_string();
        let expected = serde_json::to_string_pretty(&expected_group).unwrap();
        assert_eq!(contents, expected);
    }

    #[tokio::test]
    async fn group_search_filters_matches_and_clears_selection_for_no_results() {
        let mut state = State::new("org".to_string(), "project".to_string());
        state.set_groups(vec![
            sample_group("Alpha Team", vec![]),
            sample_group("Beta Team", vec![]),
        ]);
        state.ui.view = View::Groups {
            selected_idx: Some(1),
        };
        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(32);
        let store = StateStore::new(state, state_tx, MockVarClient::new());
        let (action_tx, action_rx) = tokio::sync::mpsc::channel(16);
        let main_loop = tokio::spawn(store.main_loop(action_rx));

        action_tx.send(Action::EnterSearchMode).await.unwrap();
        for ch in "alpha".chars() {
            action_tx
                .send(Action::SearchInsertChar { ch })
                .await
                .unwrap();
        }
        action_tx
            .send(Action::SearchInsertChar { ch: 'z' })
            .await
            .unwrap();
        action_tx.send(Action::SearchBackspace).await.unwrap();
        action_tx.send(Action::ExitSearchMode).await.unwrap();
        action_tx.send(Action::MoveSelectionDown).await.unwrap();
        drop(action_tx);
        main_loop.await.unwrap();

        let mut states = Vec::new();
        while let Some(state) = state_rx.recv().await {
            states.push(state);
        }

        let no_match_state = &states[6];
        assert_eq!(no_match_state.ui.search.groups_query(), "alphaz");
        assert!(no_match_state.filtered_group_indices().is_empty());
        assert_eq!(no_match_state.current_group(), None);

        let final_state = states.last().expect("state updates should exist");
        assert_eq!(final_state.ui.search.groups_query(), "alpha");
        assert_eq!(final_state.filtered_group_indices(), vec![0]);
        assert_eq!(
            final_state.current_group().map(|group| group.name.as_str()),
            Some("Alpha Team")
        );
        assert!(!final_state.ui.search.is_active());
    }

    #[tokio::test]
    async fn copy_reports_success_and_failure_through_status() {
        let copied = Arc::new(Mutex::new(Vec::new()));
        let mut state = State::new("org".to_string(), "project".to_string());
        state.set_groups(vec![sample_group(
            "Vars",
            vec![sample_var("endpoint", "https://example.test")],
        )]);
        state.ui.view = View::Vars {
            group_idx: 0,
            selected_var_idx: Some(0),
        };
        let success = run_action(
            state.clone(),
            TestClipboard {
                result: Ok(()),
                copied: Arc::clone(&copied),
            },
            Action::CopySelectedVar,
        )
        .await;
        assert_eq!(
            copied.lock().unwrap().as_slice(),
            &[String::from("endpoint=https://example.test")]
        );
        assert!(matches!(
            &success.ui.status,
            Some(StatusMessage {
                kind: StatusKind::Info,
                ..
            })
        ));

        let failure = run_action(
            state,
            TestClipboard {
                result: Err("clipboard unavailable".to_string()),
                copied: Arc::new(Mutex::new(Vec::new())),
            },
            Action::CopySelectedVar,
        )
        .await;
        assert!(matches!(
            &failure.ui.status,
            Some(StatusMessage {
                kind: StatusKind::Error,
                ..
            })
        ));
        assert!(
            failure
                .ui
                .status
                .as_ref()
                .is_some_and(|status| status.text.contains("clipboard unavailable"))
        );
    }

    #[tokio::test]
    async fn copy_without_a_selected_variable_reports_an_error() {
        let state = State::new("org".to_string(), "project".to_string());
        let final_state = run_action(
            state,
            TestClipboard {
                result: Ok(()),
                copied: Arc::new(Mutex::new(Vec::new())),
            },
            Action::CopySelectedVar,
        )
        .await;
        assert!(matches!(
            &final_state.ui.status,
            Some(StatusMessage {
                kind: StatusKind::Error,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn export_reports_success_and_failure_through_status() {
        let dir = tempdir().unwrap();
        let group_name = dir.path().join("Group").to_string_lossy().to_string();
        let mut state = State::new("org".to_string(), "project".to_string());
        state.set_groups(vec![sample_group(
            &group_name,
            vec![sample_var("key", "value")],
        )]);
        state.ui.view = View::Groups {
            selected_idx: Some(0),
        };
        let success = run_action(
            state,
            TestClipboard {
                result: Ok(()),
                copied: Arc::new(Mutex::new(Vec::new())),
            },
            Action::ExportCurrentGroup,
        )
        .await;

        let export_path = PathBuf::from(format!("{group_name}_variables.json"));
        assert!(export_path.exists());
        assert!(matches!(
            &success.ui.status,
            Some(StatusMessage {
                kind: StatusKind::Info,
                ..
            })
        ));

        let missing_parent = dir
            .path()
            .join("missing/Group")
            .to_string_lossy()
            .to_string();
        let mut state = State::new("org".to_string(), "project".to_string());
        state.set_groups(vec![sample_group(&missing_parent, vec![])]);
        state.ui.view = View::Groups {
            selected_idx: Some(0),
        };
        let failure = run_action(
            state,
            TestClipboard {
                result: Ok(()),
                copied: Arc::new(Mutex::new(Vec::new())),
            },
            Action::ExportCurrentGroup,
        )
        .await;
        assert!(matches!(
            &failure.ui.status,
            Some(StatusMessage {
                kind: StatusKind::Error,
                ..
            })
        ));

        let no_selection = run_action(
            State::new("org".to_string(), "project".to_string()),
            TestClipboard {
                result: Ok(()),
                copied: Arc::new(Mutex::new(Vec::new())),
            },
            Action::ExportCurrentGroup,
        )
        .await;
        assert!(matches!(
            &no_selection.ui.status,
            Some(StatusMessage {
                kind: StatusKind::Error,
                ..
            })
        ));
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
        let mut initial_state = State::new("org".to_string(), "project".to_string());
        let original_groups = vec![sample_group(
            "Previously loaded",
            vec![sample_var("old", "snapshot")],
        )];
        initial_state.set_groups(original_groups.clone());
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

        let failure_state = states
            .iter()
            .find(|state| {
                matches!(&state.ui.status, Some(status) if status.text.contains("Failed to load variable groups"))
            })
            .expect("expected an error status after refresh failure");
        assert_eq!(failure_state.data.groups, original_groups);

        let final_state = states.last().expect("state updates should exist");
        assert_eq!(final_state.data.groups, desired_groups);
        assert!(
            final_state.ui.status.is_none(),
            "status should be cleared after successful refresh"
        );
    }
}
