pub mod a11y;
pub mod cards;
pub mod color;
pub mod console_cmd;
pub mod display;
pub mod font;
pub mod input;
pub mod palette;
pub mod scene;
pub mod textinput;
pub mod view;

pub use a11y::{A11yNode, A11yRole, a11y_tree};
pub use cards::{CardRow, Mark, parse_cards};
pub use color::{Rgba, oklab_l, oklab_mute};
pub use console_cmd::{ConsoleCmd, parse_console};
pub use display::{DisplayList, P, Prim, R};
pub use input::{
    InsetState, Mods, PopupState, UiAction, UiInput, UiKey, ViewState, on_input, sync,
};
pub use palette::{BlockState, Palette, PaletteKind, block_color, palette};
pub use scene::{
    AlertButton, Hit, InsetAction, Layout, LineBox, Scene, SceneInput, TrackGeom, board_layout,
    build_board, build_overlays, build_scene, state_words, tag_rect,
};
pub use textinput::TextInput;
pub use view::{View, fit};
