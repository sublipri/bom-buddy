use anyhow::Result;
use bom_buddy::{client::Client, location::Location};
use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
use inquire::{Select, Text};
use path_dsl::path;
use std::env::args;
use std::fs::{self, create_dir_all, File};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, Level};
use tracing_subscriber::FmtSubscriber;

fn main() -> Result<()> {
    setup_logging();
    let client = Client::new();
    let state_file = get_state_file();

    let mut location = if state_file.exists() {
        debug!("Reading state from {}", state_file.display());
        let file = File::open(&state_file)?;
        serde_json::from_reader(file)?
    } else if let Some(geohash) = args().nth(1) {
        let location = client.get_location(&geohash)?;
        write_state_file(&state_file, &location)?;
        location
    } else {
        let location = search_for_location(&client)?;
        write_state_file(&state_file, &location)?;
        location
    };

    loop {
        let was_updated = client.update_if_due(&mut location)?;
        if was_updated {
            write_state_file(&state_file, &location)?;
        }
        sleep(Duration::from_secs(1));
    }
}

fn search_for_location(client: &Client) -> Result<Location> {
    loop {
        let input = Text::new("Enter your location").prompt().unwrap();
        let results = client.search(&input)?;
        if results.is_empty() {
            println!("No search results for {input}");
            continue;
        } else if results.len() == 1 {
            let result = &results[0];
            println!("Selecting only result: {result}");
            return client.get_location(&result.geohash);
        };

        let selection = Select::new("Select a result: ", results).prompt();
        let selection = match selection {
            Ok(choice) => choice,
            Err(_) => {
                println!("An error occured. Please try again.");
                continue;
            }
        };
        return client.get_location(&selection.geohash);
    }
}

fn get_state_file() -> PathBuf {
    let strategy = choose_app_strategy(AppStrategyArgs {
        top_level_domain: "org".to_string(),
        author: "sublipri".to_string(),
        app_name: "BOM Buddy".to_string(),
    })
    .unwrap();

    let run_dir = strategy.runtime_dir().unwrap_or(strategy.data_dir());
    if !run_dir.exists() {
        create_dir_all(&run_dir).unwrap();
    }

    path!(run_dir | "state.json")
}

fn write_state_file(state_file: &PathBuf, location: &Location) -> Result<()> {
    let json = serde_json::to_string(&location)?;
    let tmp = state_file.with_file_name("tmp_state.json");
    debug!("Writing state to {}", tmp.display());
    fs::write(&tmp, json)?;
    debug!("Wrote state to {}", tmp.display());
    fs::rename(&tmp, state_file)?;
    Ok(())
}

fn setup_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
