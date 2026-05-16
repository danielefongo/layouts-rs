use std::collections::HashSet;

use log::warn;

use crate::{layout::Layout, matrix::Pos};

pub trait SwapSampler {
    fn sample(
        &self,
        rng: &mut (impl rand::Rng + ?Sized),
        weights: &[(SwapMoveStrategy, usize)],
    ) -> SwapMove;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwapMoveStrategy {
    Single,
    Column,
    Row,
    ThreeCycle,
}

#[derive(Default)]
pub struct SwapMoveBuilder {
    strategies: Vec<SwapMoveStrategy>,
}

impl SwapMoveBuilder {
    pub fn full() -> Self {
        Self::new(&[
            SwapMoveStrategy::Single,
            SwapMoveStrategy::Column,
            SwapMoveStrategy::Row,
            SwapMoveStrategy::ThreeCycle,
        ])
    }

    pub fn new(strategies: &[SwapMoveStrategy]) -> Self {
        let mut builder = Self::default();
        let mut seen = HashSet::new();

        for &strategy in strategies {
            if seen.insert(strategy) {
                builder.strategies.push(strategy);
            }
        }

        builder
    }

    pub fn build(&self, positions: &[Pos]) -> SwapMoves {
        let mut strategies: Vec<(SwapMoveStrategy, Vec<SwapMove>)> = Vec::new();
        let mut len = 0usize;

        for &strategy in &self.strategies {
            let moves = match strategy {
                SwapMoveStrategy::Single => Self::single_moves(positions),
                SwapMoveStrategy::Column => Self::column_moves(positions),
                SwapMoveStrategy::Row => Self::row_moves(positions),
                SwapMoveStrategy::ThreeCycle => Self::three_cycle_moves(positions),
            };

            if !moves.is_empty() {
                len += moves.len();
                strategies.push((strategy, moves));
            }
        }

        SwapMoves { len, strategies }
    }

    fn single_moves(positions: &[Pos]) -> Vec<SwapMove> {
        let mut moves = Vec::new();
        for (i, &p1) in positions.iter().enumerate() {
            for &p2 in positions.iter().skip(i + 1) {
                moves.push(SwapMove(vec![(p1, p2)]));
            }
        }
        moves
    }

    fn three_cycle_moves(positions: &[Pos]) -> Vec<SwapMove> {
        let mut moves = Vec::new();
        for (i, &a) in positions.iter().enumerate() {
            for (j, &b) in positions.iter().enumerate().skip(i + 1) {
                for &c in positions.iter().skip(j + 1) {
                    moves.push(SwapMove(vec![(a, c), (b, c)]));
                    moves.push(SwapMove(vec![(a, b), (c, b)]));
                }
            }
        }
        moves
    }

    fn column_moves(positions: &[Pos]) -> Vec<SwapMove> {
        Self::group_moves(positions, |pos| pos.c)
    }

    fn row_moves(positions: &[Pos]) -> Vec<SwapMove> {
        Self::group_moves(positions, |pos| pos.r)
    }

    fn group_moves(positions: &[Pos], key_fn: fn(Pos) -> usize) -> Vec<SwapMove> {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<usize, Vec<Pos>> = BTreeMap::new();
        for &pos in positions {
            groups.entry(key_fn(pos)).or_default().push(pos);
        }

        let keys: Vec<usize> = groups.keys().copied().collect();
        let mut moves = Vec::new();

        for (i, &k1) in keys.iter().enumerate() {
            for &k2 in keys.iter().skip(i + 1) {
                let g1 = &groups[&k1];
                let g2 = &groups[&k2];
                let pairs: Vec<(Pos, Pos)> = g1.iter().copied().zip(g2.iter().copied()).collect();
                if !pairs.is_empty() {
                    moves.push(SwapMove(pairs));
                }
            }
        }

        moves
    }
}

#[derive(Clone, Debug, Default)]
pub struct SwapMoves {
    strategies: Vec<(SwapMoveStrategy, Vec<SwapMove>)>,
    len: usize,
}

impl SwapMoves {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn moves(&self) -> Vec<&SwapMove> {
        self.strategies
            .iter()
            .flat_map(|(_, moves)| moves)
            .collect()
    }
}

impl SwapSampler for SwapMoves {
    fn sample(
        &self,
        rng: &mut (impl rand::Rng + ?Sized),
        weights: &[(SwapMoveStrategy, usize)],
    ) -> SwapMove {
        use rand::RngExt;

        let strategies: Vec<_> = self
            .strategies
            .iter()
            .filter_map(|(strategy, moves)| {
                weights
                    .iter()
                    .find(|(k, _)| k == strategy)
                    .map(|(_, w)| (moves, *w))
            })
            .collect();

        if weights.len() > self.strategies.len() {
            warn!(
                "Swaps: weights provided for {} strategies, but only {} are available",
                weights.len(),
                self.strategies.len()
            );
        }

        let total_weight = strategies.iter().map(|(_, w)| w).sum();

        if total_weight == 0 {
            return SwapMove(vec![]);
        }

        let pick = rng.random_range(0..total_weight);
        let mut cumulative = 0;

        for (moves, weight) in &strategies {
            cumulative += weight;
            if pick < cumulative {
                return moves[rng.random_range(0..moves.len())].clone();
            }
        }

        SwapMove(vec![])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SwapMove(pub Vec<(Pos, Pos)>);

impl SwapMove {
    pub fn apply(&self, layout: &mut Layout) {
        for &(p1, p2) in &self.0 {
            layout.swap_chars(&p1, &p2);
        }
    }
}

#[cfg(test)]
mod swap_move_tests {
    use assert2::check;

    use crate::layout::Config;

    use super::*;

    fn layout() -> Layout {
        Layout::new(
            "ab\ncd",
            &Config {
                key_size: key_size!(1.0, 1.0),
                key_centers: matrix!([
                    [coords!(0.0, 0.0), coords!(0.0, 1.0)],
                    [coords!(1.0, 0.0), coords!(1.0, 1.0)]
                ]),
                finger_assignment: matrix!(fingers, [[1, 2], [1, 2]]),
                finger_effort: matrix!([[1.0, 2.0], [3.0, 4.0]]),
                finger_home_positions: [(finger!(1), pos!(0, 0)), (finger!(2), pos!(0, 1))].into(),
            },
        )
        .unwrap()
    }

    #[test]
    fn it_applies_all_swaps_in_order() {
        let mut layout = layout();
        let swap = SwapMove(vec![(pos!(0, 0), pos!(0, 1)), (pos!(1, 0), pos!(0, 1))]);

        swap.apply(&mut layout);

        check!(layout.key_for('a').unwrap().position == pos!(1, 0));
        check!(layout.key_for('b').unwrap().position == pos!(0, 0));
        check!(layout.key_for('c').unwrap().position == pos!(0, 1));
        check!(layout.key_for('d').unwrap().position == pos!(1, 1));
    }
}

#[cfg(test)]
mod single_moves_tests {
    use assert2::check;

    use super::*;

    #[test]
    fn it_builds() {
        let positions = vec![pos!(0, 0), pos!(1, 0), pos!(0, 1)];
        let swap_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Single]).build(&positions);
        check!(
            swap_moves.moves()
                == vec![
                    &SwapMove(vec![(pos!(0, 0), pos!(1, 0))]),
                    &SwapMove(vec![(pos!(0, 0), pos!(0, 1))]),
                    &SwapMove(vec![(pos!(1, 0), pos!(0, 1))]),
                ]
        );
    }

    #[test]
    fn it_builds_from_single_position() {
        let swap_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Single]).build(&[pos!(0, 0)]);
        check!(swap_moves.is_empty());
    }

    #[test]
    fn it_builds_from_empty() {
        let swap_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Single]).build(&[]);
        check!(swap_moves.is_empty());
    }
}

#[cfg(test)]
mod three_cycle_moves_tests {
    use assert2::check;

    use super::*;

    #[test]
    fn it_builds() {
        let positions = vec![pos!(0, 0), pos!(1, 0), pos!(0, 1)];
        let cycle_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::ThreeCycle]).build(&positions);
        check!(
            cycle_moves.moves()
                == vec![
                    &SwapMove(vec![(pos!(0, 0), pos!(0, 1)), (pos!(1, 0), pos!(0, 1))]),
                    &SwapMove(vec![(pos!(0, 0), pos!(1, 0)), (pos!(0, 1), pos!(1, 0))]),
                ]
        );
    }

    #[test]
    fn it_builds_from_two_positions() {
        let cycle_moves =
            SwapMoveBuilder::new(&[SwapMoveStrategy::ThreeCycle]).build(&[pos!(0, 0), pos!(1, 0)]);
        check!(cycle_moves.is_empty());
    }
}

#[cfg(test)]
mod column_moves_tests {
    use assert2::check;

    use super::*;

    #[test]
    fn it_builds() {
        let positions = vec![pos!(0, 0), pos!(1, 0), pos!(0, 1), pos!(1, 1)];
        let col_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Column]).build(&positions);
        check!(
            col_moves.moves()
                == vec![&SwapMove(vec![
                    (pos!(0, 0), pos!(0, 1)),
                    (pos!(1, 0), pos!(1, 1)),
                ])]
        );
    }

    #[test]
    fn it_builds_with_n_columns() {
        let positions = vec![
            pos!(0, 0),
            pos!(1, 0),
            pos!(0, 1),
            pos!(1, 1),
            pos!(0, 2),
            pos!(1, 2),
        ];
        let col_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Column]).build(&positions);
        check!(
            col_moves.moves()
                == vec![
                    &SwapMove(vec![(pos!(0, 0), pos!(0, 1)), (pos!(1, 0), pos!(1, 1))]),
                    &SwapMove(vec![(pos!(0, 0), pos!(0, 2)), (pos!(1, 0), pos!(1, 2))]),
                    &SwapMove(vec![(pos!(0, 1), pos!(0, 2)), (pos!(1, 1), pos!(1, 2))]),
                ]
        );
    }

    #[test]
    fn it_builds_with_n_rows() {
        let positions = vec![
            pos!(0, 0),
            pos!(1, 0),
            pos!(2, 0),
            pos!(0, 1),
            pos!(1, 1),
            pos!(2, 1),
        ];
        let col_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Column]).build(&positions);
        check!(
            col_moves.moves()
                == vec![&SwapMove(vec![
                    (pos!(0, 0), pos!(0, 1)),
                    (pos!(1, 0), pos!(1, 1)),
                    (pos!(2, 0), pos!(2, 1)),
                ])]
        );
    }

    #[test]
    fn it_builds_zips_to_shorter_column() {
        let positions = vec![pos!(0, 0), pos!(1, 0), pos!(2, 0), pos!(0, 1), pos!(1, 1)];
        let col_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Column]).build(&positions);
        check!(
            col_moves.moves()
                == vec![&SwapMove(vec![
                    (pos!(0, 0), pos!(0, 1)),
                    (pos!(1, 0), pos!(1, 1)),
                ])]
        );
    }

    #[test]
    fn it_builds_from_single_column() {
        let positions = vec![pos!(0, 0), pos!(1, 0)];
        let col_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Column]).build(&positions);
        check!(col_moves.is_empty());
    }

    #[test]
    fn it_builds_from_empty() {
        let col_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Column]).build(&[]);
        check!(col_moves.is_empty());
    }
}

#[cfg(test)]
mod row_moves_tests {
    use assert2::check;

    use super::*;

    #[test]
    fn it_builds() {
        let positions = vec![pos!(0, 0), pos!(0, 1), pos!(1, 0), pos!(1, 1)];
        let row_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Row]).build(&positions);
        check!(
            row_moves.moves()
                == vec![&SwapMove(vec![
                    (pos!(0, 0), pos!(1, 0)),
                    (pos!(0, 1), pos!(1, 1)),
                ])]
        );
    }

    #[test]
    fn it_builds_with_n_rows() {
        let positions = vec![
            pos!(0, 0),
            pos!(0, 1),
            pos!(1, 0),
            pos!(1, 1),
            pos!(2, 0),
            pos!(2, 1),
        ];
        let row_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Row]).build(&positions);
        check!(
            row_moves.moves()
                == vec![
                    &SwapMove(vec![(pos!(0, 0), pos!(1, 0)), (pos!(0, 1), pos!(1, 1))]),
                    &SwapMove(vec![(pos!(0, 0), pos!(2, 0)), (pos!(0, 1), pos!(2, 1))]),
                    &SwapMove(vec![(pos!(1, 0), pos!(2, 0)), (pos!(1, 1), pos!(2, 1))]),
                ]
        );
    }

    #[test]
    fn it_builds_with_n_columns() {
        let positions = vec![
            pos!(0, 0),
            pos!(0, 1),
            pos!(0, 2),
            pos!(1, 0),
            pos!(1, 1),
            pos!(1, 2),
        ];
        let row_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Row]).build(&positions);
        check!(
            row_moves.moves()
                == vec![&SwapMove(vec![
                    (pos!(0, 0), pos!(1, 0)),
                    (pos!(0, 1), pos!(1, 1)),
                    (pos!(0, 2), pos!(1, 2)),
                ])]
        );
    }

    #[test]
    fn it_builds_zips_to_shorter_row() {
        let positions = vec![pos!(0, 0), pos!(0, 1), pos!(0, 2), pos!(1, 0), pos!(1, 1)];
        let row_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Row]).build(&positions);
        check!(
            row_moves.moves()
                == vec![&SwapMove(vec![
                    (pos!(0, 0), pos!(1, 0)),
                    (pos!(0, 1), pos!(1, 1)),
                ])]
        );
    }

    #[test]
    fn it_builds_from_single_row() {
        let positions = vec![pos!(0, 0), pos!(0, 1)];
        let row_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Row]).build(&positions);
        check!(row_moves.is_empty());
    }

    #[test]
    fn it_builds_from_empty() {
        let row_moves = SwapMoveBuilder::new(&[SwapMoveStrategy::Row]).build(&[]);
        check!(row_moves.is_empty());
    }
}
