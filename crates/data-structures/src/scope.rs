use std::{
    collections::HashMap,
    hash::{BuildHasher, Hash, RandomState},
};

/// A single scope.
pub struct Scope<K, V, S = RandomState>(pub HashMap<K, V, S>);

impl<K: Eq + Hash, V, S: Default + BuildHasher> Scope<K, V, S> {
    /// Creates a new `Scope`.
    pub fn new() -> Self {
        Self(HashMap::default())
    }

    /// Returns a reference to the value associated with the key.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.get(key)
    }

    /// Returns a mutable reference to the value associated with the key.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.0.get_mut(key)
    }

    /// Inserts a value into the scope.
    pub fn insert(&mut self, key: K, value: V) {
        self.0.insert(key, value);
    }
}

impl<K: Eq + Hash, V, S: Default + BuildHasher> Default for Scope<K, V, S> {
    fn default() -> Self {
        Self::new()
    }
}

/// A list of scopes.
pub struct Scopes<K, V, S = RandomState>(Vec<Scope<K, V, S>>);

impl<K: Eq + Hash, V, S: Default + BuildHasher> Scopes<K, V, S> {
    /// Creates a new `Scopes` with a single scope.
    pub fn new() -> Self {
        Self(vec![Scope::new()])
    }

    /// Enters a new scope.
    pub fn enter(&mut self) {
        self.0.push(Scope::new());
    }

    /// Exits the current scope, returning it.
    pub fn exit(&mut self) -> Scope<K, V, S> {
        self.0.pop().unwrap()
    }

    /// Returns a reference to the current scope.
    pub fn current(&self) -> &Scope<K, V, S> {
        self.0.last().unwrap()
    }

    /// Returns a mutable reference to the current scope.
    pub fn current_mut(&mut self) -> &mut Scope<K, V, S> {
        self.0.last_mut().unwrap()
    }

    /// Returns a reference to the first value found in the scopes.
    pub fn find(&self, key: &K) -> Option<&V> {
        self.position(key).map(|(_, v)| v)
    }

    /// Returns a mutable reference to the first value found in the scopes.
    pub fn find_mut(&mut self, key: &K) -> Option<&mut V> {
        self.position_mut(key).map(|(_, v)| v)
    }

    /// Returns a reference to the first value found in the scopes, alongside its index.
    pub fn position(&self, key: &K) -> Option<(usize, &V)> {
        for (i, scope) in self.0.iter().enumerate().rev() {
            if let Some(value) = scope.get(key) {
                return Some((i, value));
            }
        }
        None
    }

    /// Returns a mutable reference to the first value found in the scopes, alongside its index.
    pub fn position_mut(&mut self, key: &K) -> Option<(usize, &mut V)> {
        for (i, scope) in self.0.iter_mut().enumerate().rev() {
            if let Some(value) = scope.get_mut(key) {
                return Some((i, value));
            }
        }
        None
    }

    /// Inserts a value into the current scope.
    pub fn insert(&mut self, key: K, value: V) {
        self.current_mut().insert(key, value);
    }
}

impl<K: Eq + Hash, V, S: Default + BuildHasher> Default for Scopes<K, V, S> {
    fn default() -> Self {
        Self::new()
    }
}
