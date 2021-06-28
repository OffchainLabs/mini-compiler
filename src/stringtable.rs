/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type StringId = usize;

///Maps `String`s to `usize` IDs.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StringTable {
    next_id: StringId,
    table: HashMap<String, StringId>,
    by_id: Vec<String>,
}

impl StringTable {
    pub fn new() -> Self {
        let table: HashMap<String, StringId> = HashMap::new();
        let by_id = Vec::new();
        StringTable {
            next_id: 0,
            table,
            by_id,
        }
    }

    ///Returns the `StringID` associated with `name` if it exists, if not creates a new entry and
    /// returns the newly created ID.
    pub fn get(&mut self, name: String) -> StringId {
        match self.table.get(&name) {
            Some(id) => *id,
            None => {
                let new_id = self.next_id;
                self.next_id += 1;
                self.table.insert(name.clone(), new_id);
                self.by_id.push(name);
                new_id
            }
        }
    }

    ///If an ID exists, returns it, if not returns `None`.
    pub fn get_if_exists(&self, name: &str) -> Option<StringId> {
        self.table.get(name).cloned()
    }

    ///Takes a `usize` ID and returns the associated `String`
    pub fn name_from_id(&self, name: StringId) -> &String {
        &self.by_id[name as usize]
    }
}
