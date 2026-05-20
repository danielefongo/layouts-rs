use std::{hash::Hash, iter::FromIterator};

use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub struct Map<K>(IndexMap<K, f64>);

impl<K: Eq + Hash> Default for Map<K> {
    fn default() -> Self {
        Self(IndexMap::new())
    }
}

impl<K: Eq + Hash> PartialEq for Map<K> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<K: Eq + Hash> Map<K> {
    pub fn add(&mut self, key: K, value: f64) {
        *self.0.entry(key).or_default() += value;
    }

    pub fn get(&self, key: &K) -> Option<&f64> {
        self.0.get(key)
    }

    pub fn entries(&self) -> MapEntries<impl Iterator<Item = (&K, f64)>> {
        MapEntries::new(self.0.iter().map(|(k, v)| (k, *v)))
    }
}

impl<K: Eq + Hash> FromIterator<(K, f64)> for Map<K> {
    fn from_iter<T: IntoIterator<Item = (K, f64)>>(iter: T) -> Self {
        Self(IndexMap::from_iter(iter))
    }
}

impl<'a, K: Eq + Hash + Clone + 'a> FromIterator<(&'a K, f64)> for Map<K> {
    fn from_iter<T: IntoIterator<Item = (&'a K, f64)>>(iter: T) -> Self {
        Self(IndexMap::from_iter(
            iter.into_iter().map(|(k, v)| (k.clone(), v)),
        ))
    }
}

pub struct MapEntries<I>(I);

impl<I> MapEntries<I> {
    fn new(iter: I) -> Self {
        Self(iter)
    }
}

impl<'a, K, I> Iterator for MapEntries<I>
where
    K: 'a,
    I: Iterator<Item = (&'a K, f64)>,
{
    type Item = (&'a K, f64);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a, K, I> MapEntries<I>
where
    K: 'a,
    I: Iterator<Item = (&'a K, f64)>,
{
    pub fn normalize(self, total: f64) -> MapEntries<impl Iterator<Item = (&'a K, f64)>> {
        MapEntries::new(self.0.map(move |(k, v)| (k, 100.0 * v / total)))
    }

    pub fn filter<P>(self, predicate: P) -> MapEntries<impl Iterator<Item = (&'a K, f64)>>
    where
        P: FnMut(&(&'a K, f64)) -> bool,
    {
        MapEntries::new(self.0.filter(predicate))
    }

    pub fn values(self) -> impl Iterator<Item = f64> {
        self.0.map(|(_, v)| v)
    }

    pub fn topn(self, n: usize) -> Map<K>
    where
        K: Eq + Hash + Clone,
    {
        let mut entries: Vec<_> = self.0.collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        entries.into_iter().take(n).collect()
    }
}

#[cfg(test)]
mod tests {
    use assert2::check;

    use super::*;

    #[test]
    fn it_adds_and_retrieves_entries() {
        let mut map = Map::default();
        map.add("a", 1.0);
        map.add("b", 2.0);
        map.add("a", 3.0);

        check!(map.get(&"a") == Some(&4.0));
        check!(map.get(&"b") == Some(&2.0));
        check!(map.get(&"c") == None);

        let entries = map.entries().collect::<Vec<_>>();

        check!(entries == vec![(&"a", 4.0), (&"b", 2.0)]);
    }

    #[test]
    fn it_normalizes_entries() {
        let map = Map::from_iter([("a", 3.0), ("b", 7.0)]);
        let total = 10.0;
        let normalized = map.entries().normalize(total).collect::<Vec<_>>();

        check!(normalized == vec![(&"a", 30.0), (&"b", 70.0)]);
    }

    #[test]
    fn it_gets_values() {
        let map = Map::from_iter([("a", 3.0), ("b", 7.0)]);
        let values = map.entries().values().collect::<Vec<_>>();

        check!(values == vec![3.0, 7.0]);
    }

    #[test]
    fn it_gets_topn_entries() {
        let map = Map::from_iter([("a", 3.0), ("b", 7.0), ("c", 5.0)]);
        let top2 = map.entries().topn(2);

        check!(top2 == Map::from_iter([("b", 7.0), ("c", 5.0)]));
    }
}
