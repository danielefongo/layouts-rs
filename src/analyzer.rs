use std::collections::HashMap;

use derive_more::Constructor;

use crate::{
    corpus::Corpus,
    layout::{Key, Layout},
    metrics::MetricsCollector,
    ngrams::{Bigram, Trigram, Unigram},
};

const ASCII_LOOKUP_SIZE: usize = 128;
pub struct KeyLookup<'a> {
    ascii: [Option<&'a Key>; ASCII_LOOKUP_SIZE],
    extra: HashMap<char, &'a Key>,
}

impl<'a> KeyLookup<'a> {
    pub fn get(&self, ch: &char) -> Option<&'a Key> {
        let code = *ch as usize;
        if code < ASCII_LOOKUP_SIZE
            && let Some(key) = self.ascii[code]
        {
            return Some(key);
        }
        self.extra.get(ch).copied()
    }
}

impl<'a> FromIterator<&'a Key> for KeyLookup<'a> {
    fn from_iter<I: IntoIterator<Item = &'a Key>>(iter: I) -> Self {
        let mut ascii: [Option<&'a Key>; ASCII_LOOKUP_SIZE] = [None; ASCII_LOOKUP_SIZE];
        let mut extra: HashMap<char, &'a Key> = HashMap::new();

        for key in iter {
            let code = key.ch as usize;
            if code < ASCII_LOOKUP_SIZE {
                ascii[code] = Some(key);
            } else {
                extra.insert(key.ch, key);
            }
        }

        Self { ascii, extra }
    }
}

#[derive(Clone, Constructor)]
pub struct Analyzer {
    corpus: Corpus,
}

impl Analyzer {
    pub fn analyze(&self, layout: &Layout, metrics: &mut impl MetricsCollector) {
        let lookup: KeyLookup = layout.keys().collect();

        for (char, count) in self.corpus.unigrams.iter() {
            let Some(key) = lookup.get(char) else {
                continue;
            };

            metrics.collect_unigram(&Unigram::new(key), *count);
        }

        for ((char1, char2), count) in self.corpus.bigrams.iter() {
            let Some((key1, key2)) = lookup.get(char1).zip(lookup.get(char2)) else {
                continue;
            };

            metrics.collect_bigram(&Bigram::new(key1, key2), *count);
        }

        for ((char1, char2, char3), count) in self.corpus.trigrams.iter() {
            let Some(((key1, key2), key3)) = lookup
                .get(char1)
                .zip(lookup.get(char2))
                .zip(lookup.get(char3))
            else {
                continue;
            };

            metrics.collect_trigram(&Trigram::new(key1, key2, key3), *count);
        }
    }
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use mockall::predicate::eq;
    use rstest::rstest;

    use crate::{corpus::Corpus, layout::fixtures::qwerty, metrics::MockMetricsCollector};

    use super::*;

    #[rstest]
    fn it_does_not_collect_metrics_for_not_existing_keys(qwerty: Layout) {
        let corpus = Corpus {
            chars_length: 100.0,
            word_items: vec![],
            unigrams: [('[', 10.0)].into(),
            bigrams: [(('[', '['), 10.0)].into(),
            trigrams: [(('[', '[', '['), 10.0)].into(),
        };

        let analyzer = Analyzer::new(corpus);

        analyzer.analyze(&qwerty, &mut MockMetricsCollector::new())
    }

    #[rstest]
    fn it_collects_metrics(qwerty: Layout) {
        let corpus = Corpus {
            chars_length: 100.0,
            word_items: vec![],
            unigrams: [('a', 1.0)].into(),
            bigrams: [(('a', 'a'), 2.0)].into(),
            trigrams: [(('a', 'a', 'a'), 3.0)].into(),
        };

        let key = qwerty.key_for('a').unwrap();
        let unigram = Unigram::new(key);
        let bigram = Bigram::new(key, key);
        let trigram = Trigram::new(key, key, key);

        let mut metrics = MockMetricsCollector::new();
        metrics
            .expect_collect_unigram()
            .with(eq(unigram), eq(1.0))
            .once()
            .return_const(());
        metrics
            .expect_collect_bigram()
            .with(eq(bigram), eq(2.0))
            .once()
            .return_const(());
        metrics
            .expect_collect_trigram()
            .with(eq(trigram), eq(3.0))
            .once()
            .return_const(());

        let analyzer = Analyzer::new(corpus);

        analyzer.analyze(&qwerty, &mut metrics)
    }

    #[rstest]
    fn it_generates_metrics(qwerty: Layout) {
        #[derive(Default, Debug, PartialEq)]
        struct FakeMetricsCollector {
            unigrams: f64,
            bigrams: f64,
            trigrams: f64,
        }

        impl MetricsCollector for FakeMetricsCollector {
            fn collect_unigram(&mut self, _unigram: &Unigram, count: f64) {
                self.unigrams += count;
            }

            fn collect_bigram(&mut self, _bigram: &Bigram, count: f64) {
                self.bigrams += count;
            }

            fn collect_trigram(&mut self, _trigram: &Trigram, count: f64) {
                self.trigrams += count;
            }
        }

        let corpus = Corpus {
            chars_length: 100.0,
            word_items: vec![],
            unigrams: [('a', 1.0)].into(),
            bigrams: [(('a', 'a'), 2.0)].into(),
            trigrams: [(('a', 'a', 'a'), 3.0)].into(),
        };

        let mut fake_metrics = FakeMetricsCollector::default();

        let analyzer = Analyzer::new(corpus);

        analyzer.analyze(&qwerty, &mut fake_metrics);

        check!(
            fake_metrics
                == FakeMetricsCollector {
                    unigrams: 1.0,
                    bigrams: 2.0,
                    trigrams: 3.0,
                }
        );
    }
}
