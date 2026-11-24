use std::collections::BTreeSet;

use pelhaim_core::{AlertId, LineKey, TrainId};

use crate::cards::CardRow;
use crate::scene::Hit;
use crate::textinput::TextInput;
use crate::view::View;

#[derive(Clone, PartialEq, Debug)]
pub struct ViewState {
    pub k: f32,
    pub ox: f32,
    pub oy: f32,
    pub selected: Option<TrainId>,
    pub expanded: BTreeSet<LineKey>,
    pub hover: Option<Hit>,
    pub popup: Option<PopupState>,
    pub inset: Option<InsetState>,
    pub console: TextInput,
    pub console_focused: bool,
    pub radio_log_open: bool,
    pub radio_log: Vec<CardRow>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PopupState {
    pub alert: AlertId,
    pub highlighted: Option<usize>,
    pub picking_project: bool,
}

impl PopupState {
    pub fn new(alert: AlertId) -> Self {
        PopupState {
            alert,
            highlighted: None,
            picking_project: false,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct InsetState {
    pub train: TrainId,
    pub opened_s: f64,
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
            hover: None,
            popup: None,
            inset: None,
            console: TextInput::default(),
            console_focused: false,
            radio_log_open: false,
            radio_log: Vec::new(),
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
