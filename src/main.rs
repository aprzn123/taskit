mod common;
mod input;
mod output;

use std::{
    fs::{File, create_dir_all, rename},
    io::{Read, Write},
    path::Path,
};

use clap::{Parser, Subcommand};
use common::{Apply, SaveData, SaveDataVersioned};
use directories::ProjectDirs;

#[derive(clap::Parser, Debug)]
struct CliArgs {
    #[command(subcommand)]
    command: CliSubcommands,
}

#[derive(Subcommand, Debug)]
enum CliSubcommands {
    /// Add a new event, manually inputting all of its fields.
    #[clap(alias = "add")]
    Record,
    /// Start a timer and add it as an event once it's done.
    #[clap(alias = "time", alias = "start")]
    Stopwatch,
    /// Open the dashboard that displays all tracked time and allows you to filter events.
    #[clap(alias = "list")]
    Show,
    /// Modify a previously added event.
    Amend { 
        /// Amend the most recently added event.
        #[arg(long)]
        latest: bool 
    },
    /// Mark a category as archived, so no new events will be added to it.
    Archive { category: String },
    /// Add a tag to a category for larger aggregation.
    Tag,
    /// Add a note to a day
    Note,
}

fn main() {
    #[cfg(debug_assertions)]
    let project_dirs = ProjectDirs::from("xyz", "interestingzinc", "taskit_debug").unwrap();
    #[cfg(not(debug_assertions))]
    let project_dirs = ProjectDirs::from("xyz", "interestingzinc", "taskit").unwrap();
    let save_data_file_path = {
        let mut path = project_dirs.data_dir().to_path_buf();
        if !path.exists() {
            println!("data directory does not exist. creating...");
            create_dir_all(&path).unwrap();
        }
        path.push("save.json");
        path
    };
    let (save_data, upgraded) = read_save_data(&save_data_file_path).extract();
    if upgraded {
        rename(
            &save_data_file_path,
            save_data_file_path.with_extension(".upgrade_bak"),
        )
        .unwrap();
        write_save_data(save_data.clone(), &save_data_file_path);
    }
    let cli_args = CliArgs::parse();
    let save_delta = match cli_args.command {
        CliSubcommands::Record => input::record_main(save_data),
        CliSubcommands::Stopwatch => input::stopwatch_main(save_data),
        CliSubcommands::Amend { latest: true } => input::amend_main(save_data, 0),
        CliSubcommands::Amend { latest: false } => input::dispatch_amend(save_data),
        CliSubcommands::Show => output::filter_main(save_data),
        CliSubcommands::Archive { category } => input::archive_main(save_data, category),
        CliSubcommands::Tag => input::tag_main(save_data),
        CliSubcommands::Note => input::note_main(save_data),
    };
    let mut save_data = read_save_data(&save_data_file_path).extract().0;
    save_data.apply(save_delta);
    write_save_data(save_data, &save_data_file_path);
}

fn read_save_data(path: impl AsRef<Path>) -> SaveDataVersioned {
    let mut save_data = String::new();
    if let Ok(mut save_data_file) = File::open(path) {
        save_data_file.read_to_string(&mut save_data).unwrap();
        serde_json::from_str::<SaveDataVersioned>(&save_data).unwrap()
    } else {
        Default::default()
    }
}

fn write_save_data(data: SaveData, path: impl AsRef<Path>) {
    let save_data_temp_path = path.as_ref().with_extension("tmp");
    {
        let mut save_data_temp_file = File::create(&path).unwrap();
        save_data_temp_file.write_all(&serde_json::to_vec(&SaveDataVersioned::from(data)).unwrap());
    }
    rename(save_data_temp_path, &path);
}
