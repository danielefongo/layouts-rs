use std::{collections::HashMap, hash::Hash};

pub type TrigramChars = (char, char, char);
pub type BigramChars = (char, char);
pub type UnigramChar = char;

#[derive(Default, Clone)]
pub struct Corpus {
    pub chars_length: f64,
    pub word_items: Vec<(String, f64)>,
    pub unigrams: HashMap<UnigramChar, f64>,
    pub bigrams: HashMap<BigramChars, f64>,
    pub trigrams: HashMap<TrigramChars, f64>,
}

impl Corpus {
    pub fn new(word_items: impl IntoIterator<Item = (String, f64)>) -> Self {
        let mut corpus = Self::default();

        for (word, count) in word_items {
            corpus.chars_length += count * word.chars().count() as f64;
            let chars = word.chars().collect::<Vec<_>>();

            for (i, ch) in word.chars().enumerate() {
                *corpus.unigrams.entry(ch).or_insert(0.0) += count;

                if i >= 1 {
                    let bigram = (chars[i - 1], ch);
                    *corpus.bigrams.entry(bigram).or_insert(0.0) += count;
                }

                if i >= 2 {
                    let trigram = (chars[i - 2], chars[i - 1], ch);
                    *corpus.trigrams.entry(trigram).or_insert(0.0) += count;
                }
            }

            corpus.word_items.push((word, count));
        }

        corpus
    }

    pub fn top_ngram_percentile(self, percentile: f64) -> Self {
        Self {
            bigrams: top_entries_covering_percentile(&self.bigrams, percentile),
            trigrams: top_entries_covering_percentile(&self.trigrams, percentile),
            ..self
        }
    }
}

fn top_entries_covering_percentile<K: Copy + Hash + Ord>(
    entries: &HashMap<K, f64>,
    percentile: f64,
) -> HashMap<K, f64> {
    let total_count: f64 = entries.values().sum();
    let target_count = total_count * percentile / 100.0;
    let mut sorted_entries = entries.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>();
    sorted_entries.sort_by(|(_, count_a), (_, count_b)| count_b.total_cmp(count_a));

    let mut cumulative_count = 0.0;
    let mut top_entries = HashMap::new();

    for (key, count) in sorted_entries {
        if cumulative_count >= target_count {
            break;
        }

        cumulative_count += count;
        top_entries.insert(key, count);
    }

    top_entries
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use assert2::check;

    use super::*;

    #[test]
    fn it_builds_from_words() {
        let corpus = Corpus::new([("abcde".to_string(), 10.0), ("cdefg".to_string(), 5.0)]);

        check!(corpus.chars_length == 10.0 * 5.0 + 5.0 * 5.0);
        check!(corpus.word_items == vec![("abcde".to_string(), 10.0), ("cdefg".to_string(), 5.0),]);
        check!(
            into_ordered_vec(&corpus.unigrams)
                == vec![
                    ('a', 10.0),
                    ('b', 10.0),
                    ('c', 15.0),
                    ('d', 15.0),
                    ('e', 15.0),
                    ('f', 5.0),
                    ('g', 5.0),
                ]
        );
        check!(
            into_ordered_vec(&corpus.bigrams)
                == vec![
                    (('a', 'b'), 10.0),
                    (('b', 'c'), 10.0),
                    (('c', 'd'), 15.0),
                    (('d', 'e'), 15.0),
                    (('e', 'f'), 5.0),
                    (('f', 'g'), 5.0),
                ]
        );
        check!(
            into_ordered_vec(&corpus.trigrams)
                == vec![
                    (('a', 'b', 'c'), 10.0),
                    (('b', 'c', 'd'), 10.0),
                    (('c', 'd', 'e'), 15.0),
                    (('d', 'e', 'f'), 5.0),
                    (('e', 'f', 'g'), 5.0),
                ]
        );
    }

    #[test]
    fn it_keeps_top_ngrams_covering_percentile() {
        let corpus = Corpus::new([
            ("abc".to_string(), 10.0),
            ("abd".to_string(), 5.0),
            ("xyz".to_string(), 1.0),
        ]);

        let top = corpus.top_ngram_percentile(80.0);

        check!(top.word_items == corpus.word_items);
        check!(top.unigrams == corpus.unigrams);
        check!(
            into_ordered_vec(&top.bigrams)
                == vec![(('a', 'b'), 15.0), (('b', 'c'), 10.0), (('b', 'd'), 5.0)]
        );
        check!(
            into_ordered_vec(&top.trigrams)
                == vec![(('a', 'b', 'c'), 10.0), (('a', 'b', 'd'), 5.0)]
        );
    }

    fn into_ordered_vec<T: Copy + PartialOrd + Eq + Ord>(map: &HashMap<T, f64>) -> Vec<(T, f64)> {
        let mut vec: Vec<(T, f64)> = map.iter().map(|(k, v)| (*k, *v)).collect();
        vec.sort_by(|a, b| a.0.cmp(&b.0));
        vec
    }
}
