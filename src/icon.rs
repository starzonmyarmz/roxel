use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;
use winit::window::Icon;

const ICON_PNG: &[u8] = include_bytes!("../assets/icons/roxel-1024.png");

pub fn set_window_icon(
    _marker: NonSendMarker,
    mut done: Local<bool>,
    primary: Query<Entity, With<PrimaryWindow>>,
) {
    if *done {
        return;
    }
    let Ok(entity) = primary.single() else {
        return;
    };

    let icon_set = WINIT_WINDOWS.with(|cell| {
        let windows = cell.borrow();
        let Some(window) = windows.get_window(entity) else {
            return false;
        };
        let Ok(img) = image::load_from_memory(ICON_PNG) else {
            return true;
        };
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        if let Ok(icon) = Icon::from_rgba(rgba.into_raw(), w, h) {
            window.set_window_icon(Some(icon));
        }
        true
    });

    if !icon_set {
        return;
    }
    *done = true;

    #[cfg(target_os = "macos")]
    set_macos_dock_icon();
}

/// Set the Finder icon for `path` to the given PNG image. No-op off macOS.
///
/// Uses `-[NSWorkspace setIcon:forFile:options:]`, the only API that actually
/// writes a file's custom Finder icon. The earlier `NSURLCustomIconKey` route
/// silently no-ops: that key is readable but unimplemented for writes, so the
/// file kept falling back to the document type icon (the app icon).
#[cfg(target_os = "macos")]
pub fn set_finder_icon(path: &std::path::Path, png_bytes: &[u8]) {
    use objc2::AnyThread;
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSImage, NSWorkspace, NSWorkspaceIconCreationOptions};
    use objc2_foundation::{MainThreadMarker, NSData, NSString};

    if MainThreadMarker::new().is_none() {
        return;
    }
    autoreleasepool(|_| {
        let data = NSData::with_bytes(png_bytes);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            bevy::log::warn!("set_finder_icon: failed to create NSImage from PNG data");
            return;
        };
        let path_str = NSString::from_str(path.to_str().unwrap_or(""));
        let workspace = NSWorkspace::sharedWorkspace();
        let ok = workspace.setIcon_forFile_options(
            Some(&image),
            &path_str,
            NSWorkspaceIconCreationOptions::empty(),
        );
        if !ok {
            bevy::log::warn!("set_finder_icon: NSWorkspace setIcon:forFile: returned false");
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn set_finder_icon(_path: &std::path::Path, _png_bytes: &[u8]) {}

#[cfg(target_os = "macos")]
fn set_macos_dock_icon() {
    use objc2::AnyThread;
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{MainThreadMarker, NSData};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    autoreleasepool(|_| unsafe {
        let data = NSData::with_bytes(ICON_PNG);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        app.setApplicationIconImage(Some(&image));
    });
}
