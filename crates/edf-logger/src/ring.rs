//! Fixed-capacity retention of the newest log bytes.

/// A byte ring that retains the newest `N` bytes without allocation.
pub struct LogRingBuffer<const N: usize> {
    storage: [u8; N],
    write_pos: usize,
    count: usize,
}

impl<const N: usize> LogRingBuffer<N> {
    /// Creates an empty ring.
    pub const fn new() -> Self {
        Self {
            storage: [0; N],
            write_pos: 0,
            count: 0,
        }
    }

    /// Appends bytes, discarding the oldest bytes after capacity is reached.
    pub fn write_str(&mut self, text: &str) {
        if N == 0 {
            return;
        }
        for &byte in text.as_bytes() {
            self.storage[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % N;
            self.count = (self.count + 1).min(N);
        }
    }

    /// Copies the newest bytes that fit into `buf` in chronological order.
    pub fn read_into(&self, buf: &mut [u8]) -> usize {
        if N == 0 {
            return 0;
        }
        let to_copy = self.count.min(buf.len());
        let start = (self.write_pos + N - to_copy) % N;
        for (offset, slot) in buf[..to_copy].iter_mut().enumerate() {
            *slot = self.storage[(start + offset) % N];
        }
        to_copy
    }

    /// Discards all retained bytes.
    pub fn clear(&mut self) {
        self.write_pos = 0;
        self.count = 0;
    }
}

impl<const N: usize> Default for LogRingBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> core::fmt::Write for LogRingBuffer<N> {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        LogRingBuffer::write_str(self, text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn ring_overwrites_oldest_bytes() {
        let mut ring = LogRingBuffer::<16>::new();
        ring.write_str("12345678");
        ring.write_str("abcdefghij");
        let mut output = [0; 32];
        let count = ring.read_into(&mut output);
        assert_eq!(&output[..count], b"345678abcdefghij");
    }

    #[test]
    fn ring_partial_read_returns_latest_bytes_in_order() {
        let mut ring = LogRingBuffer::<16>::new();
        ring.write_str("abcdefghijklmnopqr");
        let mut output = [0; 4];
        let count = ring.read_into(&mut output);
        assert_eq!(&output[..count], b"opqr");
    }

    #[test]
    fn ring_zero_capacity_and_clear() {
        let mut zero = LogRingBuffer::<0>::new();
        zero.write_str("ignored");
        assert_eq!(zero.read_into(&mut [0; 4]), 0);

        let mut ring = LogRingBuffer::<8>::new();
        ring.write_str("hello");
        ring.clear();
        assert_eq!(ring.read_into(&mut [0; 8]), 0);
    }

    #[test]
    fn ring_supports_fmt_write() {
        let mut ring = LogRingBuffer::<8>::new();
        write!(&mut ring, "ab:{}", 42).unwrap();
        let mut output = [0; 8];
        let count = ring.read_into(&mut output);
        assert_eq!(&output[..count], b"ab:42");
    }
}
