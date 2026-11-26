use crate::tools::Tool;
use bevy_egui::egui;
use roxel::shapes::ShapePrimitive;

pub fn brush() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/brush.svg")
}
pub fn eraser() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/eraser.svg")
}
pub fn paint_bucket() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/paint-bucket.svg")
}
pub fn pipette() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/pipette.svg")
}
pub fn shapes() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/shapes.svg")
}
pub fn box_select() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/box-select.svg")
}
pub fn move_tool() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/move.svg")
}
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn undo() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/undo.svg")
}
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn redo() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/redo.svg")
}
pub fn plus() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/plus.svg")
}
pub fn check() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/check.svg")
}
pub fn x() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/x.svg")
}
pub fn arrow_up() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/arrow-up.svg")
}
pub fn arrow_down() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/arrow-down.svg")
}
pub fn chevron_down() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/chevron-down.svg")
}
pub fn ellipsis() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/ellipsis.svg")
}
pub fn corner_down_left() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/corner-down-left.svg")
}
pub fn arrow_big_up() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/arrow-big-up.svg")
}
pub fn command() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/command.svg")
}
pub fn option() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/option.svg")
}
pub fn square() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/square.svg")
}
pub fn circle() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/circle.svg")
}
pub fn slash() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/slash.svg")
}
pub fn globe() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/globe.svg")
}
pub fn search() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/search.svg")
}

pub fn shape_primitive(p: ShapePrimitive) -> egui::ImageSource<'static> {
    match p {
        ShapePrimitive::Rectangle => square(),
        ShapePrimitive::Ellipse => circle(),
        ShapePrimitive::Line => slash(),
        ShapePrimitive::Sphere => globe(),
    }
}

pub fn tool(t: Tool) -> egui::ImageSource<'static> {
    match t {
        Tool::Brush => brush(),
        Tool::Erase => eraser(),
        Tool::Paint => paint_bucket(),
        Tool::Eyedropper => pipette(),
        Tool::Shape => shapes(),
        Tool::Select => box_select(),
        Tool::Move => move_tool(),
    }
}

macro_rules! plugin_icons {
    ($($name:literal),* $(,)?) => {
        /// The bundled icons a plugin's toolbar button can name.
        pub const PLUGIN_ICON_NAMES: &[&str] = &[$($name),*];

        pub fn plugin_icon(name: &str) -> Option<egui::ImageSource<'static>> {
            match name {
                $($name => Some(egui::include_image!(concat!("../../assets/icons/plugin/", $name, ".svg"))),)*
                _ => None,
            }
        }
    };
}

plugin_icons!(
    "blend",
    "box",
    "boxes",
    "brick-wall",
    "castle",
    "cloud",
    "cone",
    "cylinder",
    "dices",
    "droplet",
    "fence",
    "flame",
    "flip-horizontal-2",
    "flip-vertical-2",
    "flower",
    "gem",
    "grid-3x3",
    "hammer",
    "heart",
    "house",
    "layers",
    "leaf",
    "moon",
    "mountain",
    "mountain-snow",
    "paintbrush",
    "palette",
    "puzzle",
    "pyramid",
    "rotate-cw",
    "scaling",
    "shovel",
    "shuffle",
    "snowflake",
    "sparkles",
    "sprout",
    "star",
    "sun",
    "tent",
    "torus",
    "tree-deciduous",
    "tree-pine",
    "trees",
    "wand-sparkles",
    "waves",
    "zap",
);

/// The icon a plugin button falls back to when its own can't be shown.
pub const PLUGIN_FALLBACK_ICON: &str = "puzzle";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plugin_icon_name_resolves() {
        for name in PLUGIN_ICON_NAMES {
            assert!(plugin_icon(name).is_some(), "{name}");
        }
        assert!(plugin_icon(PLUGIN_FALLBACK_ICON).is_some());
        assert!(plugin_icon("brush").is_none());
    }

    #[test]
    fn plugin_icon_names_match_the_api_spec() {
        let dts = include_str!("../../docs/src/roxel.d.ts");
        let start = dts
            .find("export type IconName =")
            .expect("IconName in roxel.d.ts");
        let decl = &dts[start..start + dts[start..].find(';').unwrap()];
        let mut spec: Vec<&str> = decl.split('"').skip(1).step_by(2).collect();
        spec.sort();
        let mut ours = PLUGIN_ICON_NAMES.to_vec();
        ours.sort();
        assert_eq!(spec, ours);
    }
}
