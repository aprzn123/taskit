use core::slice;
use std::ops::Deref;

/// A SetVec is a Vec that enforces the guarantee that no elements will be duplicated. It doesn't
/// need anything faster than O(n) for most operations because we don't expect it to ever have 
/// more than a few dozen elements for our use case
#[derive(Clone, Default, Debug)]
pub struct SetVec<T>(Vec<T>);

impl<T: PartialEq> SetVec<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Push an element to the end of the vector if it is not already contained within.
    /// Return Ok(()) if it was pushed, Err(el) if it wasn't.
    pub fn push(&mut self, el: T) -> Result<(), T> {
        if self.0.contains(&el) {
            Err(el)
        } else {
            self.0.push(el);
            Ok(())
        }
    }

    pub fn remove(&mut self, el: &T) -> Option<T> {
        self.index_of(el).map(|n| self.0.remove(n))
    }

    pub fn swap_remove(&mut self, el: &T) -> Option<T> {
        self.index_of(el).map(|n| self.0.swap_remove(n))
    }

    pub fn contains(&self, el: &T) -> bool {
        self.0.contains(el)
    }

    pub fn as_slice(&self) -> &[T] {
        self.0.as_slice()
    }

    pub fn retain<F>(&mut self, pred: F)
        where F: FnMut(&T) -> bool
    {
        self.0.retain(pred)
    }

    fn index_of(&self, el: &T) -> Option<usize> {
        self.0.iter().enumerate().find(|(n, t)| *t == el).map(|(n, _)| n)
    }
}

impl<T> Deref for SetVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> From<SetVec<T>> for Vec<T> {
    fn from(SetVec(vec): SetVec<T>) -> Self {
        vec
    }
}

impl<T> AsRef<[T]> for SetVec<T> {
    fn as_ref(&self) -> &[T] {
        &self.0
    }
}

// TODO: update unverifiedsavedata to use an ordinary vec instead of Categories so that i can remove these
impl<'de, T> serde::Deserialize<'de> for SetVec<T> 
where T: serde::Deserialize<'de> + std::cmp::PartialEq {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        Vec::deserialize(deserializer).map(|mut v| { v.dedup(); Self(v) })
    }
}

impl<T> serde::Serialize for SetVec<T>
where T: serde::Serialize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
            self.0.serialize(serializer)
    }
}

impl<T> FromIterator<T> for SetVec<T>
where T: PartialEq {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = iter.into_iter().collect::<Vec<_>>();
        v.dedup();
        Self(v)
    }
}
