//! Open `.rox` files handed to us by the OS when the user double-clicks a
//! project in Finder.
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
//! drains it and routes the load through the same `PendingDialog` path as the
//! Open menu, so recents, the dirty baseline, and the toast are handled
//! uniformly.

use bevy::prelude::*;
use std::path::PathBuf;
use std::sync::Mutex;

/// Paths macOS asked us to open, waiting for [`poll_open_files_system`].
/// Cross-platform so the system and its test build everywhere; only the macOS
/// Apple Event handler ever fills it.
static OPEN_QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Take everything queued, leaving the queue empty. Pure helper so the drain
/// logic is unit-testable without the global or Apple Events.
fn drain(queue: &Mutex<Vec<PathBuf>>) -> Vec<PathBuf> {
    let mut guard = queue.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *guard)
}

/// Register the OS-level "open document" hook. No-op off macOS (Windows/Linux
/// pass the path in `argv`, which this app does not yet consume). Call once,
/// on the main thread, before the Bevy event loop starts.
pub fn install() {
    #[cfg(target_os = "macos")]
    macos::install();
}

/// Load any file the OS asked us to open. Single-window app, so if several were
/// dropped at once we open the last. Routes through `PendingDialog` /
/// `poll_dialogs_system` exactly like the Open menu.
pub fn poll_open_files_system(mut pending: ResMut<crate::ui::PendingDialog>) {
    if pending.is_active() {
        return;
    }
    let Some(path) = drain(&OPEN_QUEUE).into_iter().next_back() else {
        return;
    };
    pending.spawn(async move { Some(crate::ui::DialogResult::OpenProject(path)) });
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

    #[test]
    fn last_queued_path_wins() {
        // poll opens the most recently queued file; mirror its selection logic.
        let q = Mutex::new(vec![PathBuf::from("/a.rox"), PathBuf::from("/b.rox")]);
        let last = drain(&q).into_iter().next_back();
        assert_eq!(last, Some(PathBuf::from("/b.rox")));
    }
}
