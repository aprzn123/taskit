mod common;
mod input;
mod tui;

use std::{
    fs::{File, create_dir_all, rename},
    io::{Read, Write},
    path::Path, process::ExitCode,
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
    /// Change the name of a category
    RenameCategory,
    /// Delete a previously recorded event
    DeleteEvent,
    /// Delete a category that has no events in it
    DeleteCategory,
    /// Delete any tag
    DeleteTag,
    /// Open a TUI to view and edit associations between categories and tags
    ManageTags,
}

fn main() -> ExitCode {
    let project_dirs = ProjectDirs::from(
        "xyz", 
        "interestingzinc", 
        if cfg!(debug_assertions) { "taskit_debug" } else { "taskit" }
    ).expect("assume that there is a home directory");

    let save_data_file_path = {
        let mut path = project_dirs.data_dir().to_path_buf();
        if !path.exists() {
            println!("data directory does not exist. creating...");
            create_dir_all(&path).expect("assume we have access to data directory - can't run without");
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
        .expect("assume file rename is possible");
        write_save_data(save_data.clone(), &save_data_file_path);
    }
    let cli_args = CliArgs::parse();
    let save_delta = match cli_args.command {
        CliSubcommands::Record => input::record_main(save_data),
        CliSubcommands::Stopwatch => input::stopwatch_main(save_data),
        CliSubcommands::Amend { latest: true } => input::amend_main(save_data, 0),
        CliSubcommands::Amend { latest: false } => input::dispatch_amend(save_data),
        CliSubcommands::Show => tui::filter_main(save_data),
        CliSubcommands::Archive { category } => input::archive_main(save_data, category),
        CliSubcommands::Tag => input::tag_main(save_data),
        CliSubcommands::Note => input::note_main(save_data),
        CliSubcommands::RenameCategory => input::rename_category(save_data),
        CliSubcommands::DeleteEvent => input::delete_event_main(save_data),
        CliSubcommands::DeleteCategory => input::delete_category_main(save_data),
        CliSubcommands::DeleteTag => input::delete_tag_main(save_data),
        CliSubcommands::ManageTags => tui::tagedit_main(save_data),
    };
    let save_delta = match save_delta {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e} No modifications made.");
            return ExitCode::FAILURE;
        },
    };
    if !save_delta.is_empty() {
        let mut save_data = read_save_data(&save_data_file_path).extract().0;
        save_data.apply(save_delta).expect("save_delta doesn't actually return an error ever");
        write_save_data(save_data, &save_data_file_path);
    }
    ExitCode::SUCCESS
}

fn read_save_data(path: impl AsRef<Path>) -> SaveDataVersioned {
    let mut save_data = String::new();
    if let Ok(mut save_data_file) = File::open(path) {
        save_data_file.read_to_string(&mut save_data).expect("save data file should be readable and utf-8");
        // TODO perhaps indicate the error in more detail in the case that deserialization fails?
        serde_json::from_str::<SaveDataVersioned>(&save_data).expect("save data file should be valid JSON in the save data format")
    } else {
        Default::default()
    }
}

fn write_save_data(data: SaveData, path: impl AsRef<Path>) {
    let save_data_temp_path = path.as_ref().with_extension("tmp");
    {
        let mut save_data_temp_file = File::create(&save_data_temp_path).expect("path should be known to be valid and file creation should be allowed");
        save_data_temp_file.write_all(&serde_json::to_vec(&SaveDataVersioned::from(data)).expect("the file we just created should be writable")).expect("we should be able to write to the save file");
    }
    rename(save_data_temp_path, &path).expect("we should be able to rename files");
}
