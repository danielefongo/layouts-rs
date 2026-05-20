use std::collections::HashSet;

use proc_macro2::TokenStream as TokenStream2;
use quip::quip;
use quote::format_ident;
use syn::{Ident, ItemUse, Result};

use crate::{
    SpanError,
    stats::{Stat, StatKind, StatSection, Stats},
};

#[derive(Clone)]
pub struct Targets {
    mod_uses: Vec<ItemUse>,
    sections: Vec<TargetSection>,
}

impl Targets {
    pub fn new(stats: &Stats) -> Result<Self> {
        let sections = stats
            .sections
            .iter()
            .filter_map(TargetSection::new)
            .collect();

        let targets = Self {
            sections,
            mod_uses: stats.mod_uses.clone(),
        };
        targets.ensure_unique_target_names()?;
        Ok(targets)
    }

    fn ensure_unique_target_names(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for target in self.iter() {
            let name = target.name.to_string();
            if !seen.insert(name.clone()) {
                return Err(target
                    .name
                    .error(&format!("duplicate target name '{name}'")));
            }
        }
        Ok(())
    }

    fn iter(&self) -> impl Iterator<Item = &Target> {
        self.sections
            .iter()
            .flat_map(|section| section.targets.iter())
    }

    pub fn make_uses(&self) -> TokenStream2 {
        quip! {
            use std::hash::Hash;
            #(#{&self.mod_uses})*
        }
    }

    pub fn make_target_types(&self) -> TokenStream2 {
        quip! {
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

            fn get_score(
                distance: f64,
                tolerance: f64,
                scale: f64,
                hard_limit: Option<f64>,
                weight: f64,
            ) -> f64 {
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
                    get_score(
                        current_value - self.value,
                        self.tolerance,
                        self.scale,
                        self.hard_limit,
                        self.weight,
                    )
                }
            }

            impl<K: Hash + Eq> MapTarget<K> {
                pub fn score(&self, current_values: &crate::map::Map<K>) -> f64 {
                    current_values
                        .entries()
                        .map(|(k, v)| {
                            let target = self.value.get(k).copied();
                            let Some(target) = target else { return 0.0; };
                            let weight = *self.weight.get(k).unwrap_or(&0.0);
                            let scale = *self.scale.get(k).unwrap_or(&1.0);
                            let tolerance = *self.tolerance.get(k).unwrap_or(&0.0);
                            let hard_limit = self.hard_limit.get(k).copied();

                            get_score(v - target, tolerance, scale, hard_limit, weight)
                        })
                        .sum()
                }
            }
        }
    }

    pub fn make_diff_kind(&self) -> TokenStream2 {
        quip! {
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

                    if improvement > 1e-9 {
                        Self::Improved(delta)
                    } else if improvement < -1e-9 {
                        Self::Regressed(delta)
                    } else {
                        Self::Unchanged(delta)
                    }
                }
            }

            impl std::fmt::Display for StatDiff {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let fmt_delta = |delta: &f64| {
                        if delta.abs() < 1e-9 {
                            format!("{:.2}", delta)
                        } else {
                            format!("{:+.2}", delta)
                        }
                    };

                    match self {
                        Self::Improved(delta) => write!(f, "\x1b[32m{}\x1b[0m", fmt_delta(delta)),
                        Self::Regressed(delta) => write!(f, "\x1b[31m{}\x1b[0m", fmt_delta(delta)),
                        Self::Changed(delta) => write!(f, "\x1b[38;5;240m{}\x1b[0m", fmt_delta(delta)),
                        Self::Unchanged(delta) => write!(f, "\x1b[33m{}\x1b[0m", fmt_delta(delta)),
                    }
                }
            }
        }
    }

    pub fn make_struct(&self) -> TokenStream2 {
        let fields = self.iter().map(Target::make_targets_field);
        quip! {
            #[derive(serde::Deserialize, Clone)]
            pub struct Targets { #(#fields)* }
        }
    }

    pub fn make_stats_diff_struct(&self, stats: &Stats) -> TokenStream2 {
        let support_modules = self.sections.iter().map(TargetSection::make_module);
        let sections = stats
            .sections
            .iter()
            .map(|section| self.make_stats_diff_section_display(section));

        quip! {
            #(#support_modules)*

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
                    #(#sections)*
                    Ok(())
                }
            }
        }
    }

    fn make_stats_diff_section_display(&self, section: &StatSection) -> TokenStream2 {
        let label = format!("{}:", section.name);
        let lines = section
            .stats
            .iter()
            .map(|stat| self.make_stats_diff_stat_display(&section.name, stat));
        quip! {
            writeln!(f, #label)?;
            #(#lines)*
        }
    }

    fn make_stats_diff_stat_display(&self, section: &Ident, stat: &Stat) -> TokenStream2 {
        if let Some(target) = self
            .iter()
            .find(|target| target.section_name == *section && target.stat_name == stat.name)
        {
            return target.make_display_line();
        }

        let stat_name = &stat.name;
        let label = stat_name.to_string();
        match &stat.kind {
            StatKind::Scalar => quip! {
                let before = self.before.#section.#stat_name;
                let after = self.after.#section.#stat_name;
                writeln!(f, "  {}: {:.2} {}", #label, after, StatDiff::Changed(after - before))?;
            },
            StatKind::Map(_) => quip! {
                writeln!(f, "  {}:", #label)?;
                let mut map_entries: Vec<_> = self.after.#section.#stat_name.entries().collect();
                map_entries.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());
                for (key, after) in map_entries {
                    let before = self.before.#section.#stat_name.get(key).copied().unwrap_or(0.0);
                    writeln!(f, "    {key}: {:.2} {}", after, StatDiff::Changed(after - before))?;
                }
            },
        }
    }

    pub fn make_targets_impl(&self) -> TokenStream2 {
        quip! {
            impl Targets {
                pub fn make_diff(&self, before: crate::stats::Stats, after: crate::stats::Stats) -> StatsDiff {
                    StatsDiff::diff(after, before, self.clone())
                }
            }
        }
    }

    pub fn make_score_impl(&self) -> TokenStream2 {
        let score_lines = self.iter().map(Target::make_score_line);
        quip! {
            impl crate::stats::Stats {
                pub fn score(&self, targets: &Targets) -> f64 {
                    let mut score = 0.0;
                    #(#score_lines)*
                    score
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct TargetSection {
    mod_uses: Vec<ItemUse>,
    name: Ident,
    targets: Vec<Target>,
}

impl TargetSection {
    pub fn new(section: &StatSection) -> Option<Self> {
        let targets: Vec<_> = section
            .stats
            .iter()
            .filter_map(|stat| Target::new(&section.name, stat))
            .collect();

        (!targets.is_empty()).then(|| Self {
            mod_uses: section.mod_uses.clone(),
            name: section.name.clone(),
            targets,
        })
    }

    fn make_module(&self) -> TokenStream2 {
        let name = &self.name;
        let methods = self.targets.iter().map(Target::make_stats_diff_function);
        let mod_uses = &self.mod_uses;
        quip! {
            #[allow(unused_imports)]
            mod #name {
                use super::*;
                #(#mod_uses)*

                #(#methods)*
            }
        }
    }
}

#[derive(Clone)]
pub struct Target {
    pub name: Ident,
    pub stat_name: Ident,
    pub section_name: Ident,
    pub label: String,
    pub kind: TargetKind,
}

impl Target {
    pub fn new(section_name: &Ident, stat: &Stat) -> Option<Self> {
        let name = stat.target_name.clone()?;
        Some(Self {
            kind: TargetKind::new(stat).ok()?,
            name: name.clone(),
            stat_name: stat.name.clone(),
            section_name: section_name.clone(),
            label: stat.name.to_string(),
        })
    }

    fn make_targets_field(&self) -> TokenStream2 {
        let name = &self.name;
        match &self.kind {
            TargetKind::Scalar => quip! { pub #name: SingleTarget, },
            TargetKind::Map(key_ty) => quip! { pub #name: MapTarget<#key_ty>, },
        }
    }

    fn make_score_line(&self) -> TokenStream2 {
        let name = &self.name;
        let section = &self.section_name;
        let stat = &self.stat_name;
        match self.kind {
            TargetKind::Scalar => quip! { score += targets.#name.score(self.#section.#stat); },
            TargetKind::Map(_) => quip! { score += targets.#name.score(&self.#section.#stat); },
        }
    }

    fn make_stats_diff_function(&self) -> TokenStream2 {
        let method = format_ident!("diff_on_{}", self.stat_name);
        let name = &self.name;
        let section = &self.section_name;
        let stat = &self.stat_name;
        match &self.kind {
            TargetKind::Scalar => quip! {
                pub(super) fn #method(diff: &StatsDiff) -> StatDiff {
                    let before = diff.before.#section.#stat;
                    let after = diff.after.#section.#stat;
                    let delta = after - before;

                    if diff.targets.#name.weight == 0.0 {
                        return StatDiff::Changed(delta);
                    }

                    let target = diff.targets.#name.value;
                    StatDiff::new(before, after, target)
                }
            },
            TargetKind::Map(key_ty) => quip! {
                pub(super) fn #method(diff: &StatsDiff, key: &#key_ty) -> StatDiff {
                    let before = diff.before.#section.#stat.get(key).copied().unwrap_or(0.0);
                    let after = diff.after.#section.#stat.get(key).copied().unwrap_or(0.0);
                    let delta = after - before;

                    let Some(target) = diff.targets.#name.value.get(key).copied() else {
                        return StatDiff::Changed(delta);
                    };

                    let weight = diff.targets.#name.weight.get(key).copied().unwrap_or(0.0);
                    if weight == 0.0 {
                        return StatDiff::Changed(delta);
                    }

                    StatDiff::new(before, after, target)
                }
            },
        }
    }

    fn make_display_line(&self) -> TokenStream2 {
        let method = format_ident!("diff_on_{}", self.stat_name);
        let label = &self.label;
        let section = &self.section_name;
        let stat = &self.stat_name;
        match &self.kind {
            TargetKind::Scalar => quip! {
                writeln!(f, "  {}: {:.2} {}", #label, self.after.#section.#stat, #section::#method(self))?;
            },
            TargetKind::Map(_) => quip! {
                writeln!(f, "  {}:", #label)?;
                let mut map_entries: Vec<_> = self.after.#section.#stat.entries().collect();
                map_entries.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());
                for (key, after) in map_entries {
                    writeln!(f, "    {key}: {:.2} {}", after, #section::#method(self, key))?;
                }
            },
        }
    }
}

#[derive(Clone)]
pub enum TargetKind {
    Scalar,
    Map(Ident),
}

impl TargetKind {
    pub fn new(stat: &Stat) -> Result<Self> {
        Ok(match &stat.kind {
            StatKind::Scalar => Self::Scalar,
            StatKind::Map(key_ty) => Self::Map(key_ty.clone()),
        })
    }
}
