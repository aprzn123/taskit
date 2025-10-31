use std::{
    io::{Write, stdout},
    thread::sleep,
    time::Duration,
};

use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use inquire::{Confirm, CustomType, DateSelect, Text};

use crate::common::{DeltaItem, Event, SaveData, SimpleTime, TagCompleter};

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
    let comments = Text::new("Notes:").prompt().unwrap();
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
    delta.push(DeltaItem::AddEvent(Event {
        start_time,
        end_time,
        date,
        category,
        comments,
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
    let comments = Text::new("Notes:").prompt().unwrap();
    delta.push(DeltaItem::AddEvent(Event {
        start_time,
        end_time,
        date,
        category,
        comments,
    }));
    delta
}

pub fn amend_main(save_data: SaveData) -> Vec<DeltaItem> {
    let mut delta = vec![];
    let index = save_data.events.len() - 1;

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
    let comments = Text::new("Notes:").with_default(&save_data.events[index].comments).prompt().unwrap();
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
    delta.push(DeltaItem::ChangeEvent { index, new_event: Event {
        start_time,
        end_time,
        date,
        category,
        comments,
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
