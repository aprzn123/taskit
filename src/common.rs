pub mod invariants;
pub mod error;

use chrono::{NaiveDate, TimeDelta, Timelike};
use inquire::{
    Autocomplete,
    validator::{ErrorMessage, StringValidator, Validation},
};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, fmt::Display, ops::Sub, str::FromStr};

use error::TaskitResult;

use crate::util::SetVec;

pub use invariants::SaveData;

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
struct Categories {
    options: Vec<String>,
}

#[derive(Clone)]
pub struct CategoriesCompleter<'a>(pub &'a SetVec<String>);

#[derive(Clone)]
pub struct CategoriesPair<'a, 'b>(pub &'a SetVec<String>, pub &'b SetVec<String>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SimpleTime {
    pub hour: u8,
    pub minute: u8,
}

/// One change in the save file.
#[derive(Debug)]
pub enum DeltaItem {
    AddCategory(String),
    RenameCategory {
        old: String,
        new: String,
    },
    ArchiveCategory(String),
    AddEvent(Event),
    ChangeEvent {
        index: usize,
        new_event: Event,
    },
    AddTag(String),
    /// category, tag
    TagCategory(String, String),
    /// category, tag
    UntagCategory(String, String),
    SetDailyNote(NaiveDate, String),
    DeleteEvent(usize),
    /// Assumes category is already archived
    DeleteCategory(String),
    DeleteTag(String),
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
                assert!(self.categories.push(category).is_ok());
            }
            DeltaItem::RenameCategory { old, new } => {
                assert!(
                    (self.categories.contains(&old)
                        && !self.categories.contains(&new))
                        || (self.archived_categories.contains(&old)
                            && !self.archived_categories.contains(&new))
                );
                if self.categories.remove(&old).is_some() {
                    self.categories.push(new.clone()).expect("category name must be previously uninhabited");
                }
                if self.archived_categories.remove(&old).is_some() {
                    self.archived_categories.push(new.clone()).expect("archived category name must be previously uninhabited");
                }
                self.events.iter_mut().for_each(|ev| {
                    if ev.category == old {
                        ev.category = new.clone();
                    }
                });
                self.tag_map
                    .remove(&old)
                    .and_then(|v| self.tag_map.insert(new, v));
            }
            DeltaItem::AddEvent(event) => {
                assert!(self.categories.contains(&event.category));
                assert!(event.tags.iter().all(|tag| self.tags.contains(tag)));
                self.events.push(event);
            }
            DeltaItem::ChangeEvent { index, new_event } => {
                assert!(index < self.events.len());
                self.events[index] = new_event;
            }
            DeltaItem::ArchiveCategory(category) => {
                self.tag_map.remove(&category);
                assert!(self.categories.remove(&category).is_some());
                assert!(self.archived_categories.push(category).is_ok());
            }
            DeltaItem::AddTag(tag) => {
                assert!(self.tags.push(tag).is_ok());
            }
            DeltaItem::TagCategory(category, tag) => {
                if !self.tag_map.contains_key(&category) {
                    self.tag_map.insert(category.clone(), HashSet::new());
                }
                if !self.tag_map[&category].contains(&tag) && let Some(tags) = self.tag_map.get_mut(&category) {
                    tags.insert(tag);
                }
            }
            DeltaItem::UntagCategory(category, tag) => {
                if let Some(tags) = self.tag_map.get_mut(&category) {
                    tags.retain(|t| t != &tag)
                }
            }
            DeltaItem::SetDailyNote(date, note) => {
                self.daily_notes.insert(date, note);
            }
            DeltaItem::DeleteEvent(index) => {
                assert!(self.events.len() > index);
                self.events.remove(index);
            }
            DeltaItem::DeleteCategory(c) => self.archived_categories.retain(|x| x != &c),
            DeltaItem::DeleteTag(t) => {
                assert!(self.tags.contains(&t));
                self.tags.retain(|x| x != &t);
                self.tag_map
                    .iter_mut()
                    .for_each(|(_, v)| v.retain(|x| x != &t));
                self.events
                    .iter_mut()
                    .for_each(|ev| ev.tags.retain(|x| x != &t));
            }
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
        Ok(self
            .0
            .iter()
            .chain(self.1.iter())
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

impl<'a> Autocomplete for CategoriesCompleter<'a> {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        Ok(self
            .0
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
        let input = input.strip_prefix('#').unwrap_or(input);
        Ok(self
            .0
            .iter()
            .filter(|s| s.starts_with(input))
            .cloned()
            .map(|mut s| {
                s.insert(0, '#');
                s
            })
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
    fn validate(
        &self,
        input: &str,
    ) -> Result<inquire::validator::Validation, inquire::CustomUserError> {
        if self
            .0
            .iter()
            .chain(self.1.iter())
            .any(|cat| cat.as_str() == input)
        {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid(ErrorMessage::Default))
        }
    }
}

impl<'a> StringValidator for CategoriesCompleter<'a> {
    fn validate(
        &self,
        input: &str,
    ) -> Result<inquire::validator::Validation, inquire::CustomUserError> {
        if self.0.contains(&input.to_owned()) {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid(ErrorMessage::Default))
        }
    }
}

impl<'a> StringValidator for TagCompleter<'a> {
    fn validate(&self, input: &str) -> Result<Validation, inquire::CustomUserError> {
        let tag = input.strip_prefix('#').unwrap_or(input);
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
        TimeDelta::minutes(minutes)
    }
}

trait Upgrade {
    type Next;
    fn upgrade(self) -> Self::Next;
}


// =================================== VERSIONING WORK ===================================
//               When SaveData versioning changes, update everything here

// Version Update Tasks: (* means it requires meaningful work, - means it's automatable. parentheses
// around things that will not be macro'd)
//   * Write new UnverifiedSaveDataV[x] struct
//   * impl Upgrade from previous version to new version
//   - update UnverifiedSaveDataLatest typedef
//   - Add to UnverifiedSaveDataVersioned
//   - Update Default implementaion
//   - update From<Latest> impl to reference latest version
//   - update extract and upgrade_once to reference latest version
//
// potential syntax:
// versioned_structs!{
// UnverifiedSaveData {
//   Version(1) {
//     struct contents
//   }
//
//   Version(2) {
//     struct contents
//   }
//   
//   Upgrade(2) {
//     <function body>
//   }
// }
// }
//

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV1 {
    categories: Categories,
    events: Vec<EventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV2 {
    categories: Categories,
    archived_categories: Categories,
    events: Vec<EventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV3 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    // Maps from category name to tags
    tag_map: HashMap<String, Vec<String>>,
    events: Vec<EventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV4 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    // Maps from category name to tags
    tag_map: HashMap<String, Vec<String>>,
    events: Vec<EventV1>,
    daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV5 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    /// Maps from category name to tags
    tag_map: HashMap<String, Vec<String>>,
    events: Vec<EventV5>,
    daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV6 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    /// Maps from category name to tags
    tag_map: HashMap<String, HashSet<String>>,
    events: Vec<EventV5>,
    daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV7 {
    categories: Vec<String>,
    archived_categories: Vec<String>,
    tags: Vec<String>,
    /// Maps from category name to tags
    tag_map: HashMap<String, HashSet<String>>,
    events: Vec<EventV5>,
    daily_notes: HashMap<NaiveDate, String>,
}

pub type UnverifiedSaveDataLatest = UnverifiedSaveDataV7;

#[derive(Serialize, Deserialize, Debug)]
pub enum UnverifiedSaveDataVersioned {
    V1(UnverifiedSaveDataV1),
    V2(UnverifiedSaveDataV2),
    V3(UnverifiedSaveDataV3),
    V4(UnverifiedSaveDataV4),
    V5(UnverifiedSaveDataV5),
    V6(UnverifiedSaveDataV6),
    V7(UnverifiedSaveDataV7),
}

impl Default for UnverifiedSaveDataVersioned {
    fn default() -> Self {
        Self::V7(Default::default())
    }
}

impl From<UnverifiedSaveDataLatest> for UnverifiedSaveDataVersioned {
    fn from(value: UnverifiedSaveDataLatest) -> Self {
        Self::V7(value)
    }
}

impl UnverifiedSaveDataVersioned {
    /// Returns the latest version of SaveData, and a bool that is true iff the format was upgraded
    pub fn extract(self) -> (UnverifiedSaveDataLatest, bool) {
        if let Self::V7(data) = self {
            (data, false)
        } else {
            (self.upgrade_once().extract().0, true)
        }
    }

    fn upgrade_once(self) -> Self {
        match self {
            Self::V1(data) => Self::V2(data.upgrade()),
            Self::V2(data) => Self::V3(data.upgrade()),
            Self::V3(data) => Self::V4(data.upgrade()),
            Self::V4(data) => Self::V5(data.upgrade()),
            Self::V5(data) => Self::V6(data.upgrade()),
            Self::V6(data) => Self::V7(data.upgrade()),
            Self::V7(_) => panic!(),
        }
    }
}

impl Upgrade for UnverifiedSaveDataV1 {
    type Next = UnverifiedSaveDataV2;
    fn upgrade(self) -> Self::Next {
        UnverifiedSaveDataV2 {
            categories: self.categories,
            archived_categories: Default::default(),
            events: self.events,
        }
    }
}

impl Upgrade for UnverifiedSaveDataV2 {
    type Next = UnverifiedSaveDataV3;
    fn upgrade(self) -> Self::Next {
        UnverifiedSaveDataV3 {
            categories: self.categories,
            archived_categories: self.archived_categories,
            tags: Default::default(),
            tag_map: Default::default(),
            events: self.events,
        }
    }
}

impl Upgrade for UnverifiedSaveDataV3 {
    type Next = UnverifiedSaveDataV4;
    fn upgrade(self) -> Self::Next {
        UnverifiedSaveDataV4 {
            categories: self.categories,
            archived_categories: self.archived_categories,
            tags: self.tags,
            tag_map: self.tag_map,
            events: self.events,
            daily_notes: Default::default(),
        }
    }
}

impl Upgrade for UnverifiedSaveDataV4 {
    type Next = UnverifiedSaveDataV5;
    fn upgrade(self) -> Self::Next {
        let tags = self.tags.clone();
        UnverifiedSaveDataV5 {
            categories: self.categories,
            archived_categories: self.archived_categories,
            tags: self.tags,
            tag_map: self.tag_map,
            events: self
                .events
                .into_iter()
                .map(
                    |EventV1 {
                         start_time,
                         end_time,
                         date,
                         category,
                         comments,
                     }| EventV5 {
                        start_time,
                        end_time,
                        date,
                        category,
                        description: comments.clone(),
                        tags: comments
                            .split(' ')
                            .filter(|s| s.starts_with('#'))
                            .filter(|s| tags.iter().any(|tag| tag.as_str() == &s[1..]))
                            .map(|s| s[1..].to_owned())
                            .collect(),
                    },
                )
                .collect(),
            daily_notes: self.daily_notes,
        }
    }
}

impl Upgrade for UnverifiedSaveDataV5 {
    type Next = UnverifiedSaveDataV6;
    fn upgrade(self) -> Self::Next {
        let UnverifiedSaveDataV5 {
            categories,
            archived_categories,
            tags,
            mut tag_map,
            events,
            daily_notes,
        } = self;
        archived_categories.options.iter().for_each(|c| {
            tag_map.remove(c);
        });
        UnverifiedSaveDataV6 {
            categories,
            archived_categories,
            tags,
            tag_map: tag_map.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect(),
            events,
            daily_notes,
        }
    }
}

impl Upgrade for UnverifiedSaveDataV6 {
    type Next = UnverifiedSaveDataV7;
    fn upgrade(self) -> Self::Next {
        let UnverifiedSaveDataV6 {
            categories, 
            archived_categories, 
            tags, 
            tag_map, 
            events, 
            daily_notes 
        } = self;
        let categories = categories.options.into_iter().collect();
        let archived_categories = archived_categories.options.into_iter().collect();
        UnverifiedSaveDataV7 {
            categories,
            archived_categories,
            tags,
            tag_map,
            events,
            daily_notes,
        }
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
    pub tags: HashSet<String>,
}

pub type Event = EventV5;

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} ({}, {}-{})",
            self.category, self.description, self.date, self.start_time, self.end_time
        )
    }
}

// ================================= END VERSIONING WORK =================================
