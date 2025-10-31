use chrono::{NaiveDate, TimeDelta, Timelike};
use inquire::{validator::{ErrorMessage, StringValidator, Validation}, Autocomplete};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fmt::Display, ops::Sub, str::FromStr};

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Categories {
    pub options: Vec<String>,
}

#[derive(Clone)]
pub struct CategoriesPair<'a, 'b>(pub &'a Categories, pub &'b Categories);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub start_time: SimpleTime,
    pub end_time: SimpleTime, // if end_time before start_time: counts as that time on date + 1
    pub date: NaiveDate,
    pub category: String,
    pub comments: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SimpleTime {
    pub hour: u8,
    pub minute: u8,
}

// One change in the save file.
pub enum DeltaItem {
    AddCategory(String),
    RenameCategory { old: String, new: String },
    ArchiveCategory(String),
    AddEvent(Event),
    ChangeEvent { index: usize, new_event: Event },
    AddTag(String),
    TagCategory(String, String),
    SetDailyNote(NaiveDate, String),
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
    fn apply(&mut self, delta: T) -> Result<(), Box<dyn Error>>;
}

impl Apply<DeltaItem> for SaveData {
    fn apply(&mut self, delta: DeltaItem) -> Result<(), Box<dyn Error>> {
        match delta {
            DeltaItem::AddCategory(category) => {
                                        if !self.categories.options.contains(&category) {
                                            self.categories.options.push(category);
                                        }
                                    }
            DeltaItem::RenameCategory { old, new } => todo!(),
            DeltaItem::AddEvent(event) => self.events.push(event),
            DeltaItem::ChangeEvent { index, new_event } => self.events[index] = new_event,
            DeltaItem::ArchiveCategory(category) => {
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
        }
        Ok(())
    }
}

impl Apply<Vec<DeltaItem>> for SaveData {
    fn apply(&mut self, delta: Vec<DeltaItem>) -> Result<(), Box<dyn Error>> {
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

impl From<chrono::NaiveTime> for SimpleTime {
    fn from(value: chrono::NaiveTime) -> Self {
        Self {
            hour: value.hour() as u8,
            minute: value.minute() as u8,
        }
    }
}

impl From<SimpleTime> for chrono::NaiveTime {
    fn from(value: SimpleTime) -> Self {
        chrono::NaiveTime::from_hms_opt(value.hour as u32, value.minute as u32, 0).unwrap()
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
//   + Define new version
//   + Add variant to SaveDataVersioned
//   + Update Default impl
//   + Update SaveData type alias
//   + Update lines 1 and 2 of SaveDataVersioned::extract
//   + Update as_latest, outdated, upgrade_once
//   + impl From for new version
//   + impl Upgrade for previous version

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV1 {
    pub categories: Categories,
    pub events: Vec<Event>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV2 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub events: Vec<Event>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV3 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub tags: Vec<String>,
    // Maps from category name to tags
    pub tag_map: HashMap<String, Vec<String>>,
    pub events: Vec<Event>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct SaveDataV4 {
    pub categories: Categories,
    pub archived_categories: Categories,
    pub tags: Vec<String>,
    // Maps from category name to tags
    pub tag_map: HashMap<String, Vec<String>>,
    pub events: Vec<Event>,
    pub daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SaveDataVersioned {
    V1(SaveDataV1),
    V2(SaveDataV2),
    V3(SaveDataV3),
    V4(SaveDataV4),
}

impl Default for SaveDataVersioned {
    fn default() -> Self {
        Self::V4(Default::default())
    }
}

pub type SaveData = SaveDataV4;

impl SaveDataVersioned {
    /// Returns the latest version of SaveData, and a bool that is true iff the format was upgraded
    pub fn extract(mut self) -> (SaveData, bool) {
        if let Self::V4(data) = self {
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
            Self::V4(data) => data,
            _ => panic!()
        }
    }

    fn outdated(&self) -> bool {
        if let Self::V4(_) = self { false } else { true }
    }

    fn upgrade_once(self) -> Self {
        match self {
            Self::V1(data) => data.upgrade().into(),
            Self::V2(data) => data.upgrade().into(),
            Self::V3(data) => data.upgrade().into(),
            Self::V4(_) => panic!(),
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

// ================================= END VERSIONING WORK =================================
