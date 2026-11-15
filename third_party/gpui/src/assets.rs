use crate::{DevicePixels, Pixels, Result, SharedString, Size, size};
use collections::FxHashMap;
use smallvec::SmallVec;

use image::{Delay, Frame};
use std::{
    borrow::Cow,
    fmt,
    hash::Hash,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

/// A source of assets for this app to use.
pub trait AssetSource: 'static + Send + Sync {
    /// Load the given asset from the source path.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>>;

    /// List the assets at the given path.
    fn list(&self, path: &str) -> Result<Vec<SharedString>>;
}

impl AssetSource for () {
    fn load(&self, _path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(None)
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

/// A unique identifier for the image cache
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ImageId(pub usize);

#[derive(PartialEq, Eq, Hash, Clone)]
pub(crate) struct RenderImageParams {
    pub(crate) image_id: ImageId,
    pub(crate) frame_index: usize,
}

/// A cached and processed image, in BGRA format
pub struct RenderImage {
    /// The ID associated with this image
    pub id: ImageId,
    /// The scale factor of this image on render.
    pub(crate) scale_factor: f32,
    data: SmallVec<[Frame; 1]>,
}

impl PartialEq for RenderImage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RenderImage {}

impl RenderImage {
    /// Create a new image from the given data.
    pub fn new(data: impl Into<SmallVec<[Frame; 1]>>) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        Self {
            id: ImageId(NEXT_ID.fetch_add(1, SeqCst)),
            scale_factor: 1.0,
            data: data.into(),
        }
    }

    /// Convert this image into a byte slice.
    pub fn as_bytes(&self, frame_index: usize) -> Option<&[u8]> {
        self.data
            .get(frame_index)
            .map(|frame| frame.buffer().as_raw().as_slice())
    }

    /// Get the size of this image, in pixels.
    pub fn size(&self, frame_index: usize) -> Size<DevicePixels> {
        let (width, height) = self.data[frame_index].buffer().dimensions();
        size(width.into(), height.into())
    }

    /// Get the size of this image, in pixels for display, adjusted for the scale factor.
    pub(crate) fn render_size(&self, frame_index: usize) -> Size<Pixels> {
        self.size(frame_index)
            .map(|v| (v.0 as f32 / self.scale_factor).into())
    }

    /// Get the delay of this frame from the previous
    pub fn delay(&self, frame_index: usize) -> Delay {
        self.data[frame_index].delay()
    }

    /// Get the number of frames for this image.
    pub fn frame_count(&self) -> usize {
        self.data.len()
    }

    /// Bytes held by every decoded frame, including animated ones.
    pub(crate) fn byte_size(&self) -> usize {
        self.data
            .iter()
            .map(|frame| frame.buffer().as_raw().len())
            .sum()
    }
}

/// Budget for decoded image assets. The asset store keeps a `RenderImage`
/// alive for as long as its entry exists, so a launcher or gallery that
/// scrolls through hundreds of pictures would otherwise pin their pixels for
/// the life of the process. This tracks the last use of every decoded image
/// and names the least recently used ones to evict once the cap is exceeded.
#[derive(Default)]
pub(crate) struct ImageBudget {
    limit: usize,
    bytes: usize,
    clock: u64,
    /// Source image id to the monotonic stamp of its last use.
    used: FxHashMap<u64, u64>,
    /// Source image id to the decoded size it contributed.
    sizes: FxHashMap<u64, usize>,
}

impl ImageBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Zero leaves decoded images cached for the app's lifetime.
    pub(crate) fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// Note that one decoded image was just used, and return the source ids
    /// whose pixels must now leave the asset store.
    pub(crate) fn record(&mut self, id: u64, bytes: usize) -> Vec<u64> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(previous) = self.sizes.insert(id, bytes) {
            self.bytes = self.bytes.saturating_sub(previous);
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.used.insert(id, self.clock);
        if self.limit == 0 || self.bytes <= self.limit {
            return Vec::new();
        }
        self.evict()
    }

    fn evict(&mut self) -> Vec<u64> {
        let mut order: Vec<(u64, u64)> = self
            .used
            .iter()
            .map(|(id, stamp)| (*id, *stamp))
            .collect();
        order.sort_unstable_by_key(|(_, stamp)| *stamp);
        // The image just recorded must survive even when it alone is larger
        // than the budget, or the next frame would evict and reload it.
        let newest = order.last().map(|(id, _)| *id);
        let mut evicted = Vec::new();
        for (id, _) in order {
            if self.bytes <= self.limit {
                break;
            }
            if Some(id) == newest {
                continue;
            }
            if let Some(bytes) = self.sizes.remove(&id) {
                self.bytes = self.bytes.saturating_sub(bytes);
            }
            self.used.remove(&id);
            evicted.push(id);
        }
        evicted
    }

    /// Evict down to the current limit without recording a new use. Used when
    /// the limit is lowered at runtime.
    pub(crate) fn trim(&mut self) -> Vec<u64> {
        if self.limit == 0 || self.bytes <= self.limit {
            return Vec::new();
        }
        self.evict()
    }

    /// Forget an image whose pixels left the asset store.
    pub(crate) fn remove(&mut self, id: u64) {
        if let Some(bytes) = self.sizes.remove(&id) {
            self.bytes = self.bytes.saturating_sub(bytes);
        }
        self.used.remove(&id);
    }
}

impl fmt::Debug for RenderImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("id", &self.id)
            .field("size", &self.size(0))
            .finish()
    }
}

#[cfg(test)]
mod image_budget_tests {
    use super::*;

    #[test]
    fn the_image_budget_evicts_the_least_recently_used() {
        let mut budget = ImageBudget::new(100);
        assert!(budget.record(1, 40).is_empty());
        assert!(budget.record(2, 40).is_empty());
        assert!(
            budget.record(1, 40).is_empty(),
            "reusing an image only refreshes its stamp"
        );
        assert_eq!(
            budget.record(3, 40),
            vec![2],
            "the least recently used image leaves first"
        );
        assert_eq!(budget.bytes(), 80);
    }

    #[test]
    fn an_image_larger_than_the_budget_is_kept() {
        let mut budget = ImageBudget::new(10);
        assert!(budget.record(1, 40).is_empty());
        assert_eq!(budget.bytes(), 40, "one image must survive to be painted");
    }
}
