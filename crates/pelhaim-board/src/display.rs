use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::color::Rgba;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct P {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub enum Prim {
    Rect { r: R, color: Rgba, aa: bool },
    Seg { a: P, b: P, width: f32, color: Rgba },
    Tri { p: [P; 3], color: Rgba },
    Clip(R),
    Unclip,
}

#[derive(Default, Serialize, Clone, PartialEq, Debug)]
pub struct DisplayList {
    pub prims: Vec<Prim>,
}

// Rounded to 0.01 so snapshots match across macOS and Linux libm; `+ 0.0` turns -0.0 into 0.0.
fn r2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0 + 0.0
}

impl Serialize for P {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("P", 2)?;
        st.serialize_field("x", &r2(self.x))?;
        st.serialize_field("y", &r2(self.y))?;
        st.end()
    }
}

impl Serialize for R {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("R", 4)?;
        st.serialize_field("x", &r2(self.x))?;
        st.serialize_field("y", &r2(self.y))?;
        st.serialize_field("w", &r2(self.w))?;
        st.serialize_field("h", &r2(self.h))?;
        st.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coords_serialize_rounded() {
        let j = serde_json::to_string(&P { x: 1.23456, y: 2.0 }).unwrap();
        assert_eq!(j, "{\"x\":1.23,\"y\":2.0}");
    }

    #[test]
    fn rect_serializes_rounded() {
        let j = serde_json::to_string(&R {
            x: 0.005,
            y: -3.14567,
            w: 10.0,
            h: 7.999,
        })
        .unwrap();
        assert_eq!(j, "{\"x\":0.01,\"y\":-3.15,\"w\":10.0,\"h\":8.0}");
    }

    #[test]
    fn display_list_serializes() {
        let dl = DisplayList {
            prims: vec![
                Prim::Clip(R {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                }),
                Prim::Unclip,
            ],
        };
        let j = serde_json::to_string(&dl).unwrap();
        assert_eq!(
            j,
            "{\"prims\":[{\"Clip\":{\"x\":0.0,\"y\":0.0,\"w\":1.0,\"h\":1.0}},\"Unclip\"]}"
        );
    }
}
