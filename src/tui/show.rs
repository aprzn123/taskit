use std::{cmp::min, collections::{BTreeMap, HashMap, HashSet}, fmt::Display, io::stdout, iter, sync::{LazyLock}, time::{Duration, Instant}};

use chrono::{NaiveDate, NaiveDateTime, TimeDelta};
use crossterm::{cursor::MoveTo, event::{Event as CEvent, KeyModifiers}, execute, terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType}};
use inquire::error::InquireResult;
use itertools::Itertools;
use ratatui::{
    layout::{Constraint, Direction, Layout}, style::{Style, Stylize}, text::{Line, Span, Text}, widgets::{Block, Paragraph}, Frame
};
use smallvec::SmallVec;

use crate::{common::{Categories, CategoriesPair, DeltaItem, Event, SaveData, error::{Source, TaskitResult, With}}, tui::framework::{self, TuiState, sync::ExternalFunction}};

type Extrinsic<'a> = framework::Extrinsic<State<'a>>;

// 1 is a bit stupid but it might be what we want
type MessageVec = SmallVec<[Message; 1]>;

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
    BlinkCursor(bool),
}

impl framework::Message for Message {
    fn init() -> Option<Self> {
        Some(Self::BlinkCursor(true))
    }
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
    cursor_blink: bool,
    last_cursor_show_time: Instant,
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

enum InquireRequest<'a, 'b, 'c> {
    DateSelect(&'a str),
    CategoryFilter {categories: &'b Categories, archived_categories: &'c Categories},
}

enum InquireResponse {
    Date(InquireResult<NaiveDate>),
    Category(InquireResult<String>),
}

impl InquireResponse {
    fn date(self) -> Option<InquireResult<NaiveDate>> {
        match self {
            Self::Date(d) => Some(d),
            _ => None
        }
    }

    fn category(self) -> Option<InquireResult<String>> {
        match self {
            Self::Category(c) => Some(c),
            _ => None
        }
    }
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

impl<'a> framework::TuiState for State<'a> {
    type Message = Message;
    type Call = InquireRequest<'static, 'a, 'a>;
    type Response = InquireResponse;
    type Output = Vec<DeltaItem>;

    fn external_function(request: Self::Call) -> Self::Response {
        match request {
            InquireRequest::DateSelect(s) => InquireResponse::Date(inquire::DateSelect::new(s).prompt()),

            InquireRequest::CategoryFilter { categories, archived_categories } => InquireResponse::Category(
                inquire::Text::new("Select a category:")
                .with_autocomplete(CategoriesPair(categories, archived_categories))
                .with_validator(CategoriesPair(categories, archived_categories))
                .prompt()
            ),
        }
    }

    fn handle_message(&mut self, message: Self::Message, call: &ExternalFunction<Self::Call, Self::Response>) -> TaskitResult<Option<framework::Extrinsic<Self>>> {
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
                                let date = call.call(InquireRequest::DateSelect("Start date filter:"))
                                    .date().expect("requested a date").with(Source::SettingFilter)?;
                                enable_raw_mode().with(Source::DrawingTui)?;
                                self.applied_filters.push(Filter::StartDate(date));
                                return Ok(Some(Extrinsic::ResetRatatui));
                            },
                            HeaderButton::Filter(Filter::EndDate(_)) => {
                                // temporarily breaking out of ratatui
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0)).with(Source::DrawingTui)?;
                                disable_raw_mode().with(Source::DrawingTui)?;
                                let date = call.call(InquireRequest::DateSelect("End date filter:"))
                                    .date().expect("requested a date").with(Source::SettingFilter)?;
                                enable_raw_mode().with(Source::DrawingTui)?;
                                self.applied_filters.push(Filter::EndDate(date));
                                return Ok(Some(Extrinsic::ResetRatatui));
                            },
                            HeaderButton::Filter(Filter::Category(_)) => {
                                // temporarily breaking out of ratatui
                                execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0)).with(Source::DrawingTui)?;
                                disable_raw_mode().with(Source::DrawingTui)?;
                                let category = call.call(InquireRequest::CategoryFilter {
                                    categories: self.categories, 
                                    archived_categories: self.archived_categories,
                                }).category().expect("requested a category").with(Source::SettingFilter)?;
                                enable_raw_mode().with(Source::DrawingTui)?;
                                self.applied_filters.push(Filter::Category(category));
                                return Ok(Some(Extrinsic::ResetRatatui));
                            },
                            HeaderButton::Filter(Filter::Description(_)) => self.editing_filter = Some(Filter::Description(String::new())),
                            HeaderButton::ClearFilters => self.applied_filters.clear(),
                            HeaderButton::DeleteLastFilter => { self.applied_filters.pop(); },
                        }
                    },
            Message::KeyTyped(c) => {
                if let Some(Filter::Description(ref mut cat)) = self.editing_filter {
                    self.last_cursor_show_time = Instant::now();
                    self.cursor_blink = true;
                    cat.push(c);
                }
            },
            Message::Backspace => {
                if let Some(Filter::Description(ref mut cat)) = self.editing_filter {
                    self.last_cursor_show_time = Instant::now();
                    self.cursor_blink = true;
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
            Message::BlinkCursor(real) => {
                if real {
                    self.cursor_blink = !self.cursor_blink;
                }
                return Ok(Some(Extrinsic::ResolveAfter(Duration::from_millis(500), Box::new(|state| {
                        [Message::BlinkCursor(state.last_cursor_show_time.elapsed() > Duration::from_millis(500))].into()
                }))))
            }
        }
        Ok(None)
    }

    fn handle_keypresses(&self, ev: CEvent) -> MessageVec {
        match ev {
            CEvent::Key(key_event)
                if key_event.is_press()
                && key_event.code.is_char('c')
                && key_event.modifiers == KeyModifiers::CONTROL
                => [Message::Exit].into(),
            CEvent::Key(key_event)
                if key_event.is_press() 
                && key_event.code.is_down() 
                => [Message::ScrollDown].into(),
            CEvent::Key(key_event)
                if key_event.is_press() 
                && key_event.code.is_up() 
                => [Message::ScrollUp].into(),
            _ => {
                if let Some(Filter::Description(_)) = self.editing_filter {
                    match ev {
                        CEvent::Key(key_event)
                        if key_event.is_press() 
                        && key_event.code.is_backspace()
                        => [Message::Backspace].into(),
                        CEvent::Key(key_event)
                        if key_event.is_press()
                        && key_event.code.is_enter() 
                        => [Message::FinishFilter].into(),
                        CEvent::Key(key_event)
                        if key_event.is_press()
                        && key_event.code.is_esc() 
                        => [Message::CancelFilter].into(),
                        CEvent::Key(key_event)
                        if key_event.is_press() 
                        && key_event.code.as_char().is_some()
                        => [Message::KeyTyped(key_event.code.as_char().expect("verified is_some() in condition"))].into(),
                        _ => SmallVec::new()
                    }
                } else {
                    match ev {
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_char('q')
                            => [Message::Exit].into(),
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_left()
                            => [Message::TabLeft].into(),
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_right()
                            => [Message::TabRight].into(),
                        CEvent::Key(key_event)
                            if key_event.is_press()
                            && key_event.code.is_enter()
                            => [Message::Enter].into(),
                        _ => SmallVec::new()
                    }
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame) {
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
            .chain(self.editing_filter.iter().map(|f| {
                let cursor = if self.cursor_blink {"\u{2588}"} else {""};
                format!("(*) {f}{cursor}")
            }))
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
                    if let Some(t) = map.get_mut(ev.category.as_str()) {
                        *t += ev.end_time - ev.start_time;
                    }
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
                        if let Some(t) = map.get_mut(tag.as_str()) {
                            *t += ev.end_time - ev.start_time;
                        }
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

    fn get_output(self) -> Self::Output {
        vec![]
    }
}

pub fn filter_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    let mut events = save_data.events.clone();
    events.sort_by_key(|e| {
        -NaiveDateTime::new(e.date, e.start_time.try_into()
            .expect("trust that save file only contains valid timestamps"))
            .and_utc()
            .timestamp()
    });
    let state = State {
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
        cursor_blink: true,
        last_cursor_show_time: Instant::now(),
    };
    state.run()
}
