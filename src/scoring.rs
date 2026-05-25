use layouts_rs_macros::scoring;

#[cfg(not(test))]
#[scoring]
mod scoring {
    use crate::layout::*;
    use crate::ngrams::*;

    mod metrics {
        use super::*;

        pub fn total_chars(_unigram: &Unigram, count: f64) -> Option<f64> {
            Some(count)
        }

        pub fn pinky_off_home(unigram: &Unigram, count: f64) -> Option<f64> {
            (unigram.key.finger.kind == FingerKind::Pinky && !unigram.key.finger_home)
                .then_some(count)
        }

        pub fn effort(unigram: &Unigram, count: f64) -> Option<f64> {
            Some(unigram.key.effort * count)
        }

        pub fn column_usage(unigram: &Unigram, count: f64) -> Option<(usize, f64)> {
            Some((unigram.key.position.c, count))
        }

        pub fn row_usage(unigram: &Unigram, count: f64) -> Option<(usize, f64)> {
            Some((unigram.key.position.r, count))
        }

        pub fn finger_usage(unigram: &Unigram, count: f64) -> Option<(Finger, f64)> {
            Some((unigram.key.finger, count))
        }

        pub fn bigram_skips(_bigram: &Bigram, kind: &BigramKind, count: f64) -> Option<(u8, f64)> {
            let BigramKind::SameFingerSkip(skip) = kind else {
                return None;
            };

            Some((skip.units, count))
        }

        pub fn bigram_lateral_stretches(
            _bigram: &Bigram,
            kind: &BigramKind,
            count: f64,
        ) -> Option<(FingerKind, f64)> {
            let BigramKind::LateralStretch(stretch) = kind else {
                return None;
            };

            Some((stretch.finger, count))
        }

        pub fn bigram_scissors(
            _bigram: &Bigram,
            kind: &BigramKind,
            count: f64,
        ) -> Option<(u8, f64)> {
            let BigramKind::Scissor(scissor) = kind else {
                return None;
            };
            if !is_scissor_metric(scissor) {
                return None;
            }

            Some((scissor.units, count))
        }

        pub fn bigram_others(_bigram: &Bigram, kind: &BigramKind, count: f64) -> Option<f64> {
            matches!(kind, BigramKind::Other).then_some(count)
        }

        pub fn trigram_skips_same_hand(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(u8, f64)> {
            let TrigramKind::SameFingerSkip(skip) = kind else {
                return None;
            };
            if skip.handedness != Handedness::Same {
                return None;
            }

            Some((skip.skip.units, count))
        }

        pub fn trigram_skips_alternation(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(u8, f64)> {
            let TrigramKind::SameFingerSkip(skip) = kind else {
                return None;
            };
            if skip.handedness != Handedness::Alternate {
                return None;
            }

            Some((skip.skip.units, count))
        }

        pub fn trigram_scissors_same_hand(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(u8, f64)> {
            let TrigramKind::Scissor(scissor) = kind else {
                return None;
            };
            if scissor.handedness != Handedness::Alternate && !is_scissor_metric(&scissor.scissor) {
                return None;
            }

            Some((scissor.scissor.units, count))
        }

        pub fn trigram_scissors_alternation(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(u8, f64)> {
            let TrigramKind::Scissor(scissor) = kind else {
                return None;
            };
            if scissor.handedness != Handedness::Same && is_scissor_metric(&scissor.scissor) {
                return None;
            }

            Some((scissor.scissor.units, count))
        }

        pub fn trigram_lateral_stretches_same_hand(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(FingerKind, f64)> {
            let TrigramKind::LateralStretch(stretch) = kind else {
                return None;
            };
            if stretch.handedness != Handedness::Same {
                return None;
            }

            Some((stretch.stretch.finger, count))
        }

        pub fn trigram_lateral_stretches_alternation(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(FingerKind, f64)> {
            let TrigramKind::LateralStretch(stretch) = kind else {
                return None;
            };
            if stretch.handedness != Handedness::Alternate {
                return None;
            }

            Some((stretch.stretch.finger, count))
        }

        pub fn trigram_redirects(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(RedirectStrength, f64)> {
            let TrigramKind::Redirect(redirect) = kind else {
                return None;
            };

            Some((redirect.strength, count))
        }

        pub fn trigram_roll(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(RollDirection, f64)> {
            let TrigramKind::Roll(roll) = kind else {
                return None;
            };
            if roll.length != 3 {
                return None;
            }

            Some((roll.direction, count))
        }

        pub fn trigram_roll_bigrams(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<(RollDirection, f64)> {
            let TrigramKind::Roll(roll) = kind else {
                return None;
            };
            if roll.length != 2 {
                return None;
            }

            Some((roll.direction, count))
        }

        pub fn trigram_alternations(
            _trigram: &Trigram,
            kind: &TrigramKind,
            count: f64,
        ) -> Option<f64> {
            matches!(kind, TrigramKind::Alternation).then_some(count)
        }

        pub fn trigram_others(_trigram: &Trigram, kind: &TrigramKind, count: f64) -> Option<f64> {
            matches!(kind, TrigramKind::Other).then_some(count)
        }
    }

    mod stats {
        use super::*;
        use crate::map::*;

        #[stat_let]
        pub fn left_hand_usage(finger_usage: &FingerUsage) -> f64 {
            finger_usage
                .entries()
                .filter(|(group, _)| group.hand == Hand::Left)
                .values()
                .sum()
        }

        #[stat_let]
        pub fn right_hand_usage(finger_usage: &FingerUsage) -> f64 {
            finger_usage
                .entries()
                .filter(|(group, _)| group.hand == Hand::Right)
                .values()
                .sum()
        }

        #[stat_let]
        pub fn roll_in(trigram_roll: &TrigramRoll) -> f64 {
            trigram_roll
                .entries()
                .filter(|(group, _)| **group == RollDirection::In)
                .values()
                .sum()
        }

        #[stat_let]
        pub fn roll_out(trigram_roll: &TrigramRoll) -> f64 {
            trigram_roll
                .entries()
                .filter(|(group, _)| **group == RollDirection::Out)
                .values()
                .sum()
        }

        #[stat_let]
        pub fn roll_in_bigrams(trigram_roll_bigrams: &TrigramRollBigrams) -> f64 {
            trigram_roll_bigrams
                .entries()
                .filter(|(group, _)| **group == RollDirection::In)
                .values()
                .sum()
        }

        #[stat_let]
        pub fn roll_out_bigrams(trigram_roll_bigrams: &TrigramRollBigrams) -> f64 {
            trigram_roll_bigrams
                .entries()
                .filter(|(group, _)| **group == RollDirection::Out)
                .values()
                .sum()
        }

        #[stat_let]
        pub fn total_hand_usage(finger_usage: &FingerUsage) -> f64 {
            finger_usage.entries().values().sum()
        }

        #[stat_let]
        pub fn total_row_usage(row_usage: &RowUsage) -> f64 {
            row_usage.entries().values().sum()
        }

        #[stat_let]
        pub fn total_column_usage(column_usage: &ColumnUsage) -> f64 {
            column_usage.entries().values().sum()
        }

        mod general {
            use super::*;

            fn total_chars(total_chars: &TotalChars) -> f64 {
                *total_chars
            }

            #[target(effort)]
            fn effort(effort: &Effort, total_chars: &TotalChars) -> f64 {
                percentage(*effort, *total_chars)
            }

            #[target(pinky_off_home)]
            fn pinky_off_home(pinky_off_home: &PinkyOffHome, total_chars: &TotalChars) -> f64 {
                percentage(*pinky_off_home, *total_chars)
            }

            #[target(finger_usage)]
            fn finger_usage(lets: &StatLets, finger_usage: &FingerUsage) -> Map<Finger> {
                finger_usage
                    .entries()
                    .normalize(lets.total_hand_usage)
                    .collect()
            }

            #[target(row_usage)]
            fn row_usage(lets: &StatLets, row_usage: &RowUsage) -> Map<usize> {
                row_usage
                    .entries()
                    .normalize(lets.total_row_usage)
                    .collect()
            }

            #[target(column_usage)]
            fn column_usage(lets: &StatLets, column_usage: &ColumnUsage) -> Map<usize> {
                column_usage
                    .entries()
                    .normalize(lets.total_column_usage)
                    .collect()
            }

            #[target(left_hand_usage)]
            fn left_hand_usage(lets: &StatLets) -> f64 {
                percentage(lets.left_hand_usage, lets.total_hand_usage)
            }

            fn right_hand_usage(lets: &StatLets) -> f64 {
                percentage(lets.right_hand_usage, lets.total_hand_usage)
            }
        }

        mod bigram {
            use super::*;

            #[target(bigram_skips)]
            fn skips(bigram_skips: &BigramSkips, total_chars: &TotalChars) -> Map<u8> {
                bigram_skips.entries().normalize(*total_chars).collect()
            }

            #[target(bigram_scissors)]
            fn scissors(bigram_scissors: &BigramScissors, total_chars: &TotalChars) -> Map<u8> {
                bigram_scissors.entries().normalize(*total_chars).collect()
            }

            #[target(bigram_lateral_stretches)]
            fn lateral_stretches(
                bigram_lateral_stretches: &BigramLateralStretches,
                total_chars: &TotalChars,
            ) -> Map<FingerKind> {
                bigram_lateral_stretches
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            fn others(bigram_others: &BigramOthers, total_chars: &TotalChars) -> f64 {
                percentage(*bigram_others, *total_chars)
            }
        }

        mod trigram {
            use super::*;

            #[target(trigram_skips_same_hand)]
            fn skips_same_hand(
                trigram_skips_same_hand: &TrigramSkipsSameHand,
                total_chars: &TotalChars,
            ) -> Map<u8> {
                trigram_skips_same_hand
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            #[target(trigram_skips_alternation)]
            fn skips_alternation(
                trigram_skips_alternation: &TrigramSkipsAlternation,
                total_chars: &TotalChars,
            ) -> Map<u8> {
                trigram_skips_alternation
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            #[target(trigram_scissors_same_hand)]
            fn scissors_same_hand(
                trigram_scissors_same_hand: &TrigramScissorsSameHand,
                total_chars: &TotalChars,
            ) -> Map<u8> {
                trigram_scissors_same_hand
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            #[target(trigram_scissors_alternation)]
            fn scissors_alternation(
                trigram_scissors_alternation: &TrigramScissorsAlternation,
                total_chars: &TotalChars,
            ) -> Map<u8> {
                trigram_scissors_alternation
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            #[target(trigram_lateral_stretches_same_hand)]
            fn lateral_stretches_same_hand(
                trigram_lateral_stretches_same_hand: &TrigramLateralStretchesSameHand,
                total_chars: &TotalChars,
            ) -> Map<FingerKind> {
                trigram_lateral_stretches_same_hand
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            #[target(trigram_lateral_stretches_alternation)]
            fn lateral_stretches_alternation(
                trigram_lateral_stretches_alternation: &TrigramLateralStretchesAlternation,
                total_chars: &TotalChars,
            ) -> Map<FingerKind> {
                trigram_lateral_stretches_alternation
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            #[target(trigram_redirects)]
            fn redirects(
                trigram_redirects: &TrigramRedirects,
                total_chars: &TotalChars,
            ) -> Map<RedirectStrength> {
                trigram_redirects
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            fn roll(trigram_roll: &TrigramRoll, total_chars: &TotalChars) -> Map<RollDirection> {
                trigram_roll.entries().normalize(*total_chars).collect()
            }

            fn roll_bigrams(
                trigram_roll_bigrams: &TrigramRollBigrams,
                total_chars: &TotalChars,
            ) -> Map<RollDirection> {
                trigram_roll_bigrams
                    .entries()
                    .normalize(*total_chars)
                    .collect()
            }

            #[target(trigram_roll_ratio)]
            fn roll_ratio(lets: &StatLets) -> f64 {
                ratio(lets.roll_in, lets.roll_out)
            }

            #[target(trigram_roll_ratio_bigrams)]
            fn roll_ratio_bigrams(lets: &StatLets) -> f64 {
                ratio(lets.roll_in_bigrams, lets.roll_out_bigrams)
            }

            #[target(trigram_alternations)]
            fn alternations(
                trigram_alternations: &TrigramAlternations,
                total_chars: &TotalChars,
            ) -> f64 {
                percentage(*trigram_alternations, *total_chars)
            }

            fn others(trigram_others: &TrigramOthers, total_chars: &TotalChars) -> f64 {
                percentage(*trigram_others, *total_chars)
            }
        }
    }

    const ALLOWED_SCISSORS: [(FingerKind, FingerKind); 8] = [
        (FingerKind::Index, FingerKind::Middle),
        (FingerKind::Index, FingerKind::Ring),
        (FingerKind::Middle, FingerKind::Ring),
        (FingerKind::Ring, FingerKind::Middle),
        (FingerKind::Ring, FingerKind::Index),
        (FingerKind::Pinky, FingerKind::Ring),
        (FingerKind::Pinky, FingerKind::Middle),
        (FingerKind::Pinky, FingerKind::Index),
    ];

    fn is_scissor_metric(scissor: &Scissor) -> bool {
        scissor.units > 1
            || (scissor.units == 1
                && !ALLOWED_SCISSORS.contains(&(scissor.lower_finger, scissor.upper_finger)))
    }

    fn percentage(v: f64, total: f64) -> f64 {
        100.0 * v / total
    }

    fn ratio(a: f64, b: f64) -> f64 {
        if a + b > 0.0 {
            100.0 * a / (a + b)
        } else {
            50.0
        }
    }
}

#[cfg(test)]
#[scoring]
mod scoring {
    mod metrics {
        use crate::ngrams::Unigram;

        pub fn total_chars(_unigram: &Unigram, count: f64) -> Option<f64> {
            Some(count)
        }

        pub fn effort(unigram: &Unigram, count: f64) -> Option<f64> {
            Some(unigram.key.effort * count)
        }
    }
    mod stats {
        mod general {
            use crate::metrics::*;

            fn total_chars(total_chars: &TotalChars) -> f64 {
                *total_chars
            }

            #[target(effort)]
            fn effort(effort: &Effort, total_chars: &TotalChars) -> f64 {
                100.0 * *effort / *total_chars
            }
        }
    }
}
