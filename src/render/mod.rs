//! Color normalization and terminal raster rendering.

pub mod colors;
pub mod landmask;
pub mod map_background;
pub mod protocol;
pub mod raster;

use std::env;

/// Initialize the Rayon global thread pool for parallel rasterization and backdrop rendering.
///
/// Respects `NCVIEW_THREADS` or `RAYON_NUM_THREADS` if set. Otherwise, defaults conservatively
/// to `min(available_cpus, 8)` to prevent hogging resources on shared HPC head nodes.
pub fn configure_thread_pool() {
    let threads = if let Ok(val) = env::var("NCVIEW_THREADS") {
        val.parse::<usize>().ok()
    } else if let Ok(val) = env::var("RAYON_NUM_THREADS") {
        val.parse::<usize>().ok()
    } else {
        None
    };

    let count = threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4)
    });

    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(count.max(1))
        .build_global();
}

#[cfg(test)]
mod colors_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configure_thread_pool_does_not_panic() {
        configure_thread_pool();
    }
}
