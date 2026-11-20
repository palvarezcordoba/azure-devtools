use std::sync::{Arc, Mutex as StdMutex, MutexGuard};

use arboard::Clipboard;
use async_trait::async_trait;
use log::warn;
use tokio::{sync::Mutex as AsyncMutex, task::spawn_blocking};

use crate::azure_vars::application::ports::ClipboardAdapter;

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

#[derive(Default)]
pub struct SystemClipboard {
    clipboard: AsyncMutex<Option<SharedClipboard>>,
}

impl SystemClipboard {
    async fn clipboard_handle(&self) -> anyhow::Result<SharedClipboard> {
        if let Some(existing) = self.clipboard.lock().await.clone() {
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
        let result = spawn_blocking(move || {
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
