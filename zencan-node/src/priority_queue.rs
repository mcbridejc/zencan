//! A prioritized queue for handling CAN messages
use core::{cell::RefCell, mem::MaybeUninit};

use critical_section::Mutex;

#[derive(Clone, Copy, Debug)]
struct Prio<T: Copy>(u32, MaybeUninit<T>);

impl<T: Copy> Prio<T> {
    const EMPTY: Prio<T> = Prio(1 << 31, MaybeUninit::uninit());

    pub fn new(prio: u32, value: T) -> Self {
        let prio = prio & 0x7FFFFFFF;
        Self(prio, MaybeUninit::new(value))
    }

    pub fn is_empty(&self) -> bool {
        self.0 & (1 << 31) != 0
    }

    pub fn prio(&self) -> Option<u32> {
        if self.is_empty() {
            None
        } else {
            Some(self.0)
        }
    }

    pub fn value(&self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(unsafe { self.1.assume_init() })
        }
    }

    pub fn take(&mut self) -> Option<T> {
        let value = self.value();
        *self = Prio::EMPTY;
        value
    }
}

/// A simple prioritized queue
#[derive(Debug)]
pub struct PriorityQueue<const N: usize, T: Copy> {
    buffer: Mutex<RefCell<[Prio<T>; N]>>,
}

impl<const N: usize, T: Copy + Send> Default for PriorityQueue<N, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, T> PriorityQueue<N, T>
where
    T: Copy + Send,
{
    /// Create a new PriorityQueue
    pub const fn new() -> Self {
        Self {
            buffer: Mutex::new(RefCell::new([Prio::EMPTY; N])),
        }
    }

    /// Write an item to the queue
    ///
    /// # Arguments
    /// - `prio`: The priority of the item. Lower priority values will be read first. Bit 31 is
    ///   reserved and must always be zero, so the maximum priority value is (2**31-1)
    /// - `item`: The item to queue
    pub fn push(&self, prio: u32, item: T) -> Result<(), T> {
        critical_section::with(|cs| {
            let mut buffer = self.buffer.borrow_ref_mut(cs);
            for loc in buffer.iter_mut() {
                if loc.is_empty() {
                    *loc = Prio::new(prio, item);
                    return Ok(());
                }
            }

            Err(item)
        })
    }

    /// Remove the queue item with the lowest priority value
    ///
    /// Returns: The item with the lowest priority value in the queue, or None if the queue is empty
    pub fn pop(&self) -> Option<T> {
        critical_section::with(|cs| {
            let mut min_prio = u32::MAX;
            let mut selected_index = None;
            let mut buffer = self.buffer.borrow_ref_mut(cs);
            // Traverse the list and find the lowest priority
            for (i, loc) in buffer.iter().enumerate() {
                if let Some(prio) = loc.prio() {
                    if prio < min_prio {
                        min_prio = prio;
                        selected_index = Some(i);
                    }
                }
            }

            selected_index.map(|i| buffer[i].take())?
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_priority_queue() {
        let queue: PriorityQueue<4, u8> = PriorityQueue::new();

        queue.push(87, 2).unwrap();
        queue.push(100, 3).unwrap();
        queue.push(1, 0).unwrap();
        queue.push(10, 1).unwrap();

        // Now the queue is full
        assert_eq!(Err(12), queue.push(100, 12));

        assert_eq!(Some(0), queue.pop());
        assert_eq!(Some(1), queue.pop());
        assert_eq!(Some(2), queue.pop());
        assert_eq!(Some(3), queue.pop());
    }
}
