# Taskit

Taskit is a terminal-based time tracking software. No official release has been published yet, but it is in a fully workable and usable state.

## Features
- A minimal interface designed to be intuitive to use while minimizing barriers to consistent usage such as extraneous inputs
- Add time spent with either a stopwatch (`taskit time`) or by manually entering times (`taskit add`)
- Amend previous entries to correct errors
- Make comments on an entire day with daily notes
- Group entries into categories for different types of task
- Archive out-of-use categories
- Group categories into tags to track larger-scale potentially overlapping blocks of time
- Display recorded events in a TUI, including
    - Total time over events in categories and tags
    - Filters for date, category, etc

Planned changes before the first release:
- Allow renaming categories
- Exit program early by means other than `panic!()`
- Improve filter editing experience
- More "undo safety" - currently many operations cannot be undone except by manually editing the save file
- Display category-tag relationships

## Architecture
Taskit's current design groups its subcommands into `input` and `output` modules. In practice, this means that most subcommands go in `input.rs` and the TUI, which is much more complex than any other subcommand, goes in `output.rs`. Each subcommand is a function that takes the save data as input and outputs a list of `DeltaItem`s, which represent changes to the save file. After user input is complete, the save file is read a second time and the `DeltaItem`s are applied before it is written back. This architecture was chosen because it prevents two simultaneously-open instances of `taskit` from stepping on one anothers' toes. It doesn't account for two instances finishing at the same moment because that is extremely unlikely to happen accidentally, considering the small filesizes in question and the fact that a taskit instance normally only exits upon receiving some user input.

Most of the fundamental data structures are defined in `common.rs`, including a versioning system so that save files can be upgraded gracefully after a software update that modifies the program state representation--though as a failsafe, save file upgrades automatically trigger a backup to be created. `main.rs` handle argument parsing, save file I/O, dispatch to the appropriate function in `input` or `output`, and applying the received `DeltaItems`.

The versioning system is still fairly manual. In short, we maintain structs `SaveDataV1`, `SaveDataV2`, etc. and define a function that upgrades each to the next, an enum of all versions, and a function that upgrades an instance of that enum to the latest version--which is of course aliased to `SaveData`. Whenever a change is made to `SaveData`, we simply create a copy of the most recent version, increment its number by 1, and apply changes to the upgrade functions by rote.
