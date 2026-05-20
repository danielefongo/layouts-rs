use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quip::quip;
use syn::{Error, Item, ItemMod, Result, parse_macro_input, spanned::Spanned};

use crate::{metrics::Metrics, stats::Stats, targets::Targets};
mod metrics;
mod stats;
mod targets;

pub(crate) trait SpanError {
    fn error(&self, msg: &str) -> Error;
}

impl<T: Spanned> SpanError for T {
    fn error(&self, msg: &str) -> Error {
        Error::new(self.span(), msg)
    }
}

#[proc_macro_attribute]
pub fn scoring(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemMod);
    match try_expand_scoring(item) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn try_expand_scoring(module: ItemMod) -> Result<TokenStream2> {
    let (_, items) = module.content.expect("module should have content");

    let mut mod_items = Vec::new();
    let mut metrics_module = None;
    let mut stats_module = None;
    for item in items {
        match item {
            Item::Mod(m) if m.ident == "metrics" => metrics_module = Some(m),
            Item::Mod(m) if m.ident == "stats" => stats_module = Some(m),
            item => mod_items.push(item),
        }
    }

    let metrics = metrics_module
        .map(Metrics::try_from)
        .transpose()?
        .ok_or(module.ident.error("missing 'metrics' module"))?;
    let stats = stats_module
        .map(Stats::try_from)
        .transpose()?
        .ok_or(module.ident.error("missing 'stats' module"))?;
    let targets = Targets::new(&stats)?;
    let used_metrics = stats.used_metrics();

    Ok(quip! {
        #(#mod_items)*

        pub mod metrics {
            #{metrics.make_uses()}
            #{metrics.make_trait()}
            #{metrics.make_struct(&used_metrics)}
            #{metrics.make_trait_impl(&used_metrics)}
        }

        pub mod stats {
            #{stats.make_uses()}
            #{stats.make_struct()}
            #{stats.make_from_metrics()}
            #{stats.make_display()}
        }

        pub mod targets {
            #![allow(unused_imports)]
            #{targets.make_uses()}
            #{targets.make_target_types()}
            #{targets.make_diff_kind()}
            #{targets.make_struct()}
            #{targets.make_stats_diff_struct(&stats)}
            #{targets.make_targets_impl()}
            #{targets.make_score_impl()}
        }
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use prettyplease::unparse;
    use syn::{File, parse2};

    use super::*;

    fn normalize(tokens: TokenStream2) -> String {
        unparse(&parse2::<File>(tokens).unwrap())
    }

    #[test]
    fn it_expands_macro() {
        let input: ItemMod = parse2(quip! {
            mod scoring_fixture {
                pub type Finger = usize;

                mod metrics {
                    use crate::ngrams::*;

                    fn unigram_total(_unigram: &Unigram, count: f64) -> Option<f64> { Some(count) }
                    fn unigram_by_finger(_unigram: &Unigram, count: f64) -> Option<(Finger, f64)> { Some((0, count)) }
                    fn bigram_total(_bigram: &Bigram, count: f64) -> Option<f64> { Some(count * 2.0) }
                    fn bigram_by_kind(_bigram: &Bigram, _kind: &BigramKind, count: f64) -> Option<(Finger, f64)> { Some((1, count)) }
                    fn trigram_total(_trigram: &Trigram, count: f64) -> Option<f64> { Some(count * 3.0) }
                    fn trigram_by_kind(_trigram: &Trigram, _kind: &TrigramKind, count: f64) -> Option<(Finger, f64)> { Some((2, count)) }
                }

                mod stats {
                    use crate::map::*;

                    #[stat_let]
                    fn total(total: &UnigramTotal) -> f64 { *total }

                    mod general {
                        use crate::map::*;

                        #[target(score_target)]
                        fn score(lets: &StatLets) -> f64 { lets.total }

                        #[target(finger_target)]
                        fn fingers(by_finger: &UnigramByFinger) -> Map<Finger> { by_finger.clone() }
                    }
                }
            }
        })
        .unwrap();

        let generated = normalize(try_expand_scoring(input).unwrap());
        let expected = normalize(quip! {
            pub type Finger = usize;

            pub mod metrics {
                use crate::ngrams::*;

                #[cfg_attr(test, mockall::automock)]
                pub trait MetricsCollector {
                    fn collect_unigram(&mut self, unigram: &crate::ngrams::Unigram, count: f64);
                    fn collect_bigram(&mut self, bigram: &crate::ngrams::Bigram, count: f64);
                    fn collect_trigram(&mut self, trigram: &crate::ngrams::Trigram, count: f64);
                }

                pub type UnigramTotal = f64;
                pub type UnigramByFinger = crate::map::Map<Finger>;
                pub type BigramTotal = f64;
                pub type BigramByKind = crate::map::Map<Finger>;
                pub type TrigramTotal = f64;
                pub type TrigramByKind = crate::map::Map<Finger>;

                #[derive(Default)]
                pub struct Metrics {
                    pub unigram_total: UnigramTotal,
                    pub unigram_by_finger: UnigramByFinger,
                }

                impl Metrics {
                    fn unigram_total(_unigram: &Unigram, count: f64) -> Option<f64> { Some(count) }
                    fn unigram_by_finger(_unigram: &Unigram, count: f64) -> Option<(Finger, f64)> { Some((0, count)) }
                }

                impl MetricsCollector for Metrics {
                    #[allow(unused_variables)]
                    fn collect_unigram(&mut self, unigram: &crate::ngrams::Unigram, count: f64) {
                        if let Some(value) = Self::unigram_total(unigram, count) { self.unigram_total += value; }
                        if let Some((key, value)) = Self::unigram_by_finger(unigram, count) { self.unigram_by_finger.add(key, value); }
                    }

                    #[allow(unused_variables)]
                    fn collect_bigram(&mut self, bigram: &crate::ngrams::Bigram, count: f64) {
                        bigram.for_each_kind(|kind| { });
                    }

                    #[allow(unused_variables)]
                    fn collect_trigram(&mut self, trigram: &crate::ngrams::Trigram, count: f64) {
                        trigram.for_each_kind(|kind| { });
                    }
                }
            }

            pub mod stats {
                use crate::map::*;
                use crate::metrics::*;

                #[derive(Debug, Default, PartialEq)]
                pub struct StatLets { pub total: f64, }

                impl StatLets {
                    fn total(total: &UnigramTotal) -> f64 { *total }
                }

                pub mod general {
                    use crate::map::*;
                    use super::StatLets;
                    use crate::metrics::*;

                    #[derive(Debug, Default, PartialEq)]
                    pub struct Stats {
                        pub score: f64,
                        pub fingers: crate::map::Map<Finger>,
                    }

                    pub fn score(lets: &StatLets) -> f64 { lets.total }
                    pub fn fingers(by_finger: &UnigramByFinger) -> Map<Finger> { by_finger.clone() }
                }

                #[derive(Debug, Default, PartialEq)]
                pub struct Stats { pub general: general::Stats, }

                impl From<crate::metrics::Metrics> for Stats {
                    fn from(metrics: crate::metrics::Metrics) -> Self {
                        let lets = crate::stats::StatLets { total: crate::stats::StatLets::total(&metrics.unigram_total), };
                        Self { general: general::Stats { score: general::score(&lets), fingers: general::fingers(&metrics.unigram_by_finger), }, }
                    }
                }

                impl std::fmt::Display for Stats {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        writeln!(f, "general:")?;
                        writeln!(f, "  score: {:.2}", self.general.score)?;
                        writeln!(f, "  fingers:")?;
                        let mut entries: Vec<_> = self.general.fingers.entries().collect();
                        entries.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());
                        for (k, v) in entries { writeln!(f, "    {}: {:.2}", k, v)?; }
                        Ok(())
                    }
                }
            }

            pub mod targets {
                #![allow(unused_imports)]
                use std::hash::Hash;
                use crate::map::*;

                const HARD_LIMIT_PENALTY: f64 = 1_000_000.0;

                #[derive(serde::Deserialize, Clone)]
                pub struct SingleTarget {
                    pub value: f64,
                    #[serde(default)]
                    pub weight: f64,
                    pub scale: f64,
                    #[serde(default)]
                    pub tolerance: f64,
                    #[serde(default)]
                    pub hard_limit: Option<f64>,
                }

                #[derive(serde::Deserialize, Clone)]
                pub struct MapTarget<K: Hash + Eq> {
                    pub value: indexmap::IndexMap<K, f64>,
                    #[serde(default = "indexmap::IndexMap::new")]
                    pub weight: indexmap::IndexMap<K, f64>,
                    pub scale: indexmap::IndexMap<K, f64>,
                    #[serde(default = "indexmap::IndexMap::new")]
                    pub tolerance: indexmap::IndexMap<K, f64>,
                    #[serde(default = "indexmap::IndexMap::new")]
                    pub hard_limit: indexmap::IndexMap<K, f64>,
                }

                fn get_score(distance: f64, tolerance: f64, scale: f64, hard_limit: Option<f64>, weight: f64) -> f64 {
                    let distance = distance.abs();
                    let tolerance = tolerance.max(0.0);
                    let scale = scale.abs().max(f64::EPSILON);
                    let excess = (distance - tolerance).max(0.0);
                    let hard_penalty = match hard_limit {
                        Some(limit) if distance > limit.max(tolerance) => HARD_LIMIT_PENALTY,
                        _ => 0.0,
                    };
                    hard_penalty + weight * (excess / scale)
                }

                impl SingleTarget {
                    pub fn score(&self, current_value: f64) -> f64 {
                        get_score(current_value - self.value, self.tolerance, self.scale, self.hard_limit, self.weight)
                    }
                }

                impl<K: Hash + Eq> MapTarget<K> {
                    pub fn score(&self, current_values: &crate::map::Map<K>) -> f64 {
                        current_values.entries().map(|(k, v)| {
                            let target = self.value.get(k).copied();
                            let Some(target) = target else { return 0.0; };
                            let weight = *self.weight.get(k).unwrap_or(&0.0);
                            let scale = *self.scale.get(k).unwrap_or(&1.0);
                            let tolerance = *self.tolerance.get(k).unwrap_or(&0.0);
                            let hard_limit = self.hard_limit.get(k).copied();
                            get_score(v - target, tolerance, scale, hard_limit, weight)
                        }).sum()
                    }
                }

                pub enum StatDiff {
                    Improved(f64),
                    Regressed(f64),
                    Changed(f64),
                    Unchanged(f64),
                }

                impl StatDiff {
                    pub fn new(before: f64, after: f64, target: f64) -> Self {
                        let delta = after - before;
                        let gap_before = (before - target).abs();
                        let gap_after = (after - target).abs();
                        let improvement = gap_before - gap_after;
                        if improvement > 1e-9 { Self::Improved(delta) } else if improvement < -1e-9 { Self::Regressed(delta) } else { Self::Unchanged(delta) }
                    }
                }

                impl std::fmt::Display for StatDiff {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        let fmt_delta = |delta: &f64| {
                            if delta.abs() < 1e-9 { format!("{:.2}", delta) } else { format!("{:+.2}", delta) }
                        };
                        match self {
                            Self::Improved(delta) => write!(f, "\x1b[32m{}\x1b[0m", fmt_delta(delta)),
                            Self::Regressed(delta) => write!(f, "\x1b[31m{}\x1b[0m", fmt_delta(delta)),
                            Self::Changed(delta) => write!(f, "\x1b[38;5;240m{}\x1b[0m", fmt_delta(delta)),
                            Self::Unchanged(delta) => write!(f, "\x1b[33m{}\x1b[0m", fmt_delta(delta)),
                        }
                    }
                }

                #[derive(serde::Deserialize, Clone)]
                pub struct Targets {
                    pub score_target: SingleTarget,
                    pub finger_target: MapTarget<Finger>,
                }

                #[allow(unused_imports)]
                mod general {
                    use super::*;
                    use crate::map::*;

                    pub(super) fn diff_on_score(diff: &StatsDiff) -> StatDiff {
                        let before = diff.before.general.score;
                        let after = diff.after.general.score;
                        let delta = after - before;
                        if diff.targets.score_target.weight == 0.0 { return StatDiff::Changed(delta); }
                        let target = diff.targets.score_target.value;
                        StatDiff::new(before, after, target)
                    }

                    pub(super) fn diff_on_fingers(diff: &StatsDiff, key: &Finger) -> StatDiff {
                        let before = diff.before.general.fingers.get(key).copied().unwrap_or(0.0);
                        let after = diff.after.general.fingers.get(key).copied().unwrap_or(0.0);
                        let delta = after - before;
                        let Some(target) = diff.targets.finger_target.value.get(key).copied() else { return StatDiff::Changed(delta); };
                        let weight = diff.targets.finger_target.weight.get(key).copied().unwrap_or(0.0);
                        if weight == 0.0 { return StatDiff::Changed(delta); }
                        StatDiff::new(before, after, target)
                    }
                }

                pub struct StatsDiff {
                    before: crate::stats::Stats,
                    after: crate::stats::Stats,
                    targets: Targets,
                }

                impl StatsDiff {
                    pub fn diff(after: crate::stats::Stats, before: crate::stats::Stats, targets: Targets) -> Self {
                        Self { before, after, targets }
                    }
                }

                impl std::fmt::Display for StatsDiff {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        writeln!(f, "general:")?;
                        writeln!(f, "  {}: {:.2} {}", "score", self.after.general.score, general::diff_on_score(self))?;
                        writeln!(f, "  {}:", "fingers")?;
                        let mut map_entries: Vec<_> = self.after.general.fingers.entries().collect();
                        map_entries.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());
                        for (key, after) in map_entries {
                            writeln!(f, "    {key}: {:.2} {}", after, general::diff_on_fingers(self, key))?;
                        }
                        Ok(())
                    }
                }

                impl Targets {
                    pub fn make_diff(&self, before: crate::stats::Stats, after: crate::stats::Stats) -> StatsDiff {
                        StatsDiff::diff(after, before, self.clone())
                    }
                }

                impl crate::stats::Stats {
                    pub fn score(&self, targets: &Targets) -> f64 {
                        let mut score = 0.0;
                        score += targets.score_target.score(self.general.score);
                        score += targets.finger_target.score(&self.general.fingers);
                        score
                    }
                }
            }
        });

        assert_eq!(generated, expected);
    }
}
