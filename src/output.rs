use std::{cmp::min, collections::{BTreeMap, HashMap, HashSet}, fmt::Display, io::stdout, iter, mem, sync::LazyLock};

use chrono::{NaiveDate, NaiveDateTime, TimeDelta};
use crossterm::{cursor::MoveTo, event::{self, Event as CEvent, KeyModifiers}, execute, terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType}};
use itertools::Itertools;
use ratatui::{
    layout::{Constraint, Direction, Layout}, style::{Style, Stylize}, text::{Line, Span, Text}, widgets::{Block, Paragraph}, Frame
};

use crate::common::{Categories, CategoriesPair, DeltaItem, Event, SaveData, error::{Source, TaskitResult, With}};

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

static HEADER: LazyLock<&[HeaderButton]> = LazyLock::new(|| vec![
    HeaderButton::Filter(Filter::StartDate(Default::default())),
    HeaderButton::Filter(Filter::EndDate(Default::default())),
    HeaderButton::Filter(Filter::Category(Default::default())),
    HeaderButton::Filter(Filter::Description(Default::default())),
    HeaderButton::DeleteLastFilter,
    HeaderButton::ClearFilters,
].leak());

enum Filter {
    StartDate(NaiveDate),
    EndDate(NaiveDate),
    Category(String),
    Description(String),
}

enum HeaderButton {
    /// NOTE: argument here should be discriminant, it's only not bc rust makes that a PITA
    Filter(Filter),
    DeleteLastFilter,
    ClearFilters,
}

impl Display for HeaderButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeaderButton::Filter(Filter::StartDate(_)) => write!(f, "Start Date"),
            HeaderButton::Filter(Filter::EndDate(_)) => write!(f, "End Date"),
            HeaderButton::Filter(Filter::Category(_)) => write!(f, "Category"),
            HeaderButton::Filter(Filter::Description(_)) => write!(f, "Description"),
            HeaderButton::DeleteLastFilter => write!(f, "(delete last)"),
            HeaderButton::ClearFilters => write!(f, "(reset)"),
        }
    }
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
            Filter::Description(description) => ev.description.contains(description),
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

pub fn filter_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let mut terminal = ratatui::init();
    let mut messages: Vec<Message> = Vec::new();
    let mut events = save_data.events.clone();
    events.sort_by_key(|e| {
        -NaiveDateTime::new(e.date, e.start_time.try_into().expect("trust that save file only contains valid timestamps"))
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
        terminal.draw(|f| state.render(f)).expect("core assumption: terminal works");
        state.handle_keypresses(|m| messages.push(m));
        for message in mem::take(&mut messages).into_iter() {
            match state.handle_message(message)? {
                Some(Extrinsic::Halt) => {halt = true;},
                Some(Extrinsic::ResetRatatui) => {terminal.clear().with(Source::DrawingTui)?;},
                None => {},
            }
        }
    }
    ratatui::restore();
    Ok(vec![])
}

impl<'a> State<'a> {
    fn handle_message(&mut self, message: Message) -> TaskitResult<Option<Extrinsic>> {
        match message {
            Message::Exit => return Ok(Some(Extrinsic::Halt)),
            Message::ScrollDown => self.scroll_position = self.scroll_position.saturating_add(3),
            Message::ScrollUp => self.scroll_position = self.scroll_position.saturating_sub(3),
            Message::TabLeft => self.header_highlight = self.header_highlight.saturating_sub(1),
            Message::TabRight => self.header_highlight = min(self.header_highlight + 1, HEADER.len()),
            Message::Enter => {
                        match HEADER[self.header_highlight] {
                            HeaderButton::Filter(Filter::StartDate(_)) => {
                                // temporarily breaking out of ratatui
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0)).with(Source::DrawingTui)?;
                                disable_raw_mode().with(Source::DrawingTui)?;
                                let date = inquire::DateSelect::new("Start date filter:").prompt();
                                enable_raw_mode().with(Source::DrawingTui)?;
                                if let Ok(date) = date {
                                    self.applied_filters.push(Filter::StartDate(date));
                                }
                                return Ok(Some(Extrinsic::ResetRatatui));
                            },
                            HeaderButton::Filter(Filter::EndDate(_)) => {
                                // temporarily breaking out of ratatui
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0)).with(Source::DrawingTui)?;
                                disable_raw_mode().with(Source::DrawingTui)?;
                                let date = inquire::DateSelect::new("Start date filter:").prompt();
                                enable_raw_mode().with(Source::DrawingTui)?;
                                if let Ok(date) = date {
                                    self.applied_filters.push(Filter::EndDate(date));
                                }
                                return Ok(Some(Extrinsic::ResetRatatui));
                            },
                            HeaderButton::Filter(Filter::Category(_)) => {
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0)).with(Source::DrawingTui)?;
                                disable_raw_mode().with(Source::DrawingTui)?;
                                let category = inquire::Text::new("Select a category:")
                                    .with_autocomplete(CategoriesPair(&self.categories, &self.archived_categories))
                                    .with_validator(CategoriesPair(&self.categories, &self.archived_categories))
                                    .prompt();
                                enable_raw_mode().with(Source::DrawingTui)?;
                                if let Ok(category) = category {
                                    self.applied_filters.push(Filter::Category(category));
                                }
                                return Ok(Some(Extrinsic::ResetRatatui));
                            },
                            HeaderButton::Filter(Filter::Description(_)) => self.editing_filter = Some(Filter::Description(String::new())),
                            HeaderButton::ClearFilters => self.applied_filters.clear(),
                            HeaderButton::DeleteLastFilter => { self.applied_filters.pop(); },
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
        Ok(None)
    }

    fn handle_keypresses(&self, mut emit: impl FnMut(Message)) {
        let event = event::read().expect("core assumption: terminal works");
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
                        => emit(Message::KeyTyped(key_event.code.as_char().expect("verified is_some() in condition"))),
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
                                ev.description.clone().into(),
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
        // ...so similar in structure to category_sums code; we've gotta stop doing so much code duplication.
        // Also the loops should probably be merged so we don't end up re-iterating over the event
        // list a million times.
        let tag_sums = self.events.iter()
            .filter(|ev| (&self.applied_filters, &self.editing_filter).filter(ev))
            .fold(self.tags.iter().map(|tag| (tag.as_str(), TimeDelta::zero())).collect::<BTreeMap<&str, TimeDelta>>(),
                |mut map, ev| {
                    let tags = self.tag_map.get(&ev.category).into_iter().flatten().chain(&ev.tags).collect::<HashSet<_>>();
                    for tag in tags {
                        map.get_mut(tag.as_str()).map(|t| *t += ev.end_time - ev.start_time);
                    }
                    map
                }
            );
        // let tag_sums = category_sums.iter()
        //     .fold(
        //         self.tags.iter().map(|tag| (tag.as_str(), TimeDelta::zero())).collect::<BTreeMap<&str, TimeDelta>>(),
        //         |mut map, (cat, dur)| {
        //             for tag in self.tag_map.get(cat.to_owned()).unwrap_or(&vec![]) {
        //                 *map.get_mut(tag.as_str()).unwrap() += *dur;
        //             }
        //             map
        //         }
        //     );

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
            .constraints(vec![Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)])
            .split(frame.area());
        let main_panel_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)])
            .split(outer_layout[1]);
        let header_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(iter::repeat_n(Constraint::Length(15), HEADER.len() + 1))
            .split(outer_layout[0]);
        frame.render_widget("arrow keys for navigation - enter to select", outer_layout[2]);

        frame.render_widget(Text::styled("Filters:", Style::new().bold()), header_layout[0]);
        for (i, option) in HEADER.iter().enumerate() {
            frame.render_widget(
                Paragraph::new(Text::styled(
                    option.to_string(),
                    if self.header_highlight == i {
                        Style::new().underlined()
                    } else {
                        Style::new()
                    },
                )),
                header_layout[i + 1],
            );
        }
        frame.render_widget(filters_widget, main_panel_layout[0]);
        frame.render_widget(events_widget, main_panel_layout[1]);
        frame.render_widget(aggregated_data_widget, main_panel_layout[2]);
    }
}
