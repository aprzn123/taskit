/*
use std::collections::{HashMap, HashSet};

use crate::common::{Event, SaveData, error::TaskitResult};

pub struct InvariantsChecker {
    save_data: SaveData,
    suggestions_required: Option<Vec<InvariantRepair>>,
    invariant_checking: Option<BrokenInvariant>,
}

pub struct BrokenInvariant {
    kind: BrokenInvariantKind,
    suggestions: Vec<InvariantRepair>,
}

pub enum BrokenInvariantKind {
    CategoryNotUnique { archived: bool, main: bool, name: String }, // manual
    TagNotUnique(String),                                           // automatic
    TagContainsSpace(String),                                       // sometimes automatic
    NonCategoryTagMapKey(String),                                   // automatic
    NonTagTagMapValue(String),                                      // automatic
    NonexistentEventCategory(Event, usize),                         // manual
    NonexistentEventTag(Event, usize),                              // manual
    EventTagDescMismatch(Event, usize),                             // automatic
}

// This construction
struct VerifiedInvariantRepair(InvariantRepair);

#[derive(Clone)]
pub enum InvariantRepair {
    DedupCategoryLeaveArchived,
    DedupCategoryLeaveMain,
    DedupTag,
    RenameTagTo(String),
}

impl From<BrokenInvariantKind> for BrokenInvariant {
    fn from(value: BrokenInvariantKind) -> Self {
        Self {
            kind: value,
            suggestions: vec![],
        }
    }
}

impl BrokenInvariantKind {
    fn with_suggestions(self, suggestions: Vec<InvariantRepair>) -> BrokenInvariant {
        BrokenInvariant {
            kind: self,
            suggestions,
        }
    }
}

impl InvariantsChecker {
    pub fn new(save_data: SaveData) -> Self {
        Self { save_data, suggestions_required: None }
    }

    pub fn verify(&self, repair: InvariantRepair) -> Option<VerifiedInvariantRepair> {
        todo!("verify");
        Some(VerifiedInvariantRepair(repair))
    }

    fn query_fix<F, E>(&mut self, select_fix: F, broken_invariant: BrokenInvariant, suggestions_required: Option<Vec<InvariantRepair>>) -> Result<InvariantRepair, E> 
    where F: Fn(BrokenInvariant, &Self) -> Result<VerifiedInvariantRepair, E> {
        self.suggestions_required = suggestions_required.clone();
        Ok(select_fix(broken_invariant, &self)?.0)
    }

    // allow directive to temporarily suppress annoying warnings
    #[allow(unreachable_code)]
    pub fn fix_all<F, E>(
        mut self,
        select_fix: F,
    ) -> Result<SaveData, E> 
    where F: Fn(BrokenInvariant, &Self) -> Result<VerifiedInvariantRepair, E>,
    {
        // check for non-unique categories
        {
            let all_categories: HashMap<String, Vec<bool>> = self.save_data.categories.options.iter().cloned().fold(
                self.save_data.archived_categories.options.iter().cloned().fold(
                    HashMap::new(),
                    |mut map, s| {
                        map.entry(s).or_default().push(true);
                        map
                    }
                ),
                |mut map, s| {
                    map.entry(s).or_default().push(false);
                    map
                }
            );
            for (category, kinds) in all_categories {
                if kinds.len() > 1 {
                    // duplicate detected!
                    let in_archive = kinds.contains(&true);
                    let in_main = kinds.contains(&false);
                    let suggestions: Vec<InvariantRepair> = in_archive
                        .then_some(InvariantRepair::DedupCategoryLeaveArchived)
                        .into_iter()
                        .chain(in_main.then_some(InvariantRepair::DedupCategoryLeaveMain).into_iter())
                        .collect();
                    let repair = self.query_fix(
                        &select_fix, 
                        BrokenInvariantKind::CategoryNotUnique { archived: in_archive, main: in_main, name: category.clone() }.with_suggestions(suggestions.clone()), 
                        Some(suggestions)
                    )?;
                    self.save_data.categories.options.retain(|e| e != &category);
                    self.save_data.archived_categories.options.retain(|e| e != &category);
                    match repair {
                        InvariantRepair::DedupCategoryLeaveArchived => self.save_data.archived_categories.options.push(category),
                        InvariantRepair::DedupCategoryLeaveMain => self.save_data.categories.options.push(category),
                        _ => unreachable!(),
                    }
                }
            }
        }
        // check for non-unique tags
        {
            let all_tags: HashMap<String, usize> = self.save_data.categories.options.iter().cloned().fold(
                HashMap::new(),
                |mut map, s| {
                    *map.entry(s).or_default() += 1;
                    map
                }
            );
            for (tag, count) in all_tags {
                if count > 1 {
                    // duplicate detected!
                    let suggestions = vec![InvariantRepair::DedupTag];
                    let repair = self.query_fix(
                        &select_fix, 
                        BrokenInvariantKind::TagNotUnique(tag.clone()).with_suggestions(suggestions.clone()), 
                        Some(suggestions)
                    )?;
                    self.save_data.tags.retain(|e| e != &tag);
                    match repair {
                        InvariantRepair::DedupTag => self.save_data.tags.push(tag),
                        _ => unreachable!(),
                    }
                }
            }
        }
        todo!("check for spaces in tags");
        // check for spaces in tags
        for tag in self.save_data.tags.clone() {
            if tag.contains(' ') {
                let past_tags = todo!();
                let repair = self.query_fix(select_fix, BrokenInvariant { kind: BrokenInvariantKind::TagContainsSpace(tag), suggestions: todo!()}, None);
            }
        }
        todo!("check for non-category keys in tag_map");
        todo!("check for non-tag values in tag_map");
        todo!("check for events with nonexistent categories");
        todo!("check for events with nonexistent tags");
        todo!("check for events where the tags in the description don't match the tags in the field");
        Ok(self.save_data)
    }
}
*/
