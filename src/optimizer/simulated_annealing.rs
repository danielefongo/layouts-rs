use std::collections::HashSet;

use log::info;
use rand::{Rng, prelude::*, rngs::StdRng};
use serde::Deserialize;

use crate::{
    analyzer::Analyzer,
    layout::Layout,
    metrics::Metrics,
    optimizer::{LayoutScores, OptimizableLayout, Optimizer, RunOptions},
    stats::Stats,
    swaps::{SwapMoveBuilder, SwapMoveStrategy},
    targets::Targets,
};

#[derive(Debug, Deserialize, Clone)]
pub struct SimulatedAnnealingConfig {
    pub init_temp: f64,
    pub cooling: f64,
    pub stall_accepted: usize,
}

pub struct SimulatedAnnealingOptimizer {
    analyzer: Analyzer,
    targets: Targets,
    init_temp: f64,
    cooling: f64,
    stall_accepted: usize,
}

impl SimulatedAnnealingOptimizer {
    pub fn new(analyzer: Analyzer, targets: Targets, config: SimulatedAnnealingConfig) -> Self {
        Self {
            analyzer,
            targets,
            init_temp: config.init_temp,
            cooling: config.cooling,
            stall_accepted: config.stall_accepted,
        }
    }

    fn get_stats(&self, layout: &Layout) -> Stats {
        let mut metrics = Metrics::default();
        self.analyzer.analyze(layout, &mut metrics);
        Stats::from(metrics)
    }
}

impl Optimizer for SimulatedAnnealingOptimizer {
    fn optimize(&self, layout: &Layout, opts: RunOptions) -> Layout {
        let mut rng: StdRng = StdRng::seed_from_u64(opts.seed);

        info!("Starting optimization with options: {opts}");

        let mut best_layout = OptimizableLayout::new(
            layout.clone(),
            opts.pinned,
            opts.max_swapped,
            SwapMoveBuilder::full(),
        );

        if opts.shuffle {
            best_layout.shuffle(&mut rng);
        }

        let mut scores = LayoutScores::new();
        let mut best_score =
            scores.get_or_compute(&best_layout.layout, |layout| self.score(layout));
        let mut current = best_layout.clone();
        let mut current_score = best_score;

        let mut temp = self.init_temp.max(1e-9);
        let mut stall = 0usize;

        for iteration in 0..opts.iterations {
            if current.swap_moves.is_empty() {
                break;
            }

            let mut candidate = current.clone();
            candidate.perturb(
                &mut rng,
                1,
                &[
                    (SwapMoveStrategy::Single, 10),
                    (SwapMoveStrategy::ThreeCycle, 1),
                ],
            );

            let candidate_score =
                scores.get_or_compute(&candidate.layout, |layout| self.score(layout));
            let delta = candidate_score - current_score;

            let accept = if delta <= 0.0 {
                true
            } else {
                let prob = (-delta / temp).exp();
                let random = rng.next_u64() as f64 / u64::MAX as f64;
                random < prob
            };

            if accept {
                if candidate_score < best_score {
                    best_score = candidate_score;
                    best_layout = candidate.clone();
                }
                current = candidate;
                current_score = candidate_score;
                stall = 0;
            } else {
                stall += 1;
            }

            info!("Iteration {iteration}, best score: {best_score}");

            temp *= self.cooling;

            if stall >= self.stall_accepted {
                break;
            }
        }

        while let Some(new_score) = best_layout.try_improve(&HashSet::new(), |layout| {
            let score = self.score(layout);
            (score < best_score).then_some(score)
        }) {
            best_score = new_score;
        }

        best_layout.layout().clone()
    }

    fn score(&self, layout: &Layout) -> f64 {
        self.get_stats(layout).score(&self.targets)
    }
}

#[cfg(test)]
mod tests {
    use assert2::check;

    use super::*;
    use crate::{corpus::Corpus, layout::Config, targets::*};

    #[test]
    fn it_optimizes() {
        let layout = Layout::new(
            "ab\ncd",
            &Config {
                key_size: key_size!(1.0, 1.0),
                key_centers: matrix!([
                    [coords!(0.0, 0.0), coords!(0.0, 1.0)],
                    [coords!(1.0, 0.0), coords!(1.0, 1.0)]
                ]),
                finger_assignment: matrix!(fingers, [[1, 2], [1, 2]]),
                finger_effort: matrix!([[1.0, 100.0], [100.0, 100.0]]),
                finger_home_positions: [(finger!(1), pos!(0, 0)), (finger!(2), pos!(0, 1))].into(),
            },
        )
        .unwrap();

        let corpus = Corpus::new([("c".to_string(), 10.0)]);
        let analyzer = Analyzer::new(corpus);
        let optimizer = SimulatedAnnealingOptimizer::new(
            analyzer,
            Targets {
                effort: SingleTarget {
                    value: 0.0,
                    weight: 1.0,
                    scale: 1.0,
                    tolerance: 0.0,
                    hard_limit: None,
                },
                ..default_targets()
            },
            SimulatedAnnealingConfig {
                init_temp: 100.0,
                cooling: 0.95,
                stall_accepted: 100,
            },
        );

        let optimized_layout = optimizer.optimize(
            &layout,
            RunOptions {
                iterations: 1000,
                seed: 42,
                pinned: HashSet::new(),
                max_swapped: None,
                shuffle: true,
            },
        );

        check!(optimized_layout.key_for('c').unwrap().effort == 1.0);
    }
}
