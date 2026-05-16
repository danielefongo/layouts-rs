use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use rand::{Rng, prelude::*};
use rayon::prelude::*;

use crate::{
    layout::Layout,
    matrix::Pos,
    swaps::{SwapMoveBuilder, SwapMoveStrategy, SwapMoves, SwapSampler},
};

pub mod hill_climb;
pub mod simulated_annealing;

pub use hill_climb::*;
pub use simulated_annealing::*;

const MAX_PERTURB_ATTEMPTS: usize = 30;

#[derive(Default)]
pub struct LayoutScores {
    scores: HashMap<u64, f64>,
}

impl LayoutScores {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
        }
    }

    pub fn get_or_compute(&mut self, layout: &Layout, score_fn: impl Fn(&Layout) -> f64) -> f64 {
        let hash = layout.hash();
        if let Some(score) = self.scores.get(&hash) {
            *score
        } else {
            let score = score_fn(layout);
            self.scores.insert(hash, score);
            score
        }
    }
}

#[derive(Clone)]
struct OptimizableLayout {
    initial_layout: Layout,
    layout: Layout,
    max_swapped: Option<usize>,
    swap_moves: Arc<SwapMoves>,
    movable_positions: Arc<Vec<Pos>>,
}

impl OptimizableLayout {
    pub fn new(
        layout: Layout,
        pinned_chars: HashSet<char>,
        max_swapped: Option<usize>,
        swap_move_builder: SwapMoveBuilder,
    ) -> Self {
        let positions: Vec<Pos> = layout
            .keys()
            .filter(|key| !pinned_chars.contains(&key.ch))
            .map(|key| key.position)
            .collect();

        let movable_positions = positions.clone();

        Self {
            initial_layout: layout.clone(),
            layout,
            max_swapped,
            swap_moves: Arc::new(swap_move_builder.build(&positions)),
            movable_positions: Arc::new(movable_positions),
        }
    }

    fn diff(&self) -> usize {
        Self::diff_between(&self.layout, &self.initial_layout)
    }

    fn diff_between(layout: &Layout, initial_layout: &Layout) -> usize {
        layout
            .keys()
            .zip(initial_layout.keys())
            .filter(|(k1, k2)| k1.ch != k2.ch)
            .count()
    }

    fn try_improve(
        &mut self,
        tabu_list: &HashSet<u64>,
        score_check: impl Fn(&Layout) -> Option<f64> + Sync,
    ) -> Option<f64> {
        let initial_layout = &self.initial_layout;
        let current_layout = &self.layout;

        let moves = self.swap_moves.moves();

        let (score, best_swap) = moves
            .par_iter()
            .filter_map(|swap_move| {
                let mut candidate_layout = current_layout.clone();
                swap_move.apply(&mut candidate_layout);

                if tabu_list.contains(&candidate_layout.hash()) {
                    return None;
                }

                let score = if let Some(max) = self.max_swapped {
                    if Self::diff_between(&candidate_layout, initial_layout) <= max {
                        score_check(&candidate_layout)
                    } else {
                        None
                    }
                } else {
                    score_check(&candidate_layout)
                };

                score.map(|score| (score, swap_move))
            })
            .min_by(|(s1, _), (s2, _)| s1.partial_cmp(s2).unwrap_or(Ordering::Equal))?;

        best_swap.apply(&mut self.layout);

        Some(score)
    }

    fn shuffle<RNG: Rng + ?Sized>(&mut self, rng: &mut RNG) {
        if self.movable_positions.len() <= 1 {
            return;
        }

        let max_to_swap = self
            .max_swapped
            .unwrap_or(self.movable_positions.len())
            .min(self.movable_positions.len());

        let mut positions = self.movable_positions.to_vec();
        positions.partial_shuffle(rng, max_to_swap);
        let selected = &positions[..max_to_swap];

        let mut chars: Vec<char> = selected
            .iter()
            .map(|pos| {
                self.layout
                    .char_at(pos)
                    .expect("movable position must contain a key")
            })
            .collect();

        chars.shuffle(rng);

        for (pos, ch) in selected.iter().zip(chars) {
            self.layout.set_char(pos, ch);
        }
    }

    pub fn perturb<RNG: Rng + ?Sized>(
        &mut self,
        rng: &mut RNG,
        mut n: usize,
        weights: &[(SwapMoveStrategy, usize)],
    ) {
        let swaps_number = self.swap_moves.len();
        n = n.min(swaps_number);

        let mut applied = 0;
        for _ in 0..MAX_PERTURB_ATTEMPTS {
            if applied >= n {
                break;
            }

            let swap = self.swap_moves.sample(rng, weights);
            swap.apply(&mut self.layout);

            if let Some(max) = self.max_swapped
                && self.diff() > max
            {
                swap.apply(&mut self.layout);
            } else {
                applied += 1;
            }
        }
    }

    fn layout(&self) -> &Layout {
        &self.layout
    }
}

pub struct RunOptions {
    pub iterations: usize,
    pub seed: u64,
    pub pinned: HashSet<char>,
    pub max_swapped: Option<usize>,
    pub shuffle: bool,
}

impl fmt::Display for RunOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "iterations: {}, seed: {}, pinned: {:?}, max_swapped: {:?}, shuffle: {}",
            self.iterations, self.seed, self.pinned, self.max_swapped, self.shuffle
        )
    }
}

pub trait Optimizer {
    fn optimize(&self, layout: &Layout, opts: RunOptions) -> Layout;
    fn score(&self, layout: &Layout) -> f64;
}

#[cfg(test)]
mod layout_scores_tests {
    use std::cell::Cell;

    use assert2::check;

    use super::*;

    #[test]
    fn it_computes_score_when_layout_is_not_cached() {
        let layout = make_layout();
        let calls = Cell::new(0);
        let mut scores = LayoutScores::new();

        let score = scores.get_or_compute(&layout, |_| {
            calls.set(calls.get() + 1);
            42.0
        });

        check!(score == 42.0);
        check!(calls.get() == 1);
    }

    #[test]
    fn it_reuses_cached_score_for_same_layout() {
        let layout = make_layout();
        let calls = Cell::new(0);
        let mut scores = LayoutScores::new();

        let first = scores.get_or_compute(&layout, |_| {
            calls.set(calls.get() + 1);
            42.0
        });
        let second = scores.get_or_compute(&layout, |_| {
            calls.set(calls.get() + 1);
            13.0
        });

        check!(first == 42.0);
        check!(second == 42.0);
        check!(calls.get() == 1);
    }

    #[test]
    fn it_computes_separate_scores_for_different_layouts() {
        let layout = make_layout();
        let mut swapped = layout.clone();
        swapped.swap_chars(&pos!(0, 0), &pos!(1, 1));
        let calls = Cell::new(0);
        let mut scores = LayoutScores::new();

        let first = scores.get_or_compute(&layout, |_| {
            calls.set(calls.get() + 1);
            42.0
        });
        let second = scores.get_or_compute(&swapped, |_| {
            calls.set(calls.get() + 1);
            13.0
        });

        check!(first == 42.0);
        check!(second == 13.0);
        check!(calls.get() == 2);
    }
}

#[cfg(test)]
mod optimizable_layout_tests {
    use assert2::check;
    use rand::rng;

    use super::*;

    const STRATEGIES: &[(SwapMoveStrategy, usize); 3] = &[
        (SwapMoveStrategy::Single, 20),
        (SwapMoveStrategy::Column, 1),
        (SwapMoveStrategy::Row, 1),
    ];

    #[test]
    fn it_does_not_improve_layout_when_no_swap_gives_better_score() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::full());
        let result = optimizable.try_improve(&[].into(), |layout| {
            let score = layout_effort_score(layout);
            if score < 0.0 { Some(score) } else { None }
        });

        check!(result == None);
    }

    #[test]
    fn it_does_not_improve_layout_if_no_swaps_available() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::default());
        let result = optimizable.try_improve(&[].into(), |layout| {
            let score = layout.key_for('c').unwrap().effort;
            if score < 100.0 { Some(score) } else { None }
        });

        check!(result == None);
    }

    #[test]
    fn it_improves_layout_by_applying_the_best_swap() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::full());
        let score = optimizable
            .try_improve(&[].into(), |layout| {
                let score = layout.key_for('c').unwrap().effort;
                if score < 100.0 { Some(score) } else { None }
            })
            .unwrap();

        check!(score == 1.0);
        check!(optimizable.layout().key_for('c').unwrap().effort == 1.0);
    }

    #[test]
    fn it_improves_layout_without_moving_pinned_chars() {
        let mut optimizable = OptimizableLayout::new(
            make_layout(),
            ['a', 'c'].into(),
            None,
            SwapMoveBuilder::full(),
        );

        while optimizable
            .try_improve(&[].into(), |layout| {
                let score = layout.key_for('d').unwrap().effort;
                if score < 200.0 { Some(score) } else { None }
            })
            .is_some()
        {}

        check!(optimizable.layout().key_for('a').unwrap().position == pos!(0, 0));
        check!(optimizable.layout().key_for('c').unwrap().position == pos!(1, 0));
        check!(optimizable.layout().key_for('d').unwrap().position == pos!(0, 1));
    }

    #[test]
    fn it_does_not_improve_layout_when_only_one_char_is_unpinned() {
        let mut optimizable = OptimizableLayout::new(
            make_layout(),
            ['a', 'b', 'c'].into(),
            None,
            SwapMoveBuilder::full(),
        );
        let result = optimizable.try_improve(&[].into(), |layout| {
            let score = layout.key_for('d').unwrap().effort;
            if score < 200.0 { Some(score) } else { None }
        });

        check!(result == None);
    }

    #[test]
    fn it_does_not_improve_layout_when_swap_exceeds_max_swapped() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), Some(0), SwapMoveBuilder::full());
        let result = optimizable.try_improve(&[].into(), |layout| {
            let score = layout.key_for('c').unwrap().effort;
            if score < 100.0 { Some(score) } else { None }
        });

        check!(result == None);
    }

    #[test]
    fn it_improves_layout_when_swap_is_within_max_swapped() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), Some(2), SwapMoveBuilder::full());
        let score = optimizable
            .try_improve(&[].into(), |layout| {
                let score = layout.key_for('c').unwrap().effort;
                if score < 100.0 { Some(score) } else { None }
            })
            .unwrap();

        check!(score == 1.0);
        check!(optimizable.layout().key_for('c').unwrap().effort == 1.0);
    }

    #[test]
    fn it_does_not_improve_layout_after_convergence() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::full());

        let first = optimizable.try_improve(&[].into(), |layout| {
            let score = layout.key_for('c').unwrap().effort;
            if score < 100.0 { Some(score) } else { None }
        });
        check!(first == Some(1.0));

        let second = optimizable.try_improve(&[].into(), |layout| {
            let score = layout.key_for('c').unwrap().effort;
            if score < 1.0 { Some(score) } else { None }
        });
        check!(second == None);
    }

    #[test]
    fn it_does_not_improve_layout_if_tabu_list_contains_layout() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::full());

        optimizable.try_improve(&[].into(), |layout| {
            let score = layout.key_for('c').unwrap().effort;
            if score < 100.0 { Some(score) } else { None }
        });

        let tabu_list: HashSet<u64> = [optimizable.layout().hash()].into_iter().collect();

        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::full());

        let second = optimizable.try_improve(&tabu_list, |layout| {
            let score = layout.key_for('c').unwrap().effort;
            if score < 1.0 { Some(score) } else { None }
        });
        check!(second == None);
    }

    #[test]
    fn it_perturbs_layout() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::full());
        let before: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();

        optimizable.perturb(&mut get_rng(), 10, STRATEGIES);

        let after: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();
        check!(before != after);
    }

    #[test]
    fn it_does_not_perturb_layout_when_all_chars_are_pinned() {
        let mut optimizable = OptimizableLayout::new(
            make_layout(),
            ['a', 'b', 'c', 'd'].into(),
            None,
            SwapMoveBuilder::full(),
        );
        let before: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();

        optimizable.perturb(&mut get_rng(), 10, STRATEGIES);

        let after: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();
        check!(before == after);
    }

    #[test]
    fn it_does_not_perturb_layout_beyond_max_swapped() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), Some(0), SwapMoveBuilder::full());

        optimizable.perturb(&mut get_rng(), 10, STRATEGIES);

        check!(optimizable.diff() == 0);
    }

    #[test]
    fn it_perturbs_layout_within_max_swapped() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), Some(4), SwapMoveBuilder::full());

        optimizable.perturb(&mut get_rng(), 100, STRATEGIES);

        check!(optimizable.diff() > 0);
        check!(optimizable.diff() <= 4);
    }

    #[test]
    fn it_shuffles_layout() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), None, SwapMoveBuilder::full());
        let before: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();

        optimizable.shuffle(&mut get_rng());

        let after: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();
        check!(before != after);
    }

    #[test]
    fn it_does_not_shuffle_layout_when_all_chars_are_pinned() {
        let mut optimizable = OptimizableLayout::new(
            make_layout(),
            ['a', 'b', 'c', 'd'].into(),
            None,
            SwapMoveBuilder::full(),
        );
        let before: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();

        optimizable.shuffle(&mut get_rng());

        let after: Vec<char> = optimizable.layout().keys().map(|k| k.ch).collect();
        check!(before == after);
    }

    #[test]
    fn it_does_not_shuffle_layout_beyond_max_swapped() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), Some(0), SwapMoveBuilder::full());

        optimizable.shuffle(&mut get_rng());

        check!(optimizable.diff() == 0);
    }

    #[test]
    fn it_shuffles_layout_within_max_swapped() {
        let mut optimizable =
            OptimizableLayout::new(make_layout(), [].into(), Some(4), SwapMoveBuilder::full());

        optimizable.shuffle(&mut get_rng());

        check!(optimizable.diff() > 0);
        check!(optimizable.diff() <= 4);
    }

    fn get_rng() -> StdRng {
        StdRng::seed_from_u64(rng().next_u64())
    }

    fn layout_effort_score(layout: &Layout) -> f64 {
        layout.keys().map(|k| k.effort).sum()
    }
}

#[cfg(test)]
fn make_layout() -> Layout {
    Layout::new(
        "ab\ncd",
        &crate::layout::Config {
            key_size: key_size!(1.0, 1.0),
            key_centers: matrix!([
                [coords!(0.0, 0.0), coords!(0.0, 1.0)],
                [coords!(1.0, 0.0), coords!(1.0, 1.0)]
            ]),
            finger_assignment: matrix!(fingers, [[1, 2], [1, 2]]),
            finger_effort: matrix!([[1.0, 50.0], [100.0, 200.0]]),
            finger_home_positions: [(finger!(1), pos!(0, 0)), (finger!(2), pos!(0, 1))].into(),
        },
    )
    .unwrap()
}
