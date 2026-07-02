# Taskit

Taskit is a terminal-based time tracking software. No official release has been published yet, but it is in a
fully workable and usable state.

## Features
- A minimal interface designed to be intuitive to use while minimizing barriers to consistent usage such as
extraneous inputs
- Cross-platform support (I only use Linux so I can't test on Windows or MacOS, but from what I've
heard from my friends it works fine)
- Add time spent with either a stopwatch (`taskit time`) or by manually entering times (`taskit add`)
- Amend previous entries to correct errors
- Make comments on an entire day with daily notes
- Group entries into categories for different types of task
- Archive out-of-use categories
- Group categories into tags using a TUI to track larger-scale and potentially overlapping blocks of time
- Display recorded events in a TUI, including
    - Total time over events in categories and tags
    - Filters for date, category, etc

## Installation
prerequisites: Rust, Cargo
`$ cargo install taskit-tracker`

## Configuration
The configuration file is located at `~/.config/taskit/config.toml`. The full list of config options, along
with their defaults, is available in `taskit.toml` in this repository.

## Architecture
Most of the architectural complexity in Taskit comes from a simple central decision. Let me guide you towards
making the same decision yourself.

We start at the beginning. At the start of each execution, Taskit must read the save file in order to function
(for instance, it allows `taskit time` to know what categories are available). Then, at the end of most
executions (excluding cases like `taskit show`), it must write back to the save file. However, many instances
of Taskit are long-running programs which have to accept the possibility that a `taskit add` could have quickly
been run in the meantime. If it just wrote back to the save file its own updated version, it would overwrite
the changes made in the meantime!

Therefore, we add an extra step. Before writing back to the save file, we read it back into memory, updating
the copy that was just read and then writing that back. This resolves the issue, but raises the question of how
to represent those changes. This is where `DeltaItem`s and the `Apply` trait come in. A `DeltaItem` represents
a single atomic change to the save file. For instance, `DeltaItem::AddEvent` appends an event to the list, and
`DeltaItem::RenameCategory` both changes the name of a category *and* updates all events with that category to
have the new name.

So, we represent each subcommand as a function which takes in both the initial `SaveData` and whatever
arguments it has, and outputs a `Vec<DeltaItem>` to be applied one-by-one. There are some changes I'm
considering to this system, though. In particular, I want to answer the question of whether the `DeltaItem`s
should be as independent from the `SaveData` as they are right now. If the ideas to be had there were to come
to pass, subcommands would instead be functions that take a `&mut SaveData` and update it using some version of
`Apply` that applies the DeltaItem and also adds it to a `Vec<DeltaItem>` field *within* the `SaveData` struct.
This seems like it could help with state desyncs, but I haven't given too much thought into the detailed pros
and cons.

In order to handle updating the save file over time, I've assembled a versioning system that is currently
fairly manual but will be easy to abstract into a proc macro. Each version is defined as a separate struct,
each of which is named `SaveDataUnverifiedV<x>`, with some integer `<x>`. Each one other than the latest has a
function to upgrade to the next version. Then, we define `SaveDataUnverifiedVersioned` as an enum of all
`SaveDataUnverifiedV<x>`s, and the save file can just be a `serde_json`-serialized `SaveDataUnverifiedVersioned`, and we can always get the latest `SaveDataUnverifiedV<x>` from the file.

You will note that each of these has "Unverified" in the name. That's because they don't guarantee that all of
`SaveData`'s invariants hold. There are two separate functions that can verify the latest
`SaveDataUnverifiedV<x>` into a `SaveData`. The first just checks that all the invariants hold and fails if
they don't, but the second also attempts to *fix* the invariants if they are broken. 

Finally, there's the whole TUI framework. It is built on top of Ratatui, using a model inspired by the Elm
architecture. The "Model" is the state struct, for which `TuiState` is implemented. The "View" is the
`TuiState::render` function, although it takes `&mut self` instead of `&self` because Ratatui has a concept of
"stateful widgets" which require mutable state to render. The "Update" is the combination of
`TuiState::handle_keypresses`, which converts keypresses `TuiState::Message`s, and `TuiState::handle_message` 
applies state changes based on received messages. In addition to these, there is the concept of the "external 
function". A limitation of Crossterm is that you can only poll for keyboard events in one thread, or else you
start to have issues. This means that we need to be careful to consolidate anything that polls for keyboard
input into that same thread. In order to do this, our ordinary event polling gets put in a secondary thread,
but if the TUI needs to poll for events in another context, like when taking inputs using Inquire, that needs
a way to be called from the secondary thread. ExternalFunction resolves this by creating a standardized way to pass requests and responses between the two threads.
