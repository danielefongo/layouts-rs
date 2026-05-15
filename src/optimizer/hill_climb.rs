use std::collections::HashSet;

use derive_more::Constructor;
use log::{debug, info};
use rand::{prelude::*, rngs::StdRng};

use crate::{
    analyzer::Analyzer,
    layout::Layout,
    metrics::Metrics,
    optimizer::{LayoutScores, OptimizableLayout, Optimizer, RunOptions},
    stats::Stats,
    swaps::SwapMoveBuilder,
    targets::Targets,
};

#[derive(Constructor)]
pub struct HillClimbOptimizer {
    analyzer: Analyzer,
    targets: Targets,
}

impl HillClimbOptimizer {
    fn get_stats(&self, layout: &Layout) -> Stats {
        let mut metrics = Metrics::default();
        self.analyzer.analyze(layout, &mut metrics);
        Stats::from(metrics)
    }
}

impl Optimizer for HillClimbOptimizer {
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

        debug!("Layout score {}", best_score);

        for iteration in 0..opts.iterations {
            let mut candidate = best_layout.clone();

            if iteration > 0 {
                candidate.shuffle(&mut rng);
            }

            let mut best_candidate = candidate.clone();

            let mut current_score =
                scores.get_or_compute(&candidate.layout, |layout| self.score(layout));

            let mut tabu_list: HashSet<u64> = HashSet::new();

            let mut step = 0;
            while let Some(best_iteration_score) =
                candidate.try_improve(&tabu_list, |layout| Some(self.score(layout)))
            {
                tabu_list.insert(candidate.layout.hash());

                if best_iteration_score < current_score {
                    current_score = best_iteration_score;
                    best_candidate = candidate.clone();
                    step = 0;
                } else {
                    step += 1;
                }

                if step > candidate.swap_moves.len() / 2 {
                    break;
                }
            }

            if current_score < best_score {
                best_score = current_score;
                best_layout = best_candidate;
            }

            info!("Iteration {iteration}, best score: {best_score}");
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
        let optimizer = HillClimbOptimizer::new(
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
        );

        let optimized_layout = optimizer.optimize(
            &layout,
            RunOptions {
                iterations: 10,
                seed: 42,
                pinned: HashSet::new(),
                max_swapped: None,
                shuffle: false,
            },
        );

        check!(optimized_layout.key_for('c').unwrap().effort == 1.0);
    }
}
