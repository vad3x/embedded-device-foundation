use edf_core::ports::OtaStream;

/// Forwards bytes buffered during consumer-owned request framing exactly once.
pub struct PrefixedStream<'s, 'p, S: OtaStream> {
    stream: &'s mut S,
    prefix: &'p [u8],
    consumed: usize,
}

impl<'s, 'p, S: OtaStream> PrefixedStream<'s, 'p, S> {
    pub fn new(stream: &'s mut S, prefix: &'p [u8]) -> Self {
        Self {
            stream,
            prefix,
            consumed: 0,
        }
    }
}

impl<S: OtaStream> OtaStream for PrefixedStream<'_, '_, S> {
    async fn read(&mut self, out: &mut [u8]) -> Result<usize, ()> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.consumed < self.prefix.len() {
            let count = out.len().min(self.prefix.len() - self.consumed);
            out[..count].copy_from_slice(&self.prefix[self.consumed..self.consumed + count]);
            self.consumed += count;
            Ok(count)
        } else {
            self.stream.read(out).await
        }
    }
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()> {
        self.stream.write_all(buf).await
    }
    async fn flush(&mut self) -> Result<(), ()> {
        self.stream.flush().await
    }
}

#[cfg(test)]
mod tests {
    use crate::PrefixedStream;
    use edf_core::ports::OtaStream;
    use futures::executor::block_on;

    struct Tail {
        reads: usize,
    }
    impl OtaStream for Tail {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
            self.reads += 1;
            buf[0] = b'c';
            Ok(1)
        }
        async fn write_all(&mut self, _: &[u8]) -> Result<(), ()> {
            Ok(())
        }
        async fn flush(&mut self) -> Result<(), ()> {
            Ok(())
        }
    }

    #[test]
    fn prefix_bytes_arrive_once_then_stream_continues() {
        let mut tail = Tail { reads: 0 };
        let mut prefixed = PrefixedStream::new(&mut tail, b"ab");
        let mut one = [0; 1];
        block_on(prefixed.read(&mut one)).unwrap();
        assert_eq!(one, [b'a']);
        block_on(prefixed.read(&mut one)).unwrap();
        assert_eq!(one, [b'b']);
        block_on(prefixed.read(&mut one)).unwrap();
        assert_eq!(one, [b'c']);
        assert_eq!(tail.reads, 1);
    }
}
