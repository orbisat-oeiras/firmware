use core::{mem::MaybeUninit, ptr::NonNull};

use bbqueue::{
    BBQueue,
    prod_cons::framed::{FramedConsumer, FramedGrantR, FramedGrantW, FramedProducer},
    traits::{coordination::cs::CsCoord, notifier::maitake::MaiNotSpsc, storage::Storage},
};

pub const AUDIO_FRAME_SIZE: usize = 8192;
// Buffer roughly three seconds so SD-card latency does not immediately stall RX DMA.
pub const AUDIO_QUEUE_FRAMES: usize = 65;
pub const AUDIO_QUEUE_CAPACITY: usize = AUDIO_QUEUE_FRAMES * (AUDIO_FRAME_SIZE + 2) + 16;

pub struct AudioStorage {
    pointer: NonNull<u8>,
    length: usize,
}

impl AudioStorage {
    pub fn new(backing: &'static mut [MaybeUninit<u8>]) -> Self {
        assert!(backing.len() >= AUDIO_QUEUE_CAPACITY);
        let pointer = NonNull::new(backing.as_mut_ptr().cast()).unwrap();
        Self {
            pointer,
            length: backing.len(),
        }
    }
}

// SAFETY: Moving this handle moves only the pointer and length; the unique `'static mut`
// backing allocation remains at a stable address for the lifetime of the queue.
unsafe impl Send for AudioStorage {}

// SAFETY: The backing region is exclusively transferred to one BBQueue. All subsequent
// access goes through BBQueue's SPSC grants, whose coordination prevents overlapping
// producer/consumer regions. The pointer and length never change after construction.
unsafe impl Sync for AudioStorage {}

impl Storage for AudioStorage {
    unsafe fn ptr_len(&self) -> (NonNull<u8>, usize) {
        // SAFETY: The returned range is the same unique allocation on every call and remains
        // valid while AudioStorage (owned by the static BBQueue) exists. BBQueue coordinates
        // all access to this range through its grants.
        (self.pointer, self.length)
    }
}

pub type AudioQueue = BBQueue<AudioStorage, CsCoord, MaiNotSpsc>;
pub type AudioProducer = FramedProducer<&'static AudioQueue>;
pub type AudioConsumer = FramedConsumer<&'static AudioQueue>;
pub type AudioWriteGrant = FramedGrantW<&'static AudioQueue>;
pub type AudioReadGrant = FramedGrantR<&'static AudioQueue>;
