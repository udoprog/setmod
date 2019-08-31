use failure::Error;
use hashbrown::{hash_map, HashMap, HashSet};
use std::{fmt, sync::Arc};

/// Trait over something that has a matchable pattern.
pub trait Matchable {
    /// Get the key for the matchable element.
    fn key(&self) -> &Key;

    /// Get the pattern for the matchable element.
    fn pattern(&self) -> &Pattern;
}

pub struct Matcher<T>
where
    T: Matchable,
{
    /// All commands.
    all: HashMap<Key, Arc<T>>,
    /// Commands indexed by name.
    by_name: HashSet<Key>,
    /// Regular expression commands indexed by channel.
    by_channel_regex: HashMap<String, HashSet<Key>>,
}

impl<T> Matcher<T>
where
    T: Matchable,
{
    pub(crate) fn new() -> Self {
        Self {
            all: Default::default(),
            by_name: Default::default(),
            by_channel_regex: Default::default(),
        }
    }

    /// Test if we contain the given key.
    pub(crate) fn contains_key(&self, key: &Key) -> bool {
        self.all.contains_key(key)
    }

    /// Insert the given value.
    pub(crate) fn insert(&mut self, key: Key, value: Arc<T>) {
        match value.pattern() {
            Pattern::Name => {
                self.by_name.insert(key.clone());
            }
            Pattern::Regex { .. } => {
                self.by_channel_regex
                    .entry(key.channel.clone())
                    .or_default()
                    .insert(key.clone());
            }
        }

        self.all.insert(key, value);
    }

    /// Remove the given value.
    pub(crate) fn remove(&mut self, key: &Key) -> Option<Arc<T>> {
        if let Some(value) = self.all.remove(key) {
            match value.pattern() {
                Pattern::Name => {
                    self.by_name.remove(key);
                }
                Pattern::Regex { .. } => {
                    self.by_channel_regex
                        .entry(key.channel.clone())
                        .or_default()
                        .remove(&key);
                }
            }

            return Some(value);
        }

        None
    }

    /// Get an iterator over all the values.
    pub(crate) fn iter(&self) -> hash_map::Iter<'_, Key, Arc<T>> {
        self.all.iter()
    }

    /// Get an iterator over all the values.
    pub(crate) fn values(&self) -> hash_map::Values<'_, Key, Arc<T>> {
        self.all.values()
    }

    /// Get the underlying key.
    pub(crate) fn get(&self, key: &Key) -> Option<&Arc<T>> {
        self.all.get(key)
    }

    /// Modify the given element with the given pattern.
    pub(crate) fn modify_with_pattern<F>(&mut self, key: Key, pattern: Option<regex::Regex>, m: F)
    where
        T: Clone,
        F: FnOnce(&mut T, Pattern),
    {
        let Self {
            all,
            by_channel_regex,
            by_name,
        } = self;

        let existing = match all.get_mut(&key) {
            Some(existing) => existing,
            None => return,
        };

        let pattern = if let Some(pattern) = pattern {
            if let Pattern::Name = existing.pattern() {
                by_name.remove(&key);

                by_channel_regex
                    .entry(key.channel.clone())
                    .or_default()
                    .insert(key);
            }

            Pattern::Regex { pattern }
        } else {
            if let Pattern::Regex { .. } = existing.pattern() {
                by_channel_regex
                    .entry(key.channel.clone())
                    .or_default()
                    .remove(&key);

                by_name.insert(key);
            } else {
                // NB: nothing to do.
                return;
            }

            Pattern::Name
        };

        let mut new = (**existing).clone();
        m(&mut new, pattern);
        *existing = Arc::new(new);
    }

    /// Resolve the given command.
    pub fn resolve<'a>(
        &self,
        channel: &str,
        first: Option<&str>,
        full: &'a str,
    ) -> Option<(&Arc<T>, Captures<'a>)> {
        if let Some(first) = first {
            let key = Key::new(channel, first);

            if self.by_name.contains(&key) {
                if let Some(command) = self.get(&key) {
                    return Some((command, Default::default()));
                }
            }
        }

        if let Some(keys) = self.by_channel_regex.get(channel) {
            for key in keys {
                if let Some(command) = self.get(key) {
                    if let Pattern::Regex { pattern } = command.pattern() {
                        if let Some(captures) = pattern.captures(full) {
                            let captures = Captures {
                                captures: Some(captures),
                            };
                            return Some((command, captures));
                        }
                    }
                }
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub struct Key {
    pub channel: String,
    pub name: String,
}

impl Key {
    pub fn new(channel: &str, name: &str) -> Self {
        Self {
            channel: channel.to_string(),
            name: name.to_lowercase(),
        }
    }
}

/// How to match the given value.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum Pattern {
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "regex")]
    Regex {
        #[serde(serialize_with = "serialize_regex")]
        pattern: regex::Regex,
    },
}

impl Pattern {
    /// Convert a database pattern into a matchable pattern here.
    pub fn from_db(pattern: Option<impl AsRef<str>>) -> Result<Self, Error> {
        Ok(match pattern {
            Some(pattern) => Pattern::Regex {
                pattern: regex::Regex::new(pattern.as_ref())?,
            },
            None => Pattern::Name,
        })
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pattern::Name => "*name*".fmt(fmt),
            Pattern::Regex { pattern } => pattern.fmt(fmt),
        }
    }
}

/// Serialize a regular expression.
fn serialize_regex<S>(regex: &regex::Regex, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.collect_str(regex)
}

#[derive(Debug, Default)]
pub struct Captures<'a> {
    captures: Option<regex::Captures<'a>>,
}

impl serde::Serialize for Captures<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap as _;

        let mut m = serializer.serialize_map(self.captures.as_ref().map(|c| c.len()))?;

        if let Some(captures) = &self.captures {
            for (i, g) in captures.iter().enumerate() {
                m.serialize_entry(&i, &g.map(|m| m.as_str()))?;
            }
        }

        m.end()
    }
}
