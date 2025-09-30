use std::{fs::{create_dir_all, rename, File}, io::{Read, Write}, str::FromStr};

use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use inquire::{validator::{ErrorMessage, StringValidator, Validation}, Autocomplete, Confirm, CustomType, DateSelect, Text};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
enum SaveDataVersioned {
    V1(SaveDataV1),
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct SaveDataV1 {
    categories: Categories,
    events: Vec<Event>,
}

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
struct Categories {
    options: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Event {
    start_time: SimpleTime,
    end_time: SimpleTime, // if end_time before start_time: counts as that time on date + 1
    date: NaiveDate,
    category: String,
    comments: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct SimpleTime {
    hour: u8,
    minute: u8,
}

#[derive(clap::Parser, Debug)]
struct CliArgs {
    #[command(subcommand)]
    command: Option<CliSubcommands>,
}

#[derive(Subcommand, Debug)]
enum CliSubcommands {
    Record,
    Stopwatch,
}

#[derive(Clone)]
struct TimeValidator;

impl Autocomplete for &Categories {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        Ok(self.options.iter().filter(|s| s.starts_with(input)).cloned().collect())
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, inquire::CustomUserError> {
        let suggestions = self.get_suggestions(input).expect("get_suggestions only returns Ok");
        Ok(highlighted_suggestion.or_else(|| suggestions.into_iter().next()))
    }
}

impl SimpleTime {
    pub fn try_new(hour: u8, minute: u8) -> Option<Self> {
        if hour < 24 && minute < 60 {
            Some(Self { hour, minute })
        } else {
            None
        }
    }
}

impl FromStr for SimpleTime {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn get_time_unchecked(input: &str) -> Option<(u8, u8)> {
            let (hour, minute) = 
                if let Some(idx) = input.find(':') {
                    // time with colon
                    let (hour, minute) = input.split_at(idx);
                    (hour, &minute[1..])
                } else if input.len() == 4 {
                    // time without colon
                    let (hour, minute) = input.split_at(2);
                    (hour, minute)
                } else {
                    // not long enough regardless
                    return None;
                };
            Some((hour.parse().ok()?, minute.parse().ok()?))
        }

        let (hour, minute) = get_time_unchecked(s).ok_or(())?;
        Self::try_new(hour, minute).ok_or(())
    }
}

impl ToString for SimpleTime {
    fn to_string(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

impl StringValidator for TimeValidator {
    fn validate(&self, input: &str) -> Result<Validation, inquire::CustomUserError> {
        if input.parse::<SimpleTime>().is_ok() {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid(ErrorMessage::Default))
        }
    }
}

// =================================== VERSIONING WORK ===================================
//               When SaveData versioning changes, update everything here

type SaveData = SaveDataV1;

impl SaveDataVersioned {
    /// Returns the latest version of SaveData, and a bool that is true iff the format was upgraded
    fn upgrade(self) -> (SaveData, bool) {
        match self {
            SaveDataVersioned::V1(data_v1) => (data_v1, false),
        }
    }
}

impl Default for SaveDataVersioned {
    fn default() -> Self {
        Self::V1(Default::default())
    }
}

impl From<SaveData> for SaveDataVersioned {
    fn from(value: SaveData) -> Self {
        Self::V1(value)
    }
}
// ================================= END VERSIONING WORK =================================

fn main() {
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
    let save_data = {
        let mut save_data = String::new(); 
        if let Ok(mut save_data_file) = File::open(&save_data_file_path)
        {
            save_data_file.read_to_string(&mut save_data).unwrap();
            serde_json::from_str::<SaveDataVersioned>(&save_data).unwrap().upgrade()
        } else {
            Default::default()
        }
    };
    let cli_args = CliArgs::parse();
    let save_data = match cli_args.command {
        Some(CliSubcommands::Record) => record_main(save_data),
        Some(CliSubcommands::Stopwatch) => stopwatch_main(save_data),
        None => todo!(),
    };
    if let Some(save_data) = save_data {
        let save_data = SaveDataVersioned::from(save_data);
        let write_save_data_path = save_data_file_path.with_file_name("new_save.json");
        let mut save_data_file = File::create(&write_save_data_path).unwrap();
        save_data_file.write_all(&serde_json::to_vec(&save_data).unwrap()).unwrap();
        rename(&write_save_data_path, &save_data_file_path).unwrap();
    }
}

fn record_main(mut save_data: SaveData) -> Option<SaveData> {
    let date = DateSelect::new("Date:").prompt().unwrap();
    let start_time = CustomType::<SimpleTime>::new("Start time:").prompt().unwrap();
    let category = Text::new("Select a category:").with_autocomplete(&save_data.categories).prompt().unwrap();
    let comments = Text::new("Notes:").prompt().unwrap();
    let end_time = CustomType::<SimpleTime>::new("End time:").prompt().unwrap();
    if !save_data.categories.options.contains(&category) {
        let create = Confirm::new(&format!("Category {category} does not currently exist. Create it?")).prompt().unwrap();
        if create {
            save_data.categories.options.push(category.clone());
        } else {
            println!("Cannot create event with nonexistent category.");
            return record_main(save_data);
        }
    }
    save_data.events.push(Event { start_time, end_time, date, category, comments });
    Some(save_data)
}

fn stopwatch_main(mut save_data: SaveData) -> Option<SaveData> {
    todo!()
}
