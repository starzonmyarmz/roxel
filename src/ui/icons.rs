use crate::tools::Tool;
use bevy_egui::egui;

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
pub fn file_plus() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/file-plus.svg")
}
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn folder_open() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/folder-open.svg")
}
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn save() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/save.svg")
}
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn download() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/download.svg")
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
pub fn copy() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/copy.svg")
}
pub fn pencil() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/pencil.svg")
}
pub fn trash() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/trash-2.svg")
}
pub fn upload() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/upload.svg")
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
pub fn corner_down_left() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/icons/corner-down-left.svg")
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
