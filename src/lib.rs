#[cfg(test)]
macro_rules! pos {
    ($row:expr, $col:expr) => {
        crate::matrix::Pos::new($row, $col)
    };
}

#[cfg(test)]
macro_rules! key_size {
    ($height:expr, $width:expr) => {
        crate::layout::KeySize::new($height, $width)
    };
}

#[macro_export]
macro_rules! matrix {
    (fingers, [$([$($x:expr),* $(,)?]),+ $(,)?]) => {
        $crate::matrix::Matrix::new(vec![
            $(vec![$(Some(finger!($x))),*]),+
        ]).unwrap()
    };
    ([$([$($x:expr),* $(,)?]),+ $(,)?]) => {
        $crate::matrix::Matrix::new(vec![
            $(vec![$($x),*]),+
        ]).unwrap()
    };
}

#[macro_export]
macro_rules! coords {
    ($x:expr, $y:expr) => {
        $crate::layout::Coords::new($x, $y)
    };
}

#[macro_export]
macro_rules! size {
    ($h:expr, $w:expr) => {
        $crate::layout::KeySize::new($h, $w)
    };
}

#[cfg(test)]
macro_rules! key {
    ($ch:expr, $finger_number:expr, $pos:expr) => {
        key!($ch, $finger_number, $pos, 1.0)
    };
    ($ch:expr, $finger_number:expr, $pos:expr, $effort:expr) => {
        key!(
            $ch,
            $finger_number,
            $pos,
            $effort,
            coords!($pos.r as f64, $pos.c as f64)
        )
    };
    ($ch:expr, $finger_number:expr, $pos:expr, $effort:expr, $coords:expr) => {
        key!(
            $ch,
            $finger_number,
            $pos,
            $effort,
            $coords,
            key_size!(1.0, 1.0)
        )
    };
    ($ch:expr, $finger_number:expr, $pos:expr, $effort:expr, $coords:expr, $size:expr) => {
        crate::layout::Key::new(
            $ch,
            crate::layout::Finger::try_from($finger_number as u8).unwrap(),
            $pos,
            $effort,
            false,
            $coords,
            $size,
        )
    };
}

#[cfg(test)]
macro_rules! finger {
    ($num:expr) => {
        crate::layout::Finger::try_from($num as u8).unwrap()
    };
}

#[cfg(test)]
macro_rules! finger_home_key {
    ($ch:expr, $finger_number:expr, $pos:expr) => {
        finger_home_key!($ch, $finger_number, $pos, 1.0)
    };
    ($ch:expr, $finger_number:expr, $pos:expr, $effort:expr) => {
        crate::layout::Key::new(
            $ch,
            crate::layout::Finger::try_from($finger_number as u8).unwrap(),
            $pos,
            $effort,
            true,
            coords!($pos.r as f64, $pos.c as f64),
            key_size!(1.0, 1.0),
        )
    };
}

#[cfg(test)]
macro_rules! ngram {
    ($layout:expr, $char:expr) => {
        crate::ngrams::Unigram::new($layout.key_for($char).unwrap())
    };
    ($layout:expr, $char1:expr, $char2:expr) => {
        crate::ngrams::Bigram::new(
            $layout.key_for($char1).unwrap(),
            $layout.key_for($char2).unwrap(),
        )
    };
    ($layout:expr, $char1:expr, $char2:expr, $char3:expr) => {
        crate::ngrams::Trigram::new(
            $layout.key_for($char1).unwrap(),
            $layout.key_for($char2).unwrap(),
            $layout.key_for($char3).unwrap(),
        )
    };
}

pub mod analyzer;
pub mod config;
pub mod corpus;
pub mod layout;
pub mod map;
pub mod matrix;
mod scoring;
pub use scoring::{metrics, stats, targets};
pub mod ngrams;
pub mod optimizer;
pub mod serialization;
pub mod swaps;
