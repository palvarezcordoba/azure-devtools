use std::{fs, path::PathBuf};

use crate::azure_vars::{application::ports::VariableGroupExporter, domain::models::VarGroup};

#[derive(Default)]
pub struct JsonFileExporter;

impl VariableGroupExporter for JsonFileExporter {
    fn export(&self, group: &VarGroup, path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
        let mut group = group.clone();
        for variable in &mut group.variables {
            if variable.is_secret {
                variable.value = "<secret value hidden>".to_string();
            }
        }
        let path = path
            .unwrap_or_else(|| format!("{}_variables.json", group.name.replace(' ', "_")).into());
        let variables = group
            .variables
            .iter()
            .map(|variable| {
                serde_json::json!({
                    "name": variable.name,
                    "value": variable.value,
                    "is_secret": variable.is_secret,
                })
            })
            .collect::<Vec<_>>();
        let document = serde_json::json!({
            "name": group.name,
            "variables": variables,
        });
        fs::write(&path, serde_json::to_string_pretty(&document)?)?;
        Ok(path)
    }
}
