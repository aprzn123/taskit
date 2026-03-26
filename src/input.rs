use std::{
    fmt::Display, io::{Write, stdout}, thread::sleep, time::Duration
};

use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use inquire::{Autocomplete, Confirm, CustomType, DateSelect, Select, Text};

use crate::common::{CategoriesPair, DeltaItem, Event, SaveData, SimpleTime, TagCompleter, error::{Kind, Source, TaskitResult, With}};

#[derive(Clone)]
struct DescriptionTagsAutocomplete<'a>(&'a [String]);

impl<'a> Autocomplete for DescriptionTagsAutocomplete<'a> {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        let partial_tag = input.split(' ').next_back()
            .and_then(|s| s.strip_prefix('#'));
        if let Some(partial_tag) = partial_tag {
            Ok(self.0.iter().filter(|tag| tag.starts_with(partial_tag)).map(|s| { 
                let mut out = String::from('#'); 
                out.push_str(s); 
                out 
            }).collect())
        } else {
            Ok(vec![])
        }
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, inquire::CustomUserError> {
        Ok(if let Some(suggestion) = highlighted_suggestion {
            let last_pound = input.rfind('#').expect("
                there will only be a highlighted suggestion if there were suggestions; 
                there will only be suggestions if there was a # in the input string
                ");
            let mut out = input[..last_pound].to_owned();
            out.push_str(&suggestion);
            Some(out)
        } else {
            None
        })
    }
}

fn get_description_tags(description: &str) -> Vec<String> {
    description.split(' ').filter(|s| s.starts_with('#')).map(|s| s[1..].to_owned()).collect()
}

/// Retun Ok of delta items required to add new tags, or Err if user refused
fn validate_description_tags(tags: &[String], valid_tags: &[String]) -> TaskitResult<Vec<DeltaItem>> {
    let mut out = vec![];
    for tag in tags.iter().filter(|tag| !valid_tags.contains(tag)) {
        if tag.contains(' ') {
            return Err(Kind::NoSpaceInTag.with(Source::CreatingTag));
        }
        let create = Confirm::new(&format!(
                "Tag #{tag} does not currently exist. Create it?"
            ))
            .prompt()
            .with(Source::CreatingTag)?;
        if create {
            out.push(DeltaItem::AddTag(tag.clone()));
        } else {
            return Err(Kind::Cancelled.with(Source::CreatingTag));
        }
    }
    Ok(out)
}

pub fn record_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let mut delta = vec![];
    let date = DateSelect::new("Date:").prompt().with(Source::CreatingEntry)?;
    let start_time = CustomType::<SimpleTime>::new("Start time:").prompt().with(Source::CreatingEntry)?;
    let category = Text::new("Select a category:")
        .with_autocomplete(&save_data.categories)
        .prompt()
        .with(Source::CreatingEntry)?;
    let comments = Text::new("Notes:").with_autocomplete(DescriptionTagsAutocomplete(save_data.tags.as_ref())).prompt().with(Source::CreatingEntry)?;
    let end_time = CustomType::<SimpleTime>::new("End time:").prompt().with(Source::CreatingEntry)?;
    if save_data.archived_categories.options.contains(&category) {
        println!("Category {category} is archived. Try again!");
        return record_main(save_data);
    }
    if !save_data.categories.options.contains(&category) {
        let create = Confirm::new(&format!(
            "Category {category} does not currently exist. Create it?"
        ))
        .prompt().with(Source::CreatingCategory)?;
        if create {
            delta.push(DeltaItem::AddCategory(category.clone()));
        } else {
            println!("Cannot create event with nonexistent category.");
            return record_main(save_data);
        }
    }
    let tags = get_description_tags(&comments);
    delta.extend(validate_description_tags(&tags, &save_data.tags)?);
    delta.push(DeltaItem::AddEvent(Event {
        start_time,
        end_time,
        date,
        category,
        description: comments,
        tags,
    }));
    Ok(delta)
}

pub fn stopwatch_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let mut delta = vec![];
    let start_datetime = chrono::Local::now();
    let date = start_datetime.date_naive();
    let start_time: SimpleTime = start_datetime.time().into();
    enable_raw_mode().with(Source::RunningStopwatch)?;
    'l: loop {
        let now: SimpleTime = chrono::Local::now().time().into();
        let timedelta = now - start_time;
        print!(
            "\r{:02}:{:02} (<Enter> to finish)",
            timedelta.num_hours(),
            timedelta.num_minutes() % 60,
        );
        stdout().flush().with(Source::DrawingTui)?;
        while event::poll(Duration::ZERO).with(Source::RunningStopwatch)? {
            if let CEvent::Key(ev) = event::read().with(Source::RunningStopwatch)? {
                if ev.is_press()
                    && ev.code == KeyCode::Char('c')
                    && ev.modifiers == KeyModifiers::CONTROL
                {
                    return Err(Kind::Cancelled.with(Source::RunningStopwatch));
                } else if ev.is_press() && ev.code == KeyCode::Enter {
                    break 'l;
                }
            }
        }
        sleep(Duration::from_millis(500));
    }
    disable_raw_mode().with(Source::RunningStopwatch)?;
    println!();
    let end_datetime = chrono::Local::now();
    let end_time: SimpleTime = end_datetime.time().into();
    let category = loop {
        let category_selection = Text::new("Select a category:")
            .with_autocomplete(&save_data.categories)
            .prompt()
            .with(Source::CreatingEntry)?;
        if save_data.categories.options.contains(&category_selection) {
            break category_selection;
        } else if save_data.archived_categories.options.contains(&category_selection) {
            println!("Category {category_selection} is archived. Try again!");
        } else if 
            Confirm::new(&format!(
                "Category {category_selection} does not currently exist. Create it?"
            ))
            .prompt()
            .with(Source::CreatingCategory)?
        {
            delta.push(DeltaItem::AddCategory(category_selection.clone()));
            break category_selection;
        }
    };
    let comments = Text::new("Notes:").with_autocomplete(DescriptionTagsAutocomplete(save_data.tags.as_ref())).prompt().with(Source::CreatingEntry)?;
    let tags = get_description_tags(&comments);
    delta.extend(validate_description_tags(&tags, &save_data.tags)?);
    delta.push(DeltaItem::AddEvent(Event {
        start_time,
        end_time,
        date,
        category,
        tags,
        description: comments,
    }));
    Ok(delta)
}

/// prompts the user to select an event. events are displayed in reverse order, and the index given
/// is reversed (0 for last element, 1 for next to last, etc)
fn prompt_for_reverse_index(save_data: &SaveData) -> TaskitResult<usize> {
    struct IndexedEvent<'a>(usize, &'a Event);
    impl Display for IndexedEvent<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "({:02}) {}", self.0 + 1, self.1)
        }
    }
     Ok(Select::new("select event to modify", save_data.events.iter().rev().enumerate().map(|(n, ev)| IndexedEvent(n, ev)).collect::<Vec<IndexedEvent>>()).prompt().with(Source::SelectingEntry)?.0)
}

pub fn delete_event_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let reverse_index = prompt_for_reverse_index(&save_data)?;
    let index = save_data.events.len() - 1 - reverse_index;
    let confirm = Confirm::new(format!("Are you sure you want to delete this event? {} [y/n]", save_data.events[index]).as_str())
        .prompt().with(Source::ConfirmingDelete)?;
    if confirm {
        Ok(vec![DeltaItem::DeleteEvent(index)])
    } else {
        Err(Kind::Cancelled.with(Source::ConfirmingDelete))
    }
}

pub fn dispatch_amend(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let reverse_index = prompt_for_reverse_index(&save_data)?;
    amend_main(save_data, reverse_index)
}

// reverse_index is the index of the event to be amended, counting from the end of the list
pub fn amend_main(save_data: SaveData, reverse_index: usize) -> TaskitResult<Vec<DeltaItem>> {
    let mut delta = vec![];
    let index = save_data.events.len() - 1 - reverse_index;

    let date = DateSelect::new("Date:").with_default(save_data.events[index].date).prompt().with(Source::EditingEntry)?;
    let start_time = CustomType::<SimpleTime>::new("Start time:")
        .with_default(save_data.events[index].start_time)
        .prompt()
        .with(Source::EditingEntry)?;
    let category = Text::new("Select a category:")
        .with_autocomplete(&save_data.categories)
        .with_default(&save_data.events[index].category)
        .prompt()
        .with(Source::EditingEntry)?;
    let comments = Text::new("Notes:").with_autocomplete(DescriptionTagsAutocomplete(save_data.tags.as_ref())).with_default(&save_data.events[index].description).prompt().with(Source::EditingEntry)?;
    let end_time = CustomType::<SimpleTime>::new("End time:").with_default(save_data.events[index].end_time).prompt().with(Source::EditingEntry)?;

    if save_data.archived_categories.options.contains(&category) {
        // println!("Cannot update event with archived category {category}.");
        return Err(Kind::CategoryArchived(category).with(Source::EditingEntry));
    }
    if !save_data.categories.options.contains(&category) {
        let create = Confirm::new(&format!(
            "Category {category} does not currently exist. Create it?"
        ))
        .prompt().with(Source::CreatingCategory)?;
        if create {
            delta.push(DeltaItem::AddCategory(category.clone()));
        } else {
            // println!("Cannot update event with nonexistent category.");
            return Err(Kind::Cancelled.with(Source::CreatingCategory));
        }
    }
    let tags = get_description_tags(&comments);
    delta.extend(validate_description_tags(&tags, &save_data.tags)?);
    delta.push(DeltaItem::ChangeEvent { index, new_event: Event {
        start_time,
        end_time,
        date,
        category,
        tags,
        description: comments,
    }});
    Ok(delta)
}

pub fn archive_main(save_data: SaveData, category: String) -> TaskitResult<Vec<DeltaItem>> {
    if save_data.categories.options.contains(&category) {
        Ok(vec![DeltaItem::ArchiveCategory(category)])
    } else if save_data.archived_categories.options.contains(&category) {
        Err(Kind::CategoryArchived(category).with(Source::ArchivingCategory))
    } else {
        Err(Kind::NoSuchCategory(category).with(Source::ArchivingCategory))
    }
}

pub fn tag_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let mut delta = vec![];
    let category = Text::new("Select a category to tag:")
        .with_autocomplete(&save_data.categories)
        .with_validator(&save_data.categories)
        .prompt()
        .with(Source::UpdatingTag)?;
    let tag = Text::new("Select a tag:")
        .with_autocomplete(TagCompleter(&save_data.tags))
        .prompt()
        .with(Source::UpdatingTag)?;
    let tag = if let Some(stripped) = tag.strip_prefix('#') { stripped.to_owned() } else { tag };
    if !save_data.tags.contains(&tag) {
        let create = Confirm::new(&format!(
                "Tag #{tag} does not currently exist. Create it?"
            ))
            .prompt()
            .with(Source::CreatingTag)?;
        if create {
            delta.push(DeltaItem::AddTag(tag.clone()));
        } else {
            return Err(Kind::Cancelled.with(Source::CreatingTag));
        }
    }
    delta.push(DeltaItem::TagCategory(category, tag));
    Ok(delta)
}

pub fn note_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let date = DateSelect::new("Date:").prompt().with(Source::EditingNote)?;
    let note = inquire::Editor::new("Daily Note:").with_predefined_text(save_data.daily_notes.get(&date).map(String::as_str).unwrap_or("")).prompt().with(Source::EditingNote)?;
    Ok(vec![DeltaItem::SetDailyNote(date, note)])
}

pub fn rename_category(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let category = Text::new("Select a category to rename:")
        .with_autocomplete(CategoriesPair(&save_data.categories, &save_data.archived_categories))
        .with_validator(CategoriesPair(&save_data.categories, &save_data.archived_categories))
        .prompt()
        .with(Source::UpdatingCategory)?;
    let new_name = Text::new("Select a new category name").prompt().with(Source::UpdatingCategory)?;
    if save_data.categories.options.contains(&new_name) || save_data.archived_categories.options.contains(&new_name) {
        // println!("Category {new_name} already exists!");
        Err(Kind::DuplicateCategory(category).with(Source::UpdatingCategory))
    } else {
        Ok(vec![DeltaItem::RenameCategory { old: category, new: new_name}])
    }
}

pub fn delete_category_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let mut delta = vec![];
    let category = Text::new("Select a category to delete:")
        .with_autocomplete(CategoriesPair(&save_data.categories, &save_data.archived_categories))
        .with_validator(CategoriesPair(&save_data.categories, &save_data.archived_categories))
        .prompt()
        .with(Source::DeletingCategory)?;
    if save_data.events.iter().any(|ev| ev.category == category) {
        return Err(Kind::CategoryNotEmpty(category.clone()).with(Source::DeletingCategory));
    }
    if !Confirm::new(&format!("Are you sure you want to delete category {category}? [y/n]")).prompt().with(Source::DeletingCategory)? {
        return Err(Kind::Cancelled.with(Source::DeletingCategory));
    }
    if save_data.categories.options.contains(&category) {
        delta.push(DeltaItem::ArchiveCategory(category.clone()));
    }
    delta.push(DeltaItem::DeleteCategory(category));
    Ok(delta)
}

pub fn delete_tag_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let tag = Text::new("Select a tag to delete:")
        .with_autocomplete(TagCompleter(save_data.tags.as_ref()))
        .with_validator(TagCompleter(save_data.tags.as_ref()))
        .prompt()
        .with(Source::DeletingTag)?;
    let tag = if let Some(stripped) = tag.strip_prefix('#') { stripped.to_owned() } else { tag };
    if !Confirm::new(&format!("Are you sure you want to delete tag #{tag}? [y/n]")).prompt().with(Source::DeletingTag)? {
        return Err(Kind::Cancelled.with(Source::DeletingTag));
    }
    Ok(vec![DeltaItem::DeleteTag(tag)])
}
