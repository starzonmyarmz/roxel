pub mod ase;
pub mod atomic;
pub mod gltf;
pub mod gox;
pub mod obj;
pub mod palettes;
pub mod project;
pub mod qb;
pub mod reader;
pub mod recent;
pub mod svg;
pub mod vox;

#[cfg(test)]
pub mod test_util;

use std::sync::atomic::{AtomicBool, Ordering};

static CONFIG_READ_ONLY: AtomicBool = AtomicBool::new(false);

/// Stop every later write to the user's config files (preferences, recents,
/// palettes) for the rest of the process.
pub fn set_config_read_only() {
    CONFIG_READ_ONLY.store(true, Ordering::Relaxed);
}

pub fn config_writable() -> bool {
    writable(&CONFIG_READ_ONLY)
}

fn writable(read_only: &AtomicBool) -> bool {
    !read_only.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_flag_turns_config_writes_off() {
        let flag = AtomicBool::new(false);
        assert!(writable(&flag));
        flag.store(true, Ordering::Relaxed);
        assert!(!writable(&flag));
    }
}
