use std::collections::BTreeSet;

use pelhaim_core::{LineKey, TrainId};

use crate::view::View;

#[derive(Clone, PartialEq, Debug)]
pub struct ViewState {
    pub k: f32,
    pub ox: f32,
    pub oy: f32,
    pub selected: Option<TrainId>,
    pub expanded: BTreeSet<LineKey>,
}

// By hand, not derived: a derived `k = 0` would collapse the whole board to a point (F30).
impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            k: 1.0,
            ox: 0.0,
            oy: 0.0,
            selected: None,
            expanded: BTreeSet::new(),
        }
    }
}

impl ViewState {
    pub fn view(&self) -> View {
        View {
            k: self.k,
            ox: self.ox,
            oy: self.oy,
        }
    }
}
