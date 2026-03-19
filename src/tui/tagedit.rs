use std::{collections::{HashMap, HashSet}, mem};

use crossterm::event::{Event as CEvent, KeyModifiers};
use ratatui::{layout::{Constraint, Direction as LDirection, Layout}, style::{Color, Style, Stylize}, text::Text, widgets::{Block, BorderType, List, ListState}};
use smallvec::SmallVec;

use crate::{common::{Categories, DeltaItem, SaveData, error::TaskitResult}, tui::framework::{self, TuiState}};

type Extrinsic<'a> = framework::Extrinsic<State<'a>>;

enum Message {
    Exit,
    Right,
    Up,
    Down,
    Left,
    SwapColumns,
    Toggle,
}

enum Direction {
    CategoryToTag,
    TagToCategory
}

#[derive(Clone, PartialEq, Eq)]
enum Column {
    Left,
    Right
}

struct State<'a> {
    categories: &'a Categories,
    /// Maps from category name to tags
    original_tag_map: &'a HashMap<String, Vec<String>>,
    /// Maps from category name to tags
    new_tag_map: HashMap<String, HashSet<String>>,
    direction: Direction,
    column: Column,
    /// each element is (category, tag)
    toggles: HashSet<(String, String)>,
    left_list_state: ListState,
    right_list_state: ListState,
    tags: &'a [String],
}

impl Direction {
    fn flip(&mut self) {
        match self {
            Direction::CategoryToTag => *self = Direction::TagToCategory,
            Direction::TagToCategory => *self = Direction::CategoryToTag,
        }
    }
}

impl Column {
    fn hl_style(&self, focused: Self) -> Style {
        if self == &focused {
            Style::new().bg(Color::Blue)
        } else {
            Style::new().bg(Color::White).fg(Color::Black)
        }
    }
}

impl framework::Message for Message {
    fn init() -> Option<Self> {
        None
    }
}

impl<'a> State<'a> {
    fn focused_list_state_mut(&mut self) -> &mut ListState {
        match self.column {
            Column::Left => &mut self.left_list_state,
            Column::Right => &mut self.right_list_state,
        }
    }

    fn left(&self) -> &'a [String] {
        match self.direction {
            Direction::CategoryToTag => self.categories.options.as_ref(),
            Direction::TagToCategory => self.tags,
        }
    }

    fn right(&self) -> &'a [String] {
        match self.direction {
            Direction::CategoryToTag => self.tags,
            Direction::TagToCategory => self.categories.options.as_ref(),
        }
    }

    fn selected_category(&self) -> &'a str {
        self.categories.options[match self.direction {
            Direction::CategoryToTag => &self.left_list_state,
            Direction::TagToCategory => &self.right_list_state,
        }.selected().expect("one is always selected")].as_str()
    }

    fn selected_tag(&self) -> &'a str {
        self.tags[match self.direction {
            Direction::CategoryToTag => &self.right_list_state,
            Direction::TagToCategory => &self.left_list_state,
        }.selected().expect("one is always selected")].as_str()
    }
}

impl<'a> TuiState for State<'a> {
    type Message = Message;
    type Call = ();
    type Response = ();
    type Output = Vec<DeltaItem>;

    fn handle_message(&mut self, message: Self::Message, _: &framework::sync::ExternalFunction<Self::Call, Self::Response>) -> TaskitResult<Option<framework::Extrinsic<Self>>> {
        match message {
            Message::Exit => return Ok(Some(Extrinsic::Halt)),
            Message::Right => self.column = Column::Right,
            Message::Left => self.column = Column::Left,
            Message::Up => self.focused_list_state_mut().select_previous(),
            Message::Down => self.focused_list_state_mut().select_next(),
            Message::SwapColumns => {
                self.direction.flip();
                mem::swap(&mut self.left_list_state, &mut self.right_list_state);
            },
            Message::Toggle => {
                let selected = (self.selected_category().to_owned(), self.selected_tag().to_owned());
                let tags = self.new_tag_map.entry(selected.0.clone()).or_default();
                if !tags.remove(&selected.1) {
                    tags.insert(selected.1.clone());
                }
                if !self.toggles.remove(&selected) {
                    self.toggles.insert(selected);
                }
            },
        }
        Ok(None)
    }

    fn handle_keypresses(&self, event: CEvent) -> SmallVec<[Self::Message; 1]> {
        match event {
            CEvent::Key(k)
                if k.is_press()
                && k.code.is_char('q')
                => [Message::Exit].into(),
            CEvent::Key(k)
                if k.is_press()
                && k.code.is_char('c')
                && k.modifiers == KeyModifiers::CONTROL
                => [Message::Exit].into(),
            CEvent::Key(k)
                if k.is_press()
                && (k.code.is_char('h') || k.code.is_left())
                => [Message::Left].into(),
            CEvent::Key(k)
                if k.is_press()
                && (k.code.is_char('j') || k.code.is_down())
                => [Message::Down].into(),
            CEvent::Key(k)
                if k.is_press()
                && (k.code.is_char('k') || k.code.is_up())
                => [Message::Up].into(),
            CEvent::Key(k)
                if k.is_press()
                && (k.code.is_char('l') || k.code.is_right())
                => [Message::Right].into(),
            CEvent::Key(k)
                if k.is_press()
                && k.code.is_tab()
                => [Message::SwapColumns].into(),
            CEvent::Key(k)
                if k.is_press()
                && k.code.is_enter()
                => [Message::Toggle].into(),
            _ => SmallVec::new()
        }
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        let vertical_layout = Layout::default().direction(LDirection::Vertical).constraints([
            Constraint::Fill(1),
            Constraint::Length(1),
        ]).split(frame.area());
        let main_panels = Layout::default().direction(LDirection::Horizontal).constraints([
            Constraint::Fill(1),
            Constraint::Fill(2),
        ]).split(vertical_layout[0]);
        frame.render_widget(Text::raw("(arrow keys to move, tab to swap tags and categories, enter to add/remove tag)"), vertical_layout[1]);
        frame.render_stateful_widget(
            List::new(self.left().iter().map(String::as_str))
                .block(Block::bordered().border_type(BorderType::Rounded))
                .highlight_style(self.column.hl_style(Column::Left)), 
            main_panels[0], 
            &mut self.left_list_state
        );
        frame.render_stateful_widget(
            List::new(self.right().iter().map(|right_el| {
                let selected = match self.direction {
                    Direction::CategoryToTag => (self.selected_category().to_owned(), right_el.clone()),
                    Direction::TagToCategory => (right_el.clone(), self.selected_tag().to_owned()),
                };
                let mapping_exists = self.new_tag_map.entry(selected.0).or_default().contains(&selected.1);
                if mapping_exists {
                    Text::styled(format!("* {right_el}"), Style::new().bold().italic())
                } else {
                    Text::raw(format!("  {right_el}"))
                }
            }))
                .block(Block::bordered().border_type(BorderType::Rounded))
                .highlight_style(self.column.hl_style(Column::Right)), 
            main_panels[1], 
            &mut self.right_list_state
        );
    }

    fn external_function(_: Self::Call) -> Self::Response {
        ()
    }

    fn get_output(self) -> Self::Output {
        dbg!(self.toggles.into_iter().map(|(cat, tag)| if self.original_tag_map.get(&cat).is_some_and(|tags| tags.contains(&tag)) {
            DeltaItem::UntagCategory(cat, tag)
        } else {
            DeltaItem::TagCategory(cat, tag)
        }).collect())
    }
}

pub fn tagedit_main(save_data: SaveData) -> TaskitResult<Vec<DeltaItem>> {
    if save_data.categories.options.is_empty() || save_data.tags.is_empty() {
        println!("Must have at least one tag and one category to manage tags!");
        // Should this be Err?
        return Ok(vec![])
    }
    let state = State {
        original_tag_map: &save_data.tag_map, 
        categories: &save_data.categories, 
        tags: &save_data.tags, 
        new_tag_map: save_data.tag_map.iter().map(|(cat, tags)| (cat.clone(), tags.iter().cloned().collect())).collect(),
        direction: Direction::CategoryToTag,
        column: Column::Left,
        toggles: HashSet::new(),
        left_list_state: ListState::default().with_selected(Some(0)),
        right_list_state: ListState::default().with_selected(Some(0)),
    };
    state.run()
}
