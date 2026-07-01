pub mod invariants;
pub mod error;

use chrono::{DateTime, Local, NaiveDate, TimeDelta, Timelike};
use inquire::{
    Autocomplete,
    validator::{ErrorMessage, StringValidator, Validation},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use toml::Table;
use std::{cell::LazyCell, collections::{HashMap, HashSet}, fmt::Display, ops::Sub, str::FromStr};

use error::TaskitResult;

use crate::{common::{config::CONFIG, invariants::{Category, Opaque, Tag}}, util::SetVec};

pub use invariants::{SaveData, Event};


pub mod config {
    use std::sync::{LazyLock, OnceLock};

    use serde::Deserialize;

    pub static CONFIG_WRITE: OnceLock<Config> = OnceLock::new();
    // stupid fucking hack so i don't have to unwrap every time i query CONFIG
    pub static CONFIG: LazyLock<&Config> = LazyLock::new(|| CONFIG_WRITE.get().unwrap());

    #[derive(Deserialize, Default, Debug)]
    pub struct Config {
        #[serde(rename = "preferences")]
        pub prefs: Preferences
    }

    #[derive(Deserialize, Default, Debug)]
    pub struct Preferences {
        #[serde(default)]
        pub use_12hr_time: bool,
        /// When 12hr time is enabled and neither AM nor PM is specified for a time, when this is
        /// set to false (the default) the input won't be accepted. When it's set to true, we select
        /// AM or PM based on whichever one is closer to the current time.
        /// When 12hr time is disabled, this does nothing
        #[serde(default)]
        pub guess_am_pm: bool,
    }
}

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
struct Categories {
    options: Vec<String>,
}

#[derive(Clone)]
pub struct CategoriesCompleter<'a>(pub &'a SetVec<Category>);

#[derive(Clone)]
pub struct CategoriesPair<'a, 'b>(pub &'a [Category], pub &'b [Category]);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SimpleTime {
    pub hour: u8,
    pub minute: u8,
}

/// Represents one change in the save file. Note the use of Opaque, which is a private type used to
/// prevent manual construction of certain variants. For these variants, use the functions with
/// corresponding names in common::invariants instead. 
///
/// The private variants are those which can generate new categories or tags. For these variants, we
/// want construction of the variant to be *inherently* tied to construction of a new Category or
/// Tag object, which we achieve by only constructing those variants through a function that also
/// constructs a category/tag
#[allow(private_interfaces)]
#[derive(Debug)]
pub enum DeltaItem {
    AddCategory(Opaque<Category>),
    RenameCategory {
        old: Category,
        new: Opaque<Category>,
    },
    ArchiveCategory(Category),
    AddEvent(Event),
    ChangeEvent {
        index: usize,
        new_event: Event,
    },
    AddTag(Opaque<Tag>),
    /// category, tag
    TagCategory(Category, Tag),
    /// category, tag
    UntagCategory(Category, Tag),
    SetDailyNote(NaiveDate, String),
    DeleteEvent(usize),
    /// Assumes category is already archived
    DeleteCategory(Category),
    DeleteTag(Tag),
}

#[derive(Clone)]
pub struct TagCompleter<'a>(pub &'a SetVec<Tag>);

impl SimpleTime {
    pub fn try_new(hour: u8, minute: u8) -> Option<Self> {
        if hour < 24 && minute < 60 {
            Some(Self { hour, minute })
        } else {
            None
        }
    }

    pub fn try_new_12hr(hour: u8, minute: u8, pm: bool) -> Option<Self> {
        let hour = if hour == 0 { return None }
                   else if hour == 12 { 0 }
                   else { hour };
        if hour < 12 && minute < 60 {
            Some(Self { hour: hour + if pm {12} else {0}, minute })
        } else {
            None
        }
    }
    
    pub fn now() -> Self {
        let now = Local::now();
        Self {
            hour: now.hour() as u8,
            minute: now.minute() as u8,
        }
    }
}

impl FromStr for SimpleTime {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // is this actually any better than just using lazylock
        thread_local! {
            static RE: LazyCell<Regex> = LazyCell::new(|| Regex::new(r"^(?:(?<hour>\d\d?):(?<minute>\d\d)|(?<hour2>\d\d?)(?<minute2>\d\d))\s*(?:(?<ap>[apAP])[mM]?)?$").expect("regex should compile"));
        }
        let captures = RE.with(|re| re.captures(s)).ok_or(())?;
        let hour = captures.name("hour").or_else(|| captures.name("hour2")).ok_or(())?.as_str().parse().ok().ok_or(())?;
        let minute = captures.name("minute").or_else(|| captures.name("minute2")).ok_or(())?.as_str().parse().ok().ok_or(())?;

        // AM/PM logic:
        // Under 24hr time, fail if we see AM or PM at all
        // Under 12hr time, without guessing enabled, fail if we don't see AM or PM, use specified
        // if we do
        // Under 12hr time, with guessing enabled, guess if we don't see AM or PM, use specified if
        // we do
        if CONFIG.prefs.use_12hr_time {
            let pm = if let Some(ap) = captures.name("ap") {
                ap.as_str().to_lowercase() == "p"
            } else if CONFIG.prefs.guess_am_pm {
                let am_option = Self::try_new_12hr(hour, minute, false).ok_or(())?;
                let now = Self::now();
                now - am_option > TimeDelta::hours(6) && now - am_option < TimeDelta::hours(18)
            } else {
                Err(())?
            };
            Self::try_new_12hr(hour, minute, pm).ok_or(())
        } else {
            if captures.name("ap").is_some() {
                Err(())?
            }
            Self::try_new(hour, minute).ok_or(())
        }
    }
}

pub trait Apply<T> {
    fn apply(&mut self, delta: T) -> TaskitResult<()>;
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
            .filter(|c| c.inner().starts_with(input))
            .map(Category::own)
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
            .filter(|c| c.inner().starts_with(input))
            .map(Category::own)
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
            .filter(|t| t.inner().starts_with(input))
            .map(|t| {
                format!("{}", t)
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
            .any(|cat| cat.inner() == input)
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
        if self.0.contains_match(input) {
            Ok(Validation::Valid)
        } else {
            Ok(Validation::Invalid(ErrorMessage::Default))
        }
    }
}

impl<'a> StringValidator for TagCompleter<'a> {
    fn validate(&self, input: &str) -> Result<Validation, inquire::CustomUserError> {
        let tag = input.strip_prefix('#').unwrap_or(input);
        if self.0.contains_match(tag) {
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
        if CONFIG.prefs.use_12hr_time {
            let hour = self.hour % 12;
            write!(f, "{:02}:{:02} {}", if hour == 0 {12} else {hour}, self.minute, if self.hour >= 12 {"pm"} else {"am"})
        } else {
            write!(f, "{:02}:{:02}", self.hour, self.minute)
        }
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnverifiedEventV1 {
    pub start_time: SimpleTime,
    pub end_time: SimpleTime, // if end_time before start_time: counts as that time on date + 1
    pub date: NaiveDate,
    pub category: String,
    pub comments: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnverifiedEventV5 {
    pub start_time: SimpleTime,
    pub end_time: SimpleTime, // if end_time before start_time: counts as that time on date + 1
    pub date: NaiveDate,
    pub category: String,
    #[serde(rename = "comments")]
    pub description: String,
    pub tags: HashSet<String>,
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
    events: Vec<UnverifiedEventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV2 {
    categories: Categories,
    archived_categories: Categories,
    events: Vec<UnverifiedEventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV3 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    // Maps from category name to tags
    tag_map: HashMap<String, Vec<String>>,
    events: Vec<UnverifiedEventV1>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV4 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    // Maps from category name to tags
    tag_map: HashMap<String, Vec<String>>,
    events: Vec<UnverifiedEventV1>,
    daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV5 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    /// Maps from category name to tags
    tag_map: HashMap<String, Vec<String>>,
    events: Vec<UnverifiedEventV5>,
    daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV6 {
    categories: Categories,
    archived_categories: Categories,
    tags: Vec<String>,
    /// Maps from category name to tags
    tag_map: HashMap<String, HashSet<String>>,
    events: Vec<UnverifiedEventV5>,
    daily_notes: HashMap<NaiveDate, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct UnverifiedSaveDataV7 {
    categories: Vec<String>,
    archived_categories: Vec<String>,
    tags: Vec<String>,
    /// Maps from category name to tags
    tag_map: HashMap<String, Vec<String>>,
    events: Vec<UnverifiedEventV5>,
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
                    |UnverifiedEventV1 {
                         start_time,
                         end_time,
                         date,
                         category,
                         comments,
                     }| UnverifiedEventV5 {
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
            tag_map: tag_map.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect(),
            events,
            daily_notes,
        }
    }
}

// ================================= END VERSIONING WORK =================================
