use std::{cmp::min, collections::{BTreeMap, HashMap}, fmt::Display, io::stdout, iter, mem, ops::Add};

use chrono::{NaiveDate, NaiveDateTime, TimeDelta};
use crossterm::{cursor::MoveTo, event::{self, Event as CEvent, KeyModifiers}, execute, terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType}};
use itertools::Itertools;
use ratatui::{
    layout::{Constraint, Direction, Layout}, style::{Style, Stylize}, text::{Line, Span, Text}, widgets::{Block, Paragraph}, Frame
};

use crate::common::{Categories, CategoriesPair, DeltaItem, Event, SaveData};

enum Message {
    Exit,
    ScrollDown,
    ScrollUp,
    TabLeft,
    TabRight,
    Enter,
    KeyTyped(char),
    Backspace,
    FinishFilter,
    CancelFilter,
}

// Messages to trigger events that can't be contained to the update function
enum Extrinsic {
    Halt,
    // for after we temporarily break out of the ratatui environment
    ResetRatatui,
}

struct State<'a> {
    categories: &'a Categories,
    archived_categories: &'a Categories,
    tags: &'a [String],
    tag_map: &'a HashMap<String, Vec<String>>,
    daily_notes: &'a HashMap<NaiveDate, String>,
    events: Vec<Event>,
    scroll_position: u16,
    header_highlight: usize,
    applied_filters: Vec<Filter>,
    editing_filter: Option<Filter>,
}

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum FilterCategory {
    StartDate,
    EndDate,
    Category,
    Description,
}

enum Filter {
    StartDate(NaiveDate),
    EndDate(NaiveDate),
    Category(String),
    Description(String),
}

impl From<FilterCategory> for usize {
    fn from(value: FilterCategory) -> Self {
        value as Self
    }
}

impl TryFrom<usize> for FilterCategory {
    type Error = ();

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::StartDate),
            1 => Ok(Self::EndDate),
            2 => Ok(Self::Category),
            3 => Ok(Self::Description),
            _ => Err(())
        }
    }
}

impl FilterCategory {
    const SIZE: usize = 3;
}

impl Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Filter::StartDate(date) => write!(f, "At/After: {date}"),
            Filter::EndDate(date) => write!(f, "At/Before: {date}"),
            Filter::Category(category) => write!(f, "Category: {category}"),
            Filter::Description(description) => write!(f, "Description contains: {description}"),
        }
    }
}

trait CanFilter {
    fn filter(&self, ev: &Event) -> bool;
}

impl CanFilter for Filter {
    fn filter(&self, ev: &Event) -> bool {
        match self {
            Filter::StartDate(date) => ev.date >= *date,
            Filter::EndDate(date) => ev.date <= *date,
            Filter::Category(category) => ev.category == *category,
            Filter::Description(description) => ev.comments.contains(description),
        }
    }
}

impl<T: CanFilter> CanFilter for Vec<T> {
    fn filter(&self, ev: &Event) -> bool {
        self.iter().all(|f| f.filter(ev))
    }
}

impl<T: CanFilter, U: CanFilter> CanFilter for (&T, &U) {
    fn filter(&self, ev: &Event) -> bool {
        self.0.filter(ev) && self.1.filter(ev)
    }
}

impl<T: CanFilter> CanFilter for Option<T> {
    fn filter(&self, ev: &Event) -> bool {
        self.as_ref().is_none_or(|f| f.filter(ev))
    }
}

fn duration_to_string(duration: &TimeDelta) -> String {
    let mut duration_string = String::new();
    if duration.num_hours() != 0 {
        duration_string.push_str(&format!("{}h", duration.num_hours()));
    }
    if duration.num_minutes() % 60 != 0 {
        duration_string.push_str(&format!("{}m", duration.num_minutes() % 60));
    }
    duration_string
}

pub fn filter_main(save_data: SaveData) -> Vec<DeltaItem> {
    let mut terminal = ratatui::init();
    let mut messages: Vec<Message> = Vec::new();
    let mut events = save_data.events.clone();
    events.sort_by_key(|e| {
        -NaiveDateTime::new(e.date, e.start_time.into())
            .and_utc()
            .timestamp()
    });
    let mut state = State {
        categories: &save_data.categories,
        archived_categories: &save_data.archived_categories,
        events,
        scroll_position: 0,
        header_highlight: 0,
        applied_filters: vec![],
        editing_filter: None,
        tags: &save_data.tags,
        tag_map: &save_data.tag_map,
        daily_notes: &save_data.daily_notes,
    };
    let mut halt = false;
    while !halt {
        terminal.draw(|f| state.render(f)).unwrap();
        state.handle_keypresses(|m| messages.push(m));
        for message in mem::take(&mut messages).into_iter() {
            match state.handle_message(message) {
                Some(Extrinsic::Halt) => {halt = true;},
                Some(Extrinsic::ResetRatatui) => {terminal.clear();},
                None => {},
            }
        }
    }
    ratatui::restore();
    vec![]
}

impl<'a> State<'a> {
    // returns true to halt program, false otherwise
    fn handle_message(&mut self, message: Message) -> Option<Extrinsic> {
        match message {
            Message::Exit => return Some(Extrinsic::Halt),
            Message::ScrollDown => self.scroll_position = self.scroll_position.saturating_add(3),
            Message::ScrollUp => self.scroll_position = self.scroll_position.saturating_sub(3),
            Message::TabLeft => self.header_highlight = self.header_highlight.saturating_sub(1),
            Message::TabRight => self.header_highlight = min(self.header_highlight + 1, FilterCategory::SIZE),
            Message::Enter => {
                        match self.header_highlight.try_into().unwrap() {
                            FilterCategory::StartDate => {
                                // temporarily breaking out of ratatui
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0));
                                disable_raw_mode();
                                let date = inquire::DateSelect::new("Start date filter:").prompt().unwrap();
                                enable_raw_mode();
                                self.applied_filters.push(Filter::StartDate(date));
                                return Some(Extrinsic::ResetRatatui);
                            },
                            FilterCategory::EndDate => {
                                // temporarily breaking out of ratatui
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0));
                                disable_raw_mode();
                                let date = inquire::DateSelect::new("Start date filter:").prompt().unwrap();
                                enable_raw_mode();
                                self.applied_filters.push(Filter::EndDate(date));
                                return Some(Extrinsic::ResetRatatui);
                            },
                            FilterCategory::Category => {
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0));
                                disable_raw_mode();
                                let category = inquire::Text::new("Select a category:")
                                    .with_autocomplete(CategoriesPair(&self.categories, &self.archived_categories))
                                    .with_validator(CategoriesPair(&self.categories, &self.archived_categories))
                                    .prompt()
                                    .unwrap();
                                enable_raw_mode();
                                self.applied_filters.push(Filter::Category(category));
                                return Some(Extrinsic::ResetRatatui);
                            },
                            FilterCategory::Description => self.editing_filter = Some(Filter::Description(String::new())),
                        }
                    },
            Message::KeyTyped(c) => {
                if let Some(Filter::Description(ref mut cat)) = self.editing_filter {
                    cat.push(c);
                }
            },
            Message::Backspace => {
                if let Some(Filter::Description(ref mut cat)) = self.editing_filter {
                    cat.pop();
                }
            },
            Message::FinishFilter => {
                if let Some(fil) = self.editing_filter.take() {
                    self.applied_filters.push(fil);
                }
            },
            Message::CancelFilter => {
                self.editing_filter = None;
            },
        }
        None
    }

    fn handle_keypresses(&self, mut emit: impl FnMut(Message)) {
        let event = event::read().unwrap();
        match event {
            CEvent::Key(key_event)
                if key_event.is_press()
                && key_event.code.is_char('c')
                && key_event.modifiers == KeyModifiers::CONTROL
                => emit(Message::Exit),
            CEvent::Key(key_event) 
                if key_event.is_press() 
                && key_event.code.is_down() 
                => emit(Message::ScrollDown),
            CEvent::Key(key_event) 
                if key_event.is_press() 
                && key_event.code.is_up() 
                => emit(Message::ScrollUp),
            _ => {
                if let Some(Filter::Description(_)) = self.editing_filter {
                    match event {
                        CEvent::Key(key_event) 
                        if key_event.is_press() 
                        && key_event.code.is_backspace()
                        => emit(Message::Backspace),
                        CEvent::Key(key_event)
                        if key_event.is_press()
                        && key_event.code.is_enter() 
                        => emit(Message::FinishFilter),
                        CEvent::Key(key_event)
                        if key_event.is_press()
                        && key_event.code.is_esc() 
                        => emit(Message::CancelFilter),
                        CEvent::Key(key_event) 
                        if key_event.is_press() 
                        && key_event.code.as_char().is_some()
                        => emit(Message::KeyTyped(key_event.code.as_char().unwrap())),
                        _ => {}
                    }
                } else {
                    match event {
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_char('q')
                            => emit(Message::Exit),
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_left()
                            => emit(Message::TabLeft),
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_right()
                            => emit(Message::TabRight),
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_enter()
                            => emit(Message::Enter),
                        _ => {}
                    }
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame) {
        let events_chunked = self
            .events
            .iter()
            .filter(|ev| (&self.applied_filters, &self.editing_filter).filter(ev))
            .chunk_by(|ev| ev.date);

        let events_lines: Vec<Line> = events_chunked
            .into_iter()
            .flat_map(|(date, group)| {
                let (group1, group2): (Vec<_>, Vec<_>) = group.map(|e| (e, e)).unzip();
                let duration: TimeDelta = group1.into_iter().map(|ev| ev.end_time - ev.start_time).sum();
                iter::once(Line::default().spans(vec![
                    Span::raw("------ "),
                    Span::styled(date.to_string(), Style::new().bold()),
                    Span::raw(" ("),
                    Span::styled(duration_to_string(&duration), Style::new().yellow()),
                    Span::raw(") ------"),
                ])).chain(
                    self.daily_notes.get(&date).map(|s| Line::styled(format!("[{s}]"), Style::new().cyan().dim().italic()))
                ).chain(
                    group2.into_iter().flat_map(|ev| {
                        let duration = ev.end_time - ev.start_time;
                        [
                            // Line::raw(format!("{}: {}-{}", ev.date, ev.start_time, ev.end_time)),
                            Line::default().spans(vec![
                                Span::styled(
                                    format!("{}-{} ", ev.start_time, ev.end_time),
                                    Style::new().bold(),
                                ),
                                Span::styled(duration_to_string(&duration), Style::new().dim()),
                            ]),
                            Line::default().spans(vec![
                                Span::styled(ev.category.clone(), Style::new().blue().bold()),
                                Span::from(" - "),
                                ev.comments.clone().into(),
                            ]),
                            Line::raw(""),
                        ]
                    })
                )
            })
            .collect();

        let events_widget = Paragraph::new(events_lines)
            .block(Block::bordered())
            .scroll((self.scroll_position, 0))
            .wrap(Default::default());

        let filters_lines: Vec<Line> = self.applied_filters.iter()
            .map(ToString::to_string)
            .chain(self.editing_filter.iter().map(|f| format!("(*) {f}")))
            .map(Line::raw)
            .collect();
        let filters_widget = Paragraph::new(filters_lines)
            .block(Block::bordered())
            .wrap(Default::default());

        let category_sums = self.events.iter()
            .filter(|ev| (&self.applied_filters, &self.editing_filter).filter(ev))
            .fold(
                self.categories.options.iter().map(|cat| (cat.as_str(), TimeDelta::zero())).collect::<BTreeMap<&str, TimeDelta>>(),
                |mut map, ev| {
                    map.get_mut(ev.category.as_str()).map(|t| *t += ev.end_time - ev.start_time);
                    map
                }
            );
        let tag_sums = category_sums.iter()
            .fold(
                self.tags.iter().map(|tag| (tag.as_str(), TimeDelta::zero())).collect::<BTreeMap<&str, TimeDelta>>(),
                |mut map, (cat, dur)| {
                    for tag in self.tag_map.get(cat.to_owned()).unwrap_or(&vec![]) {
                        *map.get_mut(tag.as_str()).unwrap() += *dur;
                    }
                    map
                }
            );
        let aggregated_data_lines: Vec<Line> = iter::once(Line::styled("Aggregated durations", Style::new().bold().underlined()))
            .chain(iter::once(Line::default().spans([
                Span::styled("all", Style::new().bold().green()),
                Span::raw(": "),
                Span::raw(duration_to_string(&category_sums.values().sum())),
            ])))
            .chain(category_sums.iter().map(|(cat, duration)| {
                // Line::raw(format!("{cat}: {duration_string}"))
                Line::default().spans([
                    Span::styled(cat.to_owned(), Style::new().bold().blue()),
                    Span::raw(": "),
                    Span::raw(duration_to_string(duration)),
                ])
            }))
            .chain(iter::once(Line::default()))
            .chain(tag_sums.iter().map(|(tag, dur)| {
                Line::default().spans([
                    Span::styled(tag.to_owned(), Style::new().bold().magenta()),
                    Span::raw(": "),
                    Span::raw(duration_to_string(dur)),
                ])
            }))
            .collect();
        let aggregated_data_widget = Paragraph::new(aggregated_data_lines)
            .block(Block::bordered());

        let outer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(1), Constraint::Fill(1)])
            .split(frame.area());
        let main_panel_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)])
            .split(outer_layout[1]);
        let header_options = ["Start Date", "End Date", "Category", "Description"];
        let header_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),
                Constraint::Length(20),
                Constraint::Length(20),
                Constraint::Length(20),
            ])
            .split(outer_layout[0]);

        for (i, option) in header_options.iter().enumerate() {
            frame.render_widget(
                Paragraph::new(Text::styled(
                    option.to_string(),
                    if self.header_highlight == i {
                        Style::new().underlined()
                    } else {
                        Style::new()
                    },
                )),
                header_layout[i],
            );
        }
        frame.render_widget(filters_widget, main_panel_layout[0]);
        frame.render_widget(events_widget, main_panel_layout[1]);
        frame.render_widget(aggregated_data_widget, main_panel_layout[2]);
    }
}
