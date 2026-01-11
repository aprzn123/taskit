use std::{
    fmt::Display, io::{Write, stdout}, thread::sleep, time::Duration
};

use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use inquire::{Autocomplete, Confirm, CustomType, DateSelect, Select, Text};

use crate::common::{DeltaItem, Event, SaveData, SimpleTime, TagCompleter};

#[derive(Clone)]
struct DescriptionTagsAutocomplete<'a>(&'a [String]);

impl<'a> Autocomplete for DescriptionTagsAutocomplete<'a> {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        let partial_tag = input.split(' ').last()
            .and_then(|s| if s.starts_with('#') {Some( &s[1..]) } else { None });
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
            let last_pound = input.rfind('#').unwrap();
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

/// Retun Some of delta items required to add new tags, or None if user refused
fn validate_description_tags(tags: &[String], valid_tags: &[String]) -> Option<Vec<DeltaItem>> {
    let mut out = vec![];
    for tag in tags.iter().filter(|tag| !valid_tags.contains(tag)) {
        let create = Confirm::new(&format!(
                "Tag #{tag} does not currently exist. Create it?"
            ))
            .prompt()
            .unwrap();
        if create {
            out.push(DeltaItem::AddTag(tag.clone()));
        } else {
            return None;
        }
    }
    Some(out)
}

pub fn record_main(save_data: SaveData) -> Vec<DeltaItem> {
    let mut delta = vec![];
    let date = DateSelect::new("Date:").prompt().unwrap();
    let start_time = CustomType::<SimpleTime>::new("Start time:")
        .prompt()
        .unwrap();
    let category = Text::new("Select a category:")
        .with_autocomplete(&save_data.categories)
        .prompt()
        .unwrap();
    let comments = Text::new("Notes:").with_autocomplete(DescriptionTagsAutocomplete(save_data.tags.as_ref())).prompt().unwrap();
    let end_time = CustomType::<SimpleTime>::new("End time:").prompt().unwrap();
    if !save_data.categories.options.contains(&category) {
        let create = Confirm::new(&format!(
            "Category {category} does not currently exist. Create it?"
        ))
        .prompt()
        .unwrap();
        if create {
            delta.push(DeltaItem::AddCategory(category.clone()));
        } else {
            println!("Cannot create event with nonexistent category.");
            return record_main(save_data);
        }
    }
    let tags = get_description_tags(&comments);
    delta.extend(validate_description_tags(&tags, &save_data.tags).unwrap());
    delta.push(DeltaItem::AddEvent(Event {
        start_time,
        end_time,
        date,
        category,
        description: comments,
        tags,
    }));
    delta
}

pub fn stopwatch_main(save_data: SaveData) -> Vec<DeltaItem> {
    let mut delta = vec![];
    let start_datetime = chrono::Local::now();
    let date = start_datetime.date_naive();
    let start_time: SimpleTime = start_datetime.time().into();
    enable_raw_mode().unwrap();
    'l: loop {
        let now: SimpleTime = chrono::Local::now().time().into();
        let timedelta = now - start_time;
        print!(
            "\r{:02}:{:02} (<Enter> to finish)",
            timedelta.num_hours(),
            timedelta.num_minutes() % 60,
        );
        stdout().flush();
        while event::poll(Duration::ZERO).unwrap() {
            if let CEvent::Key(ev) = event::read().unwrap() {
                if ev.is_press()
                    && ev.code == KeyCode::Char('c')
                    && ev.modifiers == KeyModifiers::CONTROL
                {
                    return delta;
                } else if ev.is_press() && ev.code == KeyCode::Enter {
                    break 'l;
                }
            }
        }
        sleep(Duration::from_millis(500));
    }
    disable_raw_mode().unwrap();
    println!();
    let end_datetime = chrono::Local::now();
    let end_time: SimpleTime = end_datetime.time().into();
    let mut category = None;
    while category.is_none() {
        let category_selection = Text::new("Select a category:")
            .with_autocomplete(&save_data.categories)
            .prompt()
            .unwrap();
        if save_data.categories.options.contains(&category_selection) {
            category = Some(category_selection);
        } else if Confirm::new(&format!(
            "Category {category_selection} does not currently exist. Create it?"
        ))
        .prompt()
        .unwrap()
        {
            delta.push(DeltaItem::AddCategory(category_selection.clone()));
            category = Some(category_selection);
        }
    }
    let category = category.unwrap();
    let comments = Text::new("Notes:").with_autocomplete(DescriptionTagsAutocomplete(save_data.tags.as_ref())).prompt().unwrap();
    let tags = get_description_tags(&comments);
    delta.extend(validate_description_tags(&tags, &save_data.tags).unwrap());
    delta.push(DeltaItem::AddEvent(Event {
        start_time,
        end_time,
        date,
        category,
        tags,
        description: comments,
    }));
    delta
}

pub fn dispatch_amend(save_data: SaveData) -> Vec<DeltaItem> {
    struct IndexedEvent<'a>(usize, &'a Event);
    impl Display for IndexedEvent<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "({:02}) {}", self.0 + 1, self.1)
        }
    }
    let reverse_index = Select::new("select event to modify", save_data.events.iter().rev().enumerate().map(|(n, ev)| IndexedEvent(n, ev)).collect::<Vec<IndexedEvent>>()).prompt().unwrap().0;
    amend_main(save_data, reverse_index)
}

// reverse_index is the index of the event to be amended, counting from the end of the list
pub fn amend_main(save_data: SaveData, reverse_index: usize) -> Vec<DeltaItem> {
    let mut delta = vec![];
    let index = save_data.events.len() - 1 - reverse_index;

    let date = DateSelect::new("Date:").with_default(save_data.events[index].date).prompt().unwrap();
    let start_time = CustomType::<SimpleTime>::new("Start time:")
        .with_default(save_data.events[index].start_time)
        .prompt()
        .unwrap();
    let category = Text::new("Select a category:")
        .with_autocomplete(&save_data.categories)
        .with_default(&save_data.events[index].category)
        .prompt()
        .unwrap();
    let comments = Text::new("Notes:").with_autocomplete(DescriptionTagsAutocomplete(save_data.tags.as_ref())).with_default(&save_data.events[index].description).prompt().unwrap();
    let end_time = CustomType::<SimpleTime>::new("End time:").with_default(save_data.events[index].end_time).prompt().unwrap();

    if !save_data.categories.options.contains(&category) {
        let create = Confirm::new(&format!(
            "Category {category} does not currently exist. Create it?"
        ))
        .prompt()
        .unwrap();
        if create {
            delta.push(DeltaItem::AddCategory(category.clone()));
        } else {
            println!("Cannot update event with nonexistent category.");
            return delta;
        }
    }
    let tags = get_description_tags(&comments);
    delta.extend(validate_description_tags(&tags, &save_data.tags).unwrap());
    delta.push(DeltaItem::ChangeEvent { index, new_event: Event {
        start_time,
        end_time,
        date,
        category,
        tags,
        description: comments,
    }});
    delta
}

pub fn archive_main(save_data: SaveData, category: String) -> Vec<DeltaItem> {
    if save_data.categories.options.contains(&category) {
        vec![DeltaItem::ArchiveCategory(category)]
    } else {
        vec![]
    }
}

pub(crate) fn tag_main(save_data: SaveData) -> Vec<DeltaItem> {
    let mut delta = vec![];
    let category = Text::new("Select a category to tag:")
        .with_autocomplete(&save_data.categories)
        .with_validator(&save_data.categories)
        .prompt()
        .unwrap();
    let tag = Text::new("Select a tag:")
        .with_autocomplete(TagCompleter(&save_data.tags))
        .prompt()
        .unwrap();
    let tag = if tag.starts_with('#') { tag[1..].to_owned() } else { tag };
    if !save_data.tags.contains(&tag) {
        let create = Confirm::new(&format!(
                "Tag #{tag} does not currently exist. Create it?"
            ))
            .prompt()
            .unwrap();
        if create {
            delta.push(DeltaItem::AddTag(tag.clone()));
        } else {
            return vec![];
        }
    }
    delta.push(DeltaItem::TagCategory(category, tag));
    delta
}

pub fn note_main(save_data: SaveData) -> Vec<DeltaItem> {
    let date = DateSelect::new("Date:").prompt().unwrap();
    let note = inquire::Editor::new("Daily Note:").with_predefined_text(save_data.daily_notes.get(&date).map(String::as_str).unwrap_or("")).prompt().unwrap();
    vec![DeltaItem::SetDailyNote(date, note)]
}
