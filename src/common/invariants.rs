use std::{collections::{HashMap, HashSet}, fmt::Display, ops::Deref, sync::Arc};

use chrono::NaiveDate;

use crate::{common::{Apply, DeltaItem, SimpleTime, UnverifiedEventV5, UnverifiedSaveDataLatest, UnverifiedSaveDataVersioned, error::TaskitResult}, input::get_description_tags, util::SetVec};

/// Each of these represents an invariant for the SaveData struct.
#[allow(unused)] // for now we need this because the fields are only used for Debug impl
#[derive(Debug)]
pub enum VerificationError {
    /// each element of `categories` U `archived_categories` must be unique - bool is true iff
    /// violation is between both sets, false if it is contained to one of them
    NonUniqueCategories(String, bool),
    /// each element of `tags` should be unique
    NonUniqueTags(String),
    /// no element of `tags` should contain a space
    TagWithSpace(String),
    /// every key in `tag_map` should be an element of `categories`
    TagMapInvalidCategory(String),
    /// every element of every value of `tag_map` should be an element of `tags`
    TagMapInvalidTag(String),
    /// no value of `tag_map` should contain duplicates
    TagMapDuplicateTag(String),
    /// `event.category` should be an element of `categories` U `archived_categories`
    EventInvalidCategory(String),
    /// each element of `event.tags` should be an element of `tags`
    EventInvalidTag(String),
    /// `event.tags` should equal the list of words prefixed with `#` in event.description
    EventTagsMismatch {
        in_string: HashSet<String>,
        in_vec: HashSet<String>,
    },
}

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
/// Guaranteed to be a valid category
pub struct Category(Arc<str>);

impl Category {
    fn new(s: String) -> Self {
        Category(s.into())
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Hash, PartialOrd, Ord)]
/// Guaranteed to be a valid tag
/// Theoretically, this could be interned...
pub struct Tag(Arc<str>);

impl Tag {
    fn new(s: String) -> Self {
        Tag(s.into())
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub start_time: SimpleTime,
    pub end_time: SimpleTime, // if end_time before start_time: counts as that time on date + 1
    pub date: NaiveDate,
    pub category: Category,
    pub description: String,
    pub tags: HashSet<Tag>,
}

#[derive(Clone, Debug)]
pub struct SaveData {
    pub categories: SetVec<Category>,
    pub archived_categories: SetVec<Category>,
    pub tags: SetVec<Tag>,
    pub tag_map: HashMap<Category, HashSet<Tag>>,
    pub events: Vec<Event>,
    pub daily_notes: HashMap<NaiveDate, String>,
}


impl UnverifiedSaveDataLatest {
    /// Ensure that all invariants hold, erroring out if they don't
    pub fn verify(self) -> Result<SaveData, VerificationError> {
        // VerificationError::NonUniqueCategories
        for i in self.categories.iter() {
            for j in self.archived_categories.iter() {
                if i == j {
                    return Err(VerificationError::NonUniqueCategories(i.clone(), true))
                }
            }
        }
        let categories = self.categories
            .into_iter()
            .try_fold(SetVec::new(), |mut set, el|
                set.push(Category::new(el))
                    .map(|()| set)
                    .map_err(|s| VerificationError::NonUniqueCategories(s.0.to_string(), false))
            )?;
        let archived_categories = self.archived_categories
            .into_iter()
            .try_fold(SetVec::new(), |mut set, el|
                set.push(Category::new(el))
                    .map(|()| set)
                    .map_err(|s| VerificationError::NonUniqueCategories(s.0.to_string(), false))
            )?;

        // VerificationError::NonUniqueTags
        let mut tags = SetVec::new();
        for i in self.tags {
            if let Err(duplicate) = tags.push(Tag::new(i)) {
                return Err(VerificationError::NonUniqueTags(duplicate.0.to_string()))
            }
        }

        // VerificationError::TagWithSpace
        for i in tags.iter() {
            if i.0.contains(char::is_whitespace) {
                return Err(VerificationError::TagWithSpace(i.0.clone().to_string()));
            }
        }

        // VerificationError::TagMapInvalidCategory & VerificationError::TagMapInvalidTag
        let mut tag_map = HashMap::new();
        for (cat, map_tags) in self.tag_map {
            let key = categories.find(&cat).ok_or_else(|| VerificationError::TagMapInvalidCategory(cat.clone()))?.clone();
            let val = map_tags.into_iter().try_fold(HashSet::new(), |mut set, tag_name| {
                let tag = tags.find(&tag_name).ok_or(VerificationError::TagMapInvalidTag(tag_name))?.clone();
                if set.insert(tag.clone()) {
                    Ok(set)
                } else {
                    Err(VerificationError::TagMapDuplicateTag(tag.0.to_string()))
                }
            })?;
            tag_map.insert(key, val);
        }

        // event errors
        let mut events = Vec::new();
        for event in self.events {
            // VerificationError::EventInvalidCategory
            let category = categories.iter().chain(archived_categories.iter()).find(|cat| cat.inner() == event.category).ok_or_else(|| VerificationError::EventInvalidCategory(event.category))?.clone();

            // VerificationError::EventInvalidTag
            let tags = event.tags.into_iter().map(|tag|
                tags
                .find(&tag)
                .cloned()
                .ok_or_else(|| VerificationError::EventInvalidTag(tag))
            ).collect::<Result<HashSet<Tag>, VerificationError>>()?;

            // VerificationError::EventTagsMismatch
            let description_tags = get_description_tags(&event.description);
            if tags != description_tags.iter().cloned().map(Tag::new).collect() {
                return Err(VerificationError::EventTagsMismatch {
                    in_string: description_tags,
                    in_vec: tags.iter().map(|t| t.inner().to_owned()).collect(),
                });
            }
            events.push(Event {
                start_time: event.start_time, 
                end_time: event.end_time, 
                date: event.date, 
                category, 
                description: event.description, 
                tags
            })
        }

        Ok(SaveData {
            categories,
            archived_categories,
            tags,
            tag_map,
            events,
            daily_notes: self.daily_notes,
        })
    }

    /// Ensure that invariants hold. If they don't, try to fix them before erroring out.
    pub fn fix_and_verify(mut self) -> Result<SaveData, VerificationError> {
        // VerificationError::NonUniqueCategories
        for i in self.categories.iter() {
            for j in self.archived_categories.iter() {
                if i == j {
                    return Err(VerificationError::NonUniqueCategories(i.clone(), true))
                }
            }
        }
        let categories: SetVec<_> = self.categories.into_iter().map(Category::new).collect();
        let archived_categories: SetVec<_> = self.archived_categories.into_iter().map(Category::new).collect();

        // VerificationError::TagWithSpace
        let mut changes = Vec::new();
        for (i, tag) in self.tags.iter().enumerate() {
            if tag.contains(char::is_whitespace) {
                let fix = tag.replace(char::is_whitespace, "-");
                if self.tags.contains(&fix) {
                    return Err(VerificationError::TagWithSpace(tag.clone()));
                } else {
                    changes.push((i, fix));
                }
            }
        }
        for (i, fix) in changes {
            self.tags[i] = fix;
        }

        // VerificationError::NonUniqueTags
        let tags: SetVec<_> = self.tags.into_iter().map(Tag::new).collect();
        // TODO: why does the below line compile? (does it actually? does it also for ordinary Vec?)
        // tags.iter_mut().map(|e| *e = Tag(String::new()));

        // VerificationError::TagMapInvalidCategory & VerificationError::TagMapInvalidTag
        self.tag_map.retain(|cat, _| categories.contains_match(cat));
        self.tag_map.iter_mut().for_each(|(_, map_tags)| map_tags.retain(|tag| tags.contains_match(tag)));
        let tag_map = self.tag_map
            .into_iter()
            .filter_map(|(cat, map_tags)| categories.find(&cat)
                .map(|cat| (cat.clone(), map_tags.into_iter().filter_map(|tag|
                    tags.find(&tag).cloned()
                ).collect())))
            .collect();

        // event errors
        let mut events = Vec::new();
        for event in self.events {
            // VerificationError::EventInvalidCategory
            let category = categories.iter().chain(archived_categories.iter()).find(|cat| cat.inner() == event.category).ok_or_else(|| VerificationError::EventInvalidCategory(event.category))?.clone();

            // VerificationError::EventTagsMismatch
            let event_tags = get_description_tags(&event.description);

            // VerificationError::EventInvalidTag
            let tags = event_tags.into_iter().map(|tag| tags.find(&tag).ok_or(VerificationError::EventInvalidTag(tag)).cloned()).collect::<Result<_, _>>()?;

            events.push(Event {
                start_time: event.start_time,
                end_time: event.end_time,
                date: event.date,
                category,
                description: event.description,
                tags,
            })
        }

        Ok(SaveData {
            categories,
            archived_categories,
            tags,
            tag_map,
            events,
            daily_notes: self.daily_notes,
        })
    }
}

impl From<Event> for UnverifiedEventV5 {
    fn from(value: Event) -> Self {
        Self {
            start_time: value.start_time,
            end_time: value.end_time,
            date: value.date,
            category: value.category.own(),
            description: value.description,
            tags: value.tags.into_iter().map(Tag::own).collect(),
        }
    }
}

impl From<SaveData> for UnverifiedSaveDataLatest {
    fn from(value: SaveData) -> Self {
        UnverifiedSaveDataLatest {
            categories: value.categories.iter().map(Category::own).collect(),
            archived_categories: value.archived_categories.iter().map(Category::own).collect(),
            tags: value.tags.into_iter().map(Tag::own).collect(),
            tag_map: value.tag_map.into_iter().map(|(k, v)| (k.own(), v.into_iter().map(Tag::own).collect())).collect(),
            events: value.events.into_iter().map(Into::into).collect(),
            daily_notes: value.daily_notes,
        }
    }
}

impl From<SaveData> for UnverifiedSaveDataVersioned {
    fn from(value: SaveData) -> Self {
        UnverifiedSaveDataLatest::from(value).into()
    }
}

impl Deref for Tag {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl Deref for Category {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl Tag {
    pub fn own(self) -> String {
        self.0.to_string()
    }

    pub fn inner(&self) -> &str {
        &self.0
    }
}

impl Category {
    pub fn own(&self) -> String {
        self.0.to_string()
    }

    pub fn inner(&self) -> &str {
        &self.0
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.inner())
    }
}

impl Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} ({}, {}-{})",
            self.category.inner(), self.description, self.date, self.start_time, self.end_time
        )
    }
}

/// Constructs an AddTag DeltaItem and returns the Tag that will be generated, assuming it's added to
/// the list
pub fn add_tag(s: String) -> (DeltaItem, Tag) {
    let tag = Tag::new(s);
    (DeltaItem::AddTag(Opaque(tag.clone())), tag)
}

/// Constructs an AddCategory DeltaItem and returns the Category that will be generated, assuming
/// it's added to the list
pub fn add_category(s: String) -> (DeltaItem, Category) {
    let cat = Category::new(s);
    (DeltaItem::AddCategory(Opaque(cat.clone())), cat)
}

/// Constructs a RenameCategory DeltaItem and returns the Category that will be generated, assuming
/// it's added to the list
pub fn rename_category(pre: Category, post: String) -> (DeltaItem, Category) {
    let cat = Category::new(post);
    (DeltaItem::RenameCategory { old: pre, new: Opaque(cat.clone()) }, cat)
}


impl Apply<DeltaItem> for SaveData {
    fn apply(&mut self, delta: DeltaItem) -> TaskitResult<()> {
        match delta {
            DeltaItem::AddCategory(Opaque(category)) => {
                assert!(self.categories.push(category).is_ok());
            }
            DeltaItem::RenameCategory { old, new: Opaque(new) } => {
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
            DeltaItem::AddTag(Opaque(tag)) => {
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
                assert!(self.events.iter().all(|ev| !ev.tags.contains(&t)));
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

/// There's gotta be a better way to do this...
/// that said. the point here is to allow certain variants in super::DeltaItem to have arguments
/// that can only be constructed by this module. For instance, for efficiency reasons
/// `DeltaItem::AddCategory` should take a `Category` argument rather than a `String` argument, but
/// we don't want to allow constructing it from existing categories. The solution is to instead wrap
/// it in `Opaque`, so the only way to construct an instance is if you have access to the Opaque
/// constructor, which only we do.
#[derive(Debug)]
pub(super) struct Opaque<T>(T);
