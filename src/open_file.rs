//! Open `.rox` files handed to us by the OS: a double-clicked project in
//! Finder on macOS, or a path in `argv` (Windows/Linux file associations,
//! `roxel scene.rox` from a shell).
//!
//! On macOS a double-clicked document is **not** passed in `argv` for `.app`
//! bundles — LaunchServices delivers it as a `kAEOpenDocuments` Apple Event.
//! The event is routed to whichever app declares the file's type in its
//! Info.plist (`assets/Info.plist.ext`); without both halves — the plist
//! declaration *and* this handler — double-click fails with "can't open this
//! type of file". The in-app File → Open dialog bypasses LaunchServices, which
//! is why that path always worked.
//!
//! The handler pushes paths onto [`OPEN_QUEUE`]; [`poll_open_files_system`]
//! drains it and requests the open through `UnsavedGuard`, the same path as
//! the Open menu, so the unsaved-changes prompt, recents, the dirty baseline,
//! and the toast are handled uniformly.

use bevy::prelude::*;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;

/// Paths the OS asked us to open, waiting for [`poll_open_files_system`].
/// Filled from `argv` at startup and, on macOS, by the Apple Event handler.
static OPEN_QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Take everything queued, leaving the queue empty. Pure helper so the drain
/// logic is unit-testable without the global or Apple Events.
fn drain(queue: &Mutex<Vec<PathBuf>>) -> Vec<PathBuf> {
    let mut guard = queue.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *guard)
}

/// Queue the file named in `argv`, if any, and register the macOS "open
/// document" hook. Call once, on the main thread, before the Bevy event loop
/// starts.
pub fn install() {
    if let Some(path) = path_from_args(std::env::args_os()) {
        OPEN_QUEUE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(path);
    }
    #[cfg(target_os = "macos")]
    macos::install();
}

/// The file to open from a process's arguments: the last one after the
/// program name that isn't a flag. Flags are skipped because older macOS
/// passes `-psn_…` to apps launched from Finder.
fn path_from_args(args: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.is_empty() && !a.to_string_lossy().starts_with('-'))
        .last()
        .map(PathBuf::from)
}

/// Open any file the OS asked us to open. Single-window app, so if several were
/// dropped at once we open the last. Goes through the unsaved-changes guard
/// like the Open menu, so a modified document prompts first. Paths stay queued
/// while the guard is busy with another action, so none are dropped.
pub fn poll_open_files_system(mut guard: ResMut<crate::ui::UnsavedGuard>) {
    if !guard.is_idle() {
        return;
    }
    let Some(path) = drain(&OPEN_QUEUE).into_iter().next_back() else {
        return;
    };
    guard.request(crate::ui::GuardedAction::Open(Some(path)));
}

#[cfg(target_os = "macos")]
mod macos {
    use super::OPEN_QUEUE;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{AnyThread, define_class, msg_send, sel};
    use objc2_app_kit::NSApplicationWillFinishLaunchingNotification;
    use objc2_foundation::{
        NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter,
    };
    use std::path::PathBuf;

    // Four-char codes (FourCharCode / OSType), not exported by the bindings.
    /// `keyDirectObject` — the list of file URLs carried by the event.
    const KEY_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
    /// `kCoreEventClass`.
    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    /// `kAEOpenDocuments`.
    const AE_OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");

    define_class!(
        // SAFETY: NSObject has no subclassing requirements and Handler has no
        // ivars and does not implement Drop.
        #[unsafe(super(NSObject))]
        #[name = "RoxelOpenFileHandler"]
        struct Handler;

        impl Handler {
            /// `NSApplicationWillFinishLaunchingNotification` callback. Must be
            /// `will`, not `did`: on a cold launch LaunchServices delivers the
            /// `kAEOpenDocuments` event during startup, and AppKit dispatches it
            /// to its *default* (no-op) handler before `didFinishLaunching` fires.
            /// Registering at `did` loses the cold-launch event (the double-click
            /// opens a blank scene); `will` runs early enough to override AppKit's
            /// handler before the queued event is dispatched. Warm launches (app
            /// already running) work either way. Registering from `install`,
            /// before the app exists, is too early — AppKit clobbers it.
            #[unsafe(method(appWillFinishLaunching:))]
            fn app_will_finish_launching(&self, _note: &NSNotification) {
                let manager = NSAppleEventManager::sharedAppleEventManager();
                unsafe {
                    manager.setEventHandler_andSelector_forEventClass_andEventID(
                        self,
                        sel!(handleAppleEvent:withReplyEvent:),
                        CORE_EVENT_CLASS,
                        AE_OPEN_DOCUMENTS,
                    );
                }
            }

            /// `kAEOpenDocuments` handler. The direct object is a descriptor list
            /// of file URLs; push each resolvable path onto the queue.
            #[unsafe(method(handleAppleEvent:withReplyEvent:))]
            fn handle_apple_event(
                &self,
                event: &NSAppleEventDescriptor,
                _reply: &NSAppleEventDescriptor,
            ) {
                let Some(list) = event.paramDescriptorForKeyword(KEY_DIRECT_OBJECT) else {
                    return;
                };
                let mut paths = Vec::new();
                for i in 1..=list.numberOfItems() {
                    let Some(item) = list.descriptorAtIndex(i) else {
                        continue;
                    };
                    let Some(url) = item.fileURLValue() else {
                        continue;
                    };
                    if let Some(path) = url.path() {
                        paths.push(PathBuf::from(path.to_string()));
                    }
                }
                if !paths.is_empty() {
                    let mut guard = OPEN_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                    guard.extend(paths);
                }
            }
        }
    );

    pub fn install() {
        // The handler is referenced (unretained) by the notification center and,
        // later, the Apple Event manager. It must live for the whole process, so
        // leak it deliberately rather than tracking ownership.
        let handler = Handler::alloc();
        let handler: Retained<Handler> = unsafe { msg_send![handler, init] };
        let center = NSNotificationCenter::defaultCenter();
        let observer: &AnyObject = &handler;
        unsafe {
            center.addObserver_selector_name_object(
                observer,
                sel!(appWillFinishLaunching:),
                Some(NSApplicationWillFinishLaunchingNotification),
                None,
            );
        }
        std::mem::forget(handler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_takes_all_and_empties() {
        let q = Mutex::new(vec![PathBuf::from("/a.rox"), PathBuf::from("/b.rox")]);
        let drained = drain(&q);
        assert_eq!(
            drained,
            vec![PathBuf::from("/a.rox"), PathBuf::from("/b.rox")]
        );
        assert!(q.lock().unwrap().is_empty());
    }

    #[test]
    fn drain_empty_queue_is_empty() {
        let q: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
        assert!(drain(&q).is_empty());
    }

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn path_from_args_skips_program_name() {
        assert_eq!(path_from_args(args(&["roxel"])), None);
        assert_eq!(path_from_args(args(&[])), None);
        assert_eq!(
            path_from_args(args(&["roxel", "C:\\scenes\\house.rox"])),
            Some(PathBuf::from("C:\\scenes\\house.rox"))
        );
    }

    #[test]
    fn path_from_args_ignores_flags_and_takes_last() {
        assert_eq!(path_from_args(args(&["roxel", "-psn_0_12345"])), None);
        assert_eq!(
            path_from_args(args(&["roxel", "-psn_0_1", "/a.rox", "/b.rox"])),
            Some(PathBuf::from("/b.rox"))
        );
    }

    #[test]
    fn last_queued_path_wins() {
        // poll opens the most recently queued file; mirror its selection logic.
        let q = Mutex::new(vec![PathBuf::from("/a.rox"), PathBuf::from("/b.rox")]);
        let last = drain(&q).into_iter().next_back();
        assert_eq!(last, Some(PathBuf::from("/b.rox")));
    }
}
