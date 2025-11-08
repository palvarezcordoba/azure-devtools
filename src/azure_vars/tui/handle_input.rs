use crossterm::event::{Event, EventStream, KeyEvent};
use futures::StreamExt;
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc::Sender;

use ratatui::DefaultTerminal;

use crate::azure_vars::state::action::Action;
use crate::azure_vars::state::state::State;
use crate::azure_vars::tui::draw::draw_ui;

const RENDERING_TICK_RATE: Duration = Duration::from_millis(250);

pub async fn run_app(
    terminal: &mut DefaultTerminal,
    mut action_tx: Sender<Action>,
    mut state: State,
    mut state_rx: tokio::sync::mpsc::Receiver<State>,
) -> Result<(), Box<dyn Error>> {
    let mut crossterm_events = EventStream::new();
    let mut ticker = tokio::time::interval(RENDERING_TICK_RATE);
    loop {
        terminal.draw(|f| draw_ui(f, &state))?;

        tokio::select! {
            _ = ticker.tick() => {}
            Some(new_state) = state_rx.recv() => {
                state = new_state;
            }
            maybe_event = crossterm_events.next() => match maybe_event {
                Some(Ok(Event::Key(key))) => {
                    if handle_key(&state, &mut action_tx, key).await? {
                        return Ok(());
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(Box::new(e)),
                None => break Ok(()), // User Interrupted
            }
        }
    }
}

async fn handle_key(
    state: &State,
    action_tx: &mut Sender<Action>,
    key: KeyEvent,
) -> anyhow::Result<bool> {
    use crossterm::event::KeyCode::*;

    if state.ui.search.is_active() {
        let action = match key.code {
            Esc => Action::ExitSearchMode,
            Enter => Action::SubmitSearch,
            Backspace => Action::SearchBackspace,
            Char(c) => Action::SearchInsertChar { ch: c },
            _ => return Ok(false),
        };
        action_tx.send(action).await?;
        return Ok(false);
    }

    assert!(!state.ui.search.is_active());
    let action = match key.code {
        Char('q') => return Ok(true),
        Char('/') => Action::EnterSearchMode,
        Char('R') => Action::RefreshVarGroups,
        Char('T') => Action::ToggleTheme,
        Char('C') if state.is_viewing_vars() => Action::CopySelectedVar,
        Char('E') if state.is_viewing_vars() => Action::ExportCurrentGroup,
        Left if state.is_viewing_vars() => Action::ExitViewVarGroup,
        Enter if !state.is_viewing_vars() => {
            if let Some(index) = state.current_group_idx() {
                Action::EnterViewVarGroup { index }
            } else {
                return Ok(false);
            }
        }
        Up => Action::MoveSelectionUp,
        Down => Action::MoveSelectionDown,
        PageUp => Action::MoveSelectionPageUp,
        PageDown => Action::MoveSelectionPageDown,
        Home => Action::MoveSelectionTop,
        End => Action::MoveSelectionBottom,
        _ => return Ok(false),
    };

    action_tx.send(action).await?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[tokio::test]
    async fn pressing_r_triggers_refresh_action() {
        let state = State::new("org".into(), "proj".into());
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut tx = tx;
        let key = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE);

        let should_quit = handle_key(&state, &mut tx, key).await.unwrap();
        assert!(!should_quit);

        let action = rx.recv().await.expect("action should be sent");
        assert!(matches!(action, Action::RefreshVarGroups));
    }
}
