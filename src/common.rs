use chrono::{NaiveDate, TimeDelta, Timelike};
use inquire::{Autocomplete, validator::{ErrorMessage, StringValidator, Validation}};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display, ops::Sub, str::FromStr};

use crate::common::error::TaskitResult;

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Categories {
    pub options: Vec<String>,
}

#[derive(Clone)]
pub struct CategoriesPair<'a, 'b>(pub &'a Categories, pub &'b Categories);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SimpleTime {
    pub hour: u8,
    pub minute: u8,
}

/// One change in the save file.
pub enum DeltaItem {
    AddCategory(String),
    RenameCategory { old: String, new: String },
    ArchiveCategory(String),
    AddEvent(Event),
    ChangeEvent { index: usize, new_event: Event },
    AddTag(String),
    TagCategory(String, String),
    SetDailyNote(NaiveDate, String),
    DeleteEvent(usize),
    /// Assumes category is already archived
    DeleteCategory(String),
    DeleteTag(String),
}

pub mod error {
    use std::{error::Error, fmt::Display, io};

    use inquire::InquireError;

    #[derive(Debug)]
    pub struct TaskitError {
        pub kind: Kind,
        pub source: Source
    }

    #[derive(Debug)]
    pub enum Kind {
        Cancelled,
        CategoryArchived(String),
        NoSuchCategory(String),
        DuplicateCategory(String),
        CategoryNotEmpty(String),
        Other(Box<dyn Error>),
    }

    /// Used in TaskitError for "error occurred while attempting to [...]"
    #[derive(Debug)]
    pub enum Source {
        CreatingTag,
        CreatingEntry,
        CreatingCategory,
        RunningStopwatch,
        SelectingEntry,
        EditingEntry,
        ArchivingCategory,
        UpdatingTag,
        EditingNote,
        UpdatingCategory,
        DrawingTui,
        SettingFilter,
        ConfirmingDelete,
        DeletingCategory,
        DeletingTag,
    }

    pub type TaskitResult<T> = Result<T, TaskitError>;

    impl Source {
        fn activity(&self) -> &'static str {
            match self {
                Source::CreatingTag => "creating a tag",
                Source::CreatingEntry => "creating an entry",
                Source::CreatingCategory => "creating a category",
                Source::RunningStopwatch => "stopwatch was running",
                Source::SelectingEntry => "selecting an entry to edit",
                Source::EditingEntry => "editing an entry",
                Source::ArchivingCategory => "archiving a category",
                Source::UpdatingTag => "updating a tag",
                Source::EditingNote => "editing a daily note",
                Source::UpdatingCategory => "updating a category",
                Source::DrawingTui => "performing TUI operations",
                Source::SettingFilter => "setting a filter",
                Source::ConfirmingDelete => "confirming deletion",
                Source::DeletingCategory => "deleting a category",
                Source::DeletingTag => "deleting a tag",
            }
        }
    }

    impl Display for TaskitError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let Self {kind, source} = self;
            let activity = source.activity();
            match kind {
                Kind::Cancelled => write!(f, "User cancelled while {activity}."),
                Kind::CategoryArchived(c) => write!(f, "Operation ({activity}) could not be completed because category {c} is archived."),
                Kind::NoSuchCategory(c) => write!(f, "While {activity}, tried to use category '{c}', which doesn't exist."),
                Kind::DuplicateCategory(c) => write!(f, "While {activity}, tried to create category '{c}', which already exists."),
                Kind::CategoryNotEmpty(c) => write!(f, "Category {c} was not empty while {activity}."),
                Kind::Other(error) => write!(f, "An external error occurred while {activity}: {error}"),
            }
        }
    }

    impl Error for TaskitError { }

    impl From<(InquireError, Source)> for TaskitError {
        fn from((value, source): (InquireError, Source)) -> Self {
            match value {
                InquireError::NotTTY => panic!("Taskit assumes it is running in an interactive terminal"),
                InquireError::InvalidConfiguration(s) => panic!("internal error: invalid configuration\n{}", s),
                InquireError::IO(error) => error.with(source).into(),
                InquireError::OperationCanceled => Self { kind: Kind::Cancelled, source },
                InquireError::OperationInterrupted => Self{ kind: Kind::Cancelled, source },
                InquireError::Custom(error) => Self { kind: Kind::Other(error), source },
            }
        }
    }

    impl From<(io::Error, Source)> for TaskitError {
        fn from((value, source): (io::Error, Source)) -> Self {
            Self { kind: Kind::Other(Box::new(value)), source }
        }
    }

    /// attaches source data to a given error
    pub trait With<T>: Sized {
        type Joined;
        fn with(self, source: T) -> Self::Joined;
    }

    impl With<Source> for io::Error {
        type Joined = (Self, Source);

        fn with(self, source: Source) -> Self::Joined {
            (self, source)
        }
    }

    impl With<Source> for InquireError {
        type Joined = (Self, Source);

        fn with(self, source: Source) -> Self::Joined {
            (self, source)
        }
    }

    impl With<Source> for Kind {
        type Joined = TaskitError;

        fn with(self, source: Source) -> Self::Joined {
            TaskitError { kind: self, source }
        }
    }

    impl<T, E> With<Source> for Result<T, E>
    where E: With<Source>
    {
        type Joined = Result<T, E::Joined>;

        fn with(self, source: Source) -> Self::Joined {
            self.map_err(|e| e.with(source))
        }
    }
}

#[derive(Clone)]
pub struct TagCompleter<'a>(pub &'a [String]);

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
            let (hour, minute) = if let Some(idx) = input.find(':') {
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

pub trait Apply<T> {
    fn apply(&mut self, delta: T) -> TaskitResult<()>;
}

impl Apply<DeltaItem> for SaveData {
    fn apply(&mut self, delta: DeltaItem) -> TaskitResult<()> {
        match delta {
            DeltaItem::AddCategory(category) => {
                                                if !self.categories.options.contains(&category) {
                                                    self.categories.options.push(category);
                                                }
                                            }
            DeltaItem::RenameCategory { old, new } => {
                        self.categories.options.iter_mut().find(|c| c == &&old).map(|c| *c = new.clone());
                        self.archived_categories.options.iter_mut().find(|c| c == &&old).map(|c| *c = new.clone());
                        self.events.iter_mut().for_each(|ev| {if ev.category == old {ev.category = new.clone();}});
                        self.tag_map.remove(&old).and_then(|v| self.tag_map.insert(new, v));
                    },
            DeltaItem::AddEvent(event) => self.events.push(event),
            DeltaItem::ChangeEvent { index, new_event } => self.events[index] = new_event,
            DeltaItem::ArchiveCategory(category) => {
                                        self.tag_map.remove(&category);
                                        self.categories.options.retain(|x| *x != category);
                                        self.archived_categories.options.push(category);
                                    },
            DeltaItem::AddTag(tag) => if !self.tags.contains(&tag) { self.tags.push(tag); },
            DeltaItem::TagCategory(category, tag) => {
                                if !self.tag_map.contains_key(&category) {
                                    self.tag_map.insert(category.clone(), vec![]);
                                }
                                if !self.tag_map[&category].contains(&tag) {
                                    if let Some(tags) = self.tag_map.get_mut(&category) { tags.push(tag); }
                                } 
                            },
            DeltaItem::SetDailyNote(date, note) => {self.daily_notes.insert(date, note);},
            DeltaItem::DeleteEvent(index) => {self.events.remove(index);},
            DeltaItem::DeleteCategory(c) => self.archived_categories.options.retain(|x| x != &c),
            DeltaItem::DeleteTag(t) => {
                self.tags.retain(|x| x != &t);
                self.tag_map.iter_mut().for_each(|(_, v)| v.retain(|x| x != &t));
                self.events.iter_mut().for_each(|ev| ev.tags.retain(|x| x != &t));
            },
        }
        Ok(())
    }
}

impl Apply<Vec<DeltaItem>> for SaveData {
    fn apply(&mut self, delta: Vec<DeltaItem>) -> TaskitResult<()> {
        for delta in delta {
            self.apply(delta)?;
        }
        Ok(())
    }
}

impl<'a, 'b> Autocomplete for CategoriesPair<'a, 'b> {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        Ok(self.0
            .options
            .iter()
            .chain(self.1.options.iter())
            .filter(|s| s.starts_with(input))
            .cloned()
            .collect()
        )
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>
    ) -> Result<inquire::autocompletion::Replacement, inquire::CustomUserError> {
        let suggestions = self
            .get_suggestions(input)
            .expect("get_suggestions only returns Ok");
        Ok(highlighted_suggestion.or_else(|| suggestions.into_iter().next()))
    }
}

impl Autocomplete for &Categories {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        Ok(self
            .options
            .iter()
            .filter(|s| s.starts_with(input))
            .cloned()
            .collect())
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, inquire::CustomUserError> {
        let suggestions = self
            .get_suggestions(input)
            .expect("get_suggestions only returns Ok");
        Ok(highlighted_suggestion.or_else(|| suggestions.into_iter().next()))
    }
}

impl<'a> Autocomplete for TagCompleter<'a> {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        let input = if input.starts_with('#') { &input[1..] } else { input };
        Ok(self
            .0
            .iter()
            .filter(|s| s.starts_with(input))
            .cloned()
            .map(|mut s| { s.insert(0, '#'); s })
            .collect())
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, inquire::CustomUserError> {
        let suggestions = self
            .get_suggestions(input)
            .expect("get_suggestions only returns Ok");
        Ok(highlighted_suggestion.or_else(|| suggestions.into_iter().next()))
    }
}

impl<'a, 'b> StringValidator for CategoriesPair<'a, 'b> {
    fn validate(&self, input: &str) -> Result<inquire::validator::Validation, inquire::CustomUserError> {
        if self.0.options.iter().chain(self.1.options.iter()).find(|cat| cat.as_str() == input).is_some() {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid(ErrorMessage::Default))
        }
    }
}

impl StringValidator for &Categories {
    fn validate(&self, input: &str) -> Result<inquire::validator::Validation, inquire::CustomUserError> {
        if self.options.contains(&input.to_owned()) {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid(ErrorMessage::Default))
        }
    }
}

impl<'a> StringValidator for TagCompleter<'a> {
    fn validate(&self, input: &str) -> Result<Validation, inquire::CustomUserError> {
        let tag = if input.starts_with('#') { &input[1..] } else { input };
        if self.0.contains(&tag.to_owned()) {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid(ErrorMessage::Default))
        }
    }
}

impl From<chrono::NaiveTime> for SimpleTime {
    fn from(value: chrono::NaiveTime) -> Self {
        Self {
            hour: value.hour() as u8,
            minute: value.minute() as u8,
        }
    }
}

impl TryFrom<SimpleTime> for chrono::NaiveTime {
    type Error = ();
    fn try_from(value: SimpleTime) -> Result<Self, ()> {
        chrono::NaiveTime::from_hms_opt(value.hour as u32, value.minute as u32, 0).ok_or(())
    }
}

impl Display for SimpleTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

// Note that this implementation is slightly unusual in that e.g. 01:00 - 23:00 = 2 hr
impl Sub for SimpleTime {
    type Output = TimeDelta;

    fn sub(self, rhs: Self) -> Self::Output {
        let lhs_minute = (self.hour as i64) * 60 + self.minute as i64;
        let rhs_minute = (rhs.hour as i64) * 60 + rhs.minute as i64;
        let mut minutes = lhs_minute - rhs_minute;
        if minutes < 0 {
            minutes += 60 * 24;
        }
        return TimeDelta::minutes(minutes);
    }
}

trait Upgrade {
    type Next;
    fn upgrade(self) -> Self::Next;
}

// =================================== VERSIONING WORK ===================================
//               When SaveData versioning changes, update everything here

// Version Update Tasks:
//   - Define new version
//   - Add variant to SaveDataVersioned
//   - Update Default impl
//   - Update SaveData type alias
//   - Update line 1 of SaveDataVersioned::extract
//   - Update as_latest, outdated, upgrade_once
//   - impl From for new version
//   - impl Upgrade for previous version
//   - if Event is updated, update its Display impl

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV1 {
    pub categories: Categories,
    pub events: Vec<EventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV2 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub events: Vec<EventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV3 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub tags: Vec<String>,
    // Maps from category name to tags
    pub tag_map: HashMap<String, Vec<String>>,
    pub events: Vec<EventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV4 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub tags: Vec<String>,
    // Maps from category name to tags
    pub tag_map: HashMap<String, Vec<String>>,
    pub events: Vec<EventV1>,
    pub daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV5 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub tags: Vec<String>,
    /// Maps from category name to tags
    pub tag_map: HashMap<String, Vec<String>>,
    pub events: Vec<EventV5>,
    pub daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV6 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub tags: Vec<String>,
    /// Maps from category name to tags
    pub tag_map: HashMap<String, Vec<String>>,
    pub events: Vec<EventV5>,
    pub daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SaveDataVersioned {
    V1(SaveDataV1),
    V2(SaveDataV2),
    V3(SaveDataV3),
    V4(SaveDataV4),
    V5(SaveDataV5),
    V6(SaveDataV6),
}

impl Default for SaveDataVersioned {
    fn default() -> Self {
        Self::V6(Default::default())
    }
}

pub type SaveData = SaveDataV6;

impl SaveDataVersioned {
    /// Returns the latest version of SaveData, and a bool that is true iff the format was upgraded
    pub fn extract(mut self) -> (SaveData, bool) {
        if let Self::V6(data) = self {
            (data, false)
        } else {
            while self.outdated() {
                self = self.upgrade_once();
            }
            // panic safety: as_latest only panics if self.outdated(), which is guaranteed false
            (self.as_latest(), true)
        }
    }

    fn as_latest(self) -> SaveData {
        match self {
            Self::V6(data) => data,
            _ => panic!()
        }
    }

    fn outdated(&self) -> bool {
        if let Self::V6(_) = self { false } else { true }
    }

    fn upgrade_once(self) -> Self {
        match self {
            Self::V1(data) => data.upgrade().into(),
            Self::V2(data) => data.upgrade().into(),
            Self::V3(data) => data.upgrade().into(),
            Self::V4(data) => data.upgrade().into(),
            Self::V5(data) => data.upgrade().into(),
            Self::V6(_) => panic!(),
        }
    }
}

impl From<SaveDataV1> for SaveDataVersioned {
    fn from(value: SaveDataV1) -> Self {
        Self::V1(value)
    }
}

impl From<SaveDataV2> for SaveDataVersioned {
    fn from(value: SaveDataV2) -> Self {
        Self::V2(value)
    }
}

impl From<SaveDataV3> for SaveDataVersioned {
    fn from(value: SaveDataV3) -> Self {
        Self::V3(value)
    }
}

impl From<SaveDataV4> for SaveDataVersioned {
    fn from(value: SaveDataV4) -> Self {
        Self::V4(value)
    }
}

impl From<SaveDataV5> for SaveDataVersioned {
    fn from(value: SaveDataV5) -> Self {
        Self::V5(value)
    }
}

impl From<SaveDataV6> for SaveDataVersioned {
    fn from(value: SaveDataV6) -> Self {
        Self::V6(value)
    }
}


impl Upgrade for SaveDataV1 {
    type Next = SaveDataV2;
    fn upgrade(self) -> Self::Next {
        SaveDataV2 {
            categories: self.categories,
            archived_categories: Default::default(),
            events: self.events
        }
    }
}

impl Upgrade for SaveDataV2 {
    type Next = SaveDataV3;
    fn upgrade(self) -> Self::Next {
        SaveDataV3 {
            categories: self.categories,
            archived_categories: self.archived_categories,
            tags: Default::default(),
            tag_map: Default::default(),
            events: self.events,
        }
    }
}

impl Upgrade for SaveDataV3 {
    type Next = SaveDataV4;
    fn upgrade(self) -> Self::Next {
        SaveDataV4 {
            categories: self.categories,
            archived_categories: self.archived_categories,
            tags: self.tags,
            tag_map: self.tag_map,
            events: self.events,
            daily_notes: Default::default(),
        }
    }
}

impl Upgrade for SaveDataV4 {
    type Next = SaveDataV5;
    fn upgrade(self) -> Self::Next {
        let tags = self.tags.clone();
        SaveDataV5 {
            categories: self.categories,
            archived_categories: self.archived_categories,
            tags: self.tags,
            tag_map: self.tag_map,
            events: self.events
                .into_iter()
                .map(|EventV1 { start_time, end_time, date, category, comments }| 
                    EventV5 {
                        start_time,
                        end_time,
                        date,
                        category,
                        description: comments.clone(),
                        tags: comments
                            .split(' ')
                            .filter(|s| s.starts_with('#'))
                            .filter(|s| tags.iter().find(|tag| tag.as_str() == &s[1..]).is_some())
                            .map(|s| s[1..].to_owned())
                            .collect()
                    }
                ).collect(),
            daily_notes: self.daily_notes,
        }
    }
}

impl Upgrade for SaveDataV5 {
    type Next = SaveDataV6;
    fn upgrade(self) -> Self::Next {
        let SaveDataV5 { categories, archived_categories, tags, mut tag_map, events, daily_notes } = self;
        archived_categories.options.iter().for_each(|c| {tag_map.remove(c);});
        SaveDataV6 { categories, archived_categories, tags, tag_map, events, daily_notes }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventV1 {
    pub start_time: SimpleTime,
    pub end_time: SimpleTime, // if end_time before start_time: counts as that time on date + 1
    pub date: NaiveDate,
    pub category: String,
    pub comments: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventV5 {
    pub start_time: SimpleTime,
    pub end_time: SimpleTime, // if end_time before start_time: counts as that time on date + 1
    pub date: NaiveDate,
    pub category: String,
    #[serde(rename = "comments")]
    pub description: String,
    pub tags: Vec<String>,
}

pub type Event = EventV5;

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({}, {}-{})", self.category, self.description, self.date, self.start_time, self.end_time)
    }
}

// ================================= END VERSIONING WORK =================================
