pub mod color;
pub mod display;
pub mod font;
pub mod input;
pub mod palette;
pub mod scene;
pub mod view;

pub use color::{Rgba, oklab_l, oklab_mute};
pub use display::{DisplayList, P, Prim, R};
pub use input::ViewState;
pub use palette::{BlockState, Palette, PaletteKind, block_color, palette};
pub use scene::{
    AlertButton, Hit, InsetAction, Layout, LineBox, Scene, SceneInput, TrackGeom, board_layout,
    build_board, build_scene, tag_rect,
};
pub use view::{View, fit};
