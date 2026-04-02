use std::{error::Error, fmt::Display, io};

use inquire::InquireError;

#[derive(Debug)]
pub struct TaskitError {
    pub kind: Kind,
    pub source: Source
}

#[derive(Debug)]
pub enum Kind {
    Cancelled,
    CategoryArchived(String),
    NoSuchCategory(String),
    DuplicateCategory(String),
    CategoryNotEmpty(String),
    Other(Box<dyn Error>),
    NoSpaceInTag,
}

/// Used in TaskitError for "error occurred while attempting to [...]"
#[derive(Debug)]
pub enum Source {
    CreatingTag,
    CreatingEntry,
    CreatingCategory,
    RunningStopwatch,
    SelectingEntry,
    EditingEntry,
    ArchivingCategory,
    UpdatingTag,
    EditingNote,
    UpdatingCategory,
    DrawingTui,
    SettingFilter,
    ConfirmingDelete,
    DeletingCategory,
    DeletingTag,
}

pub type TaskitResult<T> = Result<T, TaskitError>;

impl Source {
    fn activity(&self) -> &'static str {
        match self {
            Source::CreatingTag => "creating a tag",
            Source::CreatingEntry => "creating an entry",
            Source::CreatingCategory => "creating a category",
            Source::RunningStopwatch => "stopwatch was running",
            Source::SelectingEntry => "selecting an entry to edit",
            Source::EditingEntry => "editing an entry",
            Source::ArchivingCategory => "archiving a category",
            Source::UpdatingTag => "updating a tag",
            Source::EditingNote => "editing a daily note",
            Source::UpdatingCategory => "updating a category",
            Source::DrawingTui => "performing TUI operations",
            Source::SettingFilter => "setting a filter",
            Source::ConfirmingDelete => "confirming deletion",
            Source::DeletingCategory => "deleting a category",
            Source::DeletingTag => "deleting a tag",
        }
    }
}

impl Display for TaskitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {kind, source} = self;
        let activity = source.activity();
        match kind {
            Kind::Cancelled => write!(f, "User cancelled while {activity}."),
            Kind::CategoryArchived(c) => write!(f, "Operation ({activity}) could not be completed because category {c} is archived."),
            Kind::NoSuchCategory(c) => write!(f, "While {activity}, tried to use category '{c}', which doesn't exist."),
            Kind::DuplicateCategory(c) => write!(f, "While {activity}, tried to create category '{c}', which already exists."),
            Kind::CategoryNotEmpty(c) => write!(f, "Category {c} was not empty while {activity}."),
            Kind::NoSpaceInTag => write!(f, "Spaces aren't allowed in tags. Occurred while {activity}."),
            Kind::Other(error) => write!(f, "An external error occurred while {activity}: {error}"),
        }
    }
}

impl Error for TaskitError { }

impl From<(InquireError, Source)> for TaskitError {
    fn from((value, source): (InquireError, Source)) -> Self {
        match value {
            InquireError::NotTTY => panic!("Taskit assumes it is running in an interactive terminal"),
            InquireError::InvalidConfiguration(s) => panic!("internal error: invalid configuration\n{}", s),
            InquireError::IO(error) => error.with(source).into(),
            InquireError::OperationCanceled => Self { kind: Kind::Cancelled, source },
            InquireError::OperationInterrupted => Self{ kind: Kind::Cancelled, source },
            InquireError::Custom(error) => Self { kind: Kind::Other(error), source },
        }
    }
}

impl From<(io::Error, Source)> for TaskitError {
    fn from((value, source): (io::Error, Source)) -> Self {
        Self { kind: Kind::Other(Box::new(value)), source }
    }
}

/// attaches source data to a given error
pub trait With<T>: Sized {
    type Joined;
    fn with(self, source: T) -> Self::Joined;
}

impl With<Source> for io::Error {
    type Joined = (Self, Source);

    fn with(self, source: Source) -> Self::Joined {
        (self, source)
    }
}

impl With<Source> for InquireError {
    type Joined = (Self, Source);

    fn with(self, source: Source) -> Self::Joined {
        (self, source)
    }
}

impl With<Source> for Kind {
    type Joined = TaskitError;

    fn with(self, source: Source) -> Self::Joined {
        TaskitError { kind: self, source }
    }
}

impl<T, E> With<Source> for Result<T, E>
where E: With<Source>
{
    type Joined = Result<T, E::Joined>;

    fn with(self, source: Source) -> Self::Joined {
        self.map_err(|e| e.with(source))
    }
}

