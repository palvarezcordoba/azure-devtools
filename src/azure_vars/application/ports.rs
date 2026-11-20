use async_trait::async_trait;
use std::path::PathBuf;

use crate::azure_vars::domain::models::VarGroup;

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

pub trait VariableGroupExporter: Send + Sync {
    fn export(&self, group: &VarGroup, path: Option<PathBuf>) -> anyhow::Result<PathBuf>;
}
