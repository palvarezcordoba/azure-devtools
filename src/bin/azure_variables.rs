use azure_devops_rust_api::{Credential, distributed_task::ClientBuilder};
use azure_devtools::azure_vars::state::{
    action::Action,
    state::State,
    state_store::{AzureApiVariableGroupsClient, StateStore},
};
use azure_devtools::azure_vars::tui::handle_input::run_app;
use azure_identity::AzureCliCredential;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::path::PathBuf;
use tokio::sync::mpsc::channel;

use clap::{Parser, Subcommand};

#[derive(Debug, Subcommand, Clone)]
enum SubCommands {
    Init,
    Tui,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: SubCommands,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    organization: String,
    project: String,
}

fn ensure_init(args: &Args, config_path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    if matches!(args.command, SubCommands::Init) {
        if config_path.exists() {
            println!(
                "Config file already exists at {}",
                config_path.to_string_lossy()
            );
            std::process::exit(0);
        }

        let organization = dialoguer::Input::<String>::new()
            .with_prompt("Azure DevOps Organization")
            .interact_text()?;
        let project = dialoguer::Input::<String>::new()
            .with_prompt("Azure DevOps Project")
            .interact_text()?;

        let config = Config {
            organization,
            project,
        };
        let config_yaml = serde_yaml::to_string(&config)?;
        std::fs::write(config_path, config_yaml)?;

        println!("Config file created at {}", config_path.to_string_lossy());
        println!("You can now run the 'tui' subcommand to manage variable groups.");

        print!("Alternatively, you can set the ADO_ORGANIZATION and ADO_PROJECT environment");
        println!(" variables to override the config values on a per-run basis if needed.\n");
        std::process::exit(0);
    } else if !config_path.exists() {
        println!(
            "Config file not found at {}. Please run 'init' command first.",
            config_path.to_string_lossy()
        );
        std::process::exit(1);
    } else {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let config_dir = dirs::config_dir()
        .ok_or("Could not determine config directory")?
        .join("azure_devtools");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.yaml");

    ensure_init(&args, &config_path)?;

    if PathBuf::from("logging.yaml").exists() {
        println!("Using logging configuration from logging.yaml");
        log4rs::init_file("logging.yaml", Default::default()).unwrap();
    }
    let config = serde_yaml::from_str::<Config>(&std::fs::read_to_string(&config_path)?)?;
    let azure_cli_credential = AzureCliCredential::new(None)?;
    let credential = Credential::from_token_credential(azure_cli_credential);
    let organization = env::var("ADO_ORGANIZATION").unwrap_or(config.organization);
    let project = env::var("ADO_PROJECT").unwrap_or(config.project);

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (action_tx, action_rx) = channel(10);
    let (state_tx, state_rx) = channel(10);
    let state = State::new(organization, project);
    let client = ClientBuilder::new(credential).build();
    let var_groups_client = AzureApiVariableGroupsClient::new(client.variablegroups_client());
    let state_store = StateStore::new(state.clone(), state_tx, var_groups_client);

    let state_task = state_store.main_loop(action_rx);
    action_tx.send(Action::RefreshVarGroups).await?;
    tokio::spawn(state_task);
    run_app(&mut terminal, action_tx, state, state_rx).await?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
