use async_trait::async_trait;
use azure_devops_rust_api::distributed_task::variablegroups;

use crate::azure_vars::{
    application::ports::VariableGroupsClient,
    domain::models::{VarEntry, VarGroup},
};

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
        let response = self
            .client
            .get_variable_groups(organization.to_string(), project.to_string())
            .await?;

        Ok(response
            .value
            .into_iter()
            .filter_map(|group| {
                let name = group.name.clone()?;
                let variables = group.variables.as_ref()?.as_object()?;
                let variables = variables
                    .iter()
                    .map(|(name, value)| {
                        let is_secret = value
                            .get("isSecret")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false);
                        let value = if is_secret {
                            "<secret value hidden>".to_string()
                        } else {
                            value
                                .get("value")
                                .and_then(|value| value.as_str())
                                .unwrap_or("<no value>")
                                .to_string()
                        };
                        VarEntry {
                            name: name.clone(),
                            value,
                            is_secret,
                        }
                    })
                    .collect();
                Some(VarGroup { name, variables })
            })
            .collect())
    }
}
