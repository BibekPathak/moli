use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReflectorId(u64);

impl ReflectorId {
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reflector<K> {
    id: ReflectorId,
    key: K,
}

impl<K: Clone> Reflector<K> {
    pub fn id(&self) -> ReflectorId {
        self.id
    }

    pub fn key(&self) -> K {
        self.key.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DomPtr<K>(K);

impl<K: Clone> DomPtr<K> {
    pub fn new(key: K) -> Self {
        Self(key)
    }

    pub fn key(&self) -> K {
        self.0.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DomRoot<K> {
    reflector: Reflector<K>,
}

impl<K: Clone> DomRoot<K> {
    pub fn reflector_id(&self) -> ReflectorId {
        self.reflector.id()
    }
}

#[derive(Debug, Clone)]
pub struct ReflectorRegistry<K> {
    next_id: u64,
    ids_by_key: HashMap<K, ReflectorId>,
    keys_by_id: HashMap<ReflectorId, K>,
}

impl<K> Default for ReflectorRegistry<K> {
    fn default() -> Self {
        Self {
            next_id: 0,
            ids_by_key: HashMap::new(),
            keys_by_id: HashMap::new(),
        }
    }
}

impl<K> ReflectorRegistry<K>
where
    K: Clone + Eq + Hash,
{
    pub fn intern(&mut self, key: K) -> Reflector<K> {
        if let Some(existing) = self.ids_by_key.get(&key).copied() {
            return Reflector { id: existing, key };
        }

        self.next_id += 1;
        let id = ReflectorId(self.next_id);
        self.ids_by_key.insert(key.clone(), id);
        self.keys_by_id.insert(id, key.clone());
        Reflector { id, key }
    }

    pub fn existing(&self, key: K) -> Option<Reflector<K>> {
        self.ids_by_key
            .get(&key)
            .copied()
            .map(|id| Reflector { id, key })
    }

    pub fn root(&mut self, ptr: DomPtr<K>) -> DomRoot<K> {
        DomRoot {
            reflector: self.intern(ptr.key()),
        }
    }

    pub fn key_for_id(&self, id: ReflectorId) -> Option<K> {
        self.keys_by_id.get(&id).cloned()
    }

    pub fn rekey(&mut self, old_key: K, new_key: K) -> Option<ReflectorId> {
        if old_key == new_key {
            return self.ids_by_key.get(&old_key).copied();
        }
        if self.ids_by_key.contains_key(&new_key) {
            return None;
        }

        let id = self.ids_by_key.remove(&old_key)?;
        self.ids_by_key.insert(new_key.clone(), id);
        self.keys_by_id.insert(id, new_key);
        Some(id)
    }

    pub fn rekey_matching(
        &mut self,
        mut replacement: impl FnMut(&K) -> Option<K>,
    ) -> Option<usize> {
        let replacements = self
            .ids_by_key
            .keys()
            .filter_map(|old_key| replacement(old_key).map(|new_key| (old_key.clone(), new_key)))
            .collect::<Vec<_>>();
        let mut destinations = HashSet::with_capacity(replacements.len());
        if replacements
            .iter()
            .any(|(_, new_key)| !destinations.insert(new_key.clone()))
            || replacements.iter().any(|(old_key, new_key)| {
                old_key != new_key && self.ids_by_key.contains_key(new_key)
            })
        {
            return None;
        }
        for (old_key, new_key) in &replacements {
            self.rekey(old_key.clone(), new_key.clone())?;
        }
        Some(replacements.len())
    }

    pub fn len(&self) -> usize {
        self.ids_by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids_by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::ReflectorRegistry;

    #[test]
    fn reflector_registry_reuses_identity_for_same_key() {
        let mut registry = ReflectorRegistry::default();

        let first = registry.intern(7_u32);
        let second = registry.intern(7_u32);

        assert_eq!(first.id(), second.id());
        assert_eq!(first.key(), second.key());
        assert_eq!(first.id().raw(), 1);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.key_for_id(first.id()), Some(7_u32));
    }

    #[test]
    fn reflector_registry_supports_non_copy_keys() {
        let mut registry = ReflectorRegistry::default();

        let first = registry.intern("highlight(name)".to_owned());
        let second = registry.intern("highlight(name)".to_owned());
        let other = registry.intern("highlight(other)".to_owned());

        assert_eq!(first.id(), second.id());
        assert_ne!(first.id(), other.id());
        assert_eq!(
            registry.key_for_id(first.id()),
            Some("highlight(name)".to_owned())
        );
    }

    #[test]
    fn reflector_registry_allocates_distinct_identity_for_distinct_keys() {
        let mut registry = ReflectorRegistry::default();

        let first = registry.intern(1_u32);
        let second = registry.intern(2_u32);

        assert_ne!(first.id(), second.id());
        assert_eq!(registry.existing(1_u32), Some(first));
        assert_eq!(registry.existing(2_u32), Some(second));
        assert!(registry.existing(99_u32).is_none());
        assert_eq!(registry.key_for_id(first.id()), Some(1_u32));
        assert_eq!(registry.key_for_id(second.id()), Some(2_u32));
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
    }

    #[test]
    fn reflector_registry_rekeys_an_existing_identity_without_changing_its_id() {
        let mut registry = ReflectorRegistry::default();
        let reflector = registry.intern(7_u32);

        assert_eq!(registry.rekey(7, 9), Some(reflector.id()));
        assert_eq!(registry.existing(7), None);
        assert_eq!(
            registry.existing(9).map(|entry| entry.id()),
            Some(reflector.id())
        );
        assert_eq!(registry.key_for_id(reflector.id()), Some(9));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn reflector_registry_does_not_rekey_over_an_existing_identity() {
        let mut registry = ReflectorRegistry::default();
        let first = registry.intern(7_u32);
        let second = registry.intern(9_u32);

        assert_eq!(registry.rekey(7, 9), None);
        assert_eq!(registry.existing(7), Some(first));
        assert_eq!(registry.existing(9), Some(second));
    }

    #[test]
    fn reflector_registry_rekeys_a_matching_key_set_atomically() {
        let mut registry = ReflectorRegistry::default();
        let first = registry.intern((7_u32, 1_u32));
        let second = registry.intern((9_u32, 1_u32));
        let unchanged = registry.intern((11_u32, 2_u32));

        assert_eq!(
            registry.rekey_matching(|(value, generation)| {
                (*generation == 1).then_some((*value, 3))
            }),
            Some(2)
        );
        assert_eq!(
            registry.existing((7, 3)).map(|entry| entry.id()),
            Some(first.id())
        );
        assert_eq!(
            registry.existing((9, 3)).map(|entry| entry.id()),
            Some(second.id())
        );
        assert_eq!(registry.existing((11, 2)), Some(unchanged));
    }
}
