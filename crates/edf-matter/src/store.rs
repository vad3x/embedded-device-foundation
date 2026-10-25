//! Split Wi-Fi credentials from Matter persistence.

use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, NETWORKS_KEY};

/// Routes only `NETWORKS_KEY` to Wi-Fi storage; all other keys use Matter storage.
pub struct PartitionedWifiStore<M, W> {
    pub matter_store: M,
    pub wifi_store: W,
}

impl<M, W> PartitionedWifiStore<M, W> {
    pub const fn new(matter_store: M, wifi_store: W) -> Self {
        Self {
            matter_store,
            wifi_store,
        }
    }
}

impl<M: KvBlobStore, W: KvBlobStore> KvBlobStore for PartitionedWifiStore<M, W> {
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
        if key == NETWORKS_KEY {
            self.wifi_store.load(key, buf)
        } else {
            self.matter_store.load(key, buf)
        }
    }

    fn store(&mut self, key: u16, data: &[u8], buf: &mut [u8]) -> Result<(), Error> {
        if key == NETWORKS_KEY {
            self.wifi_store.store(key, data, buf)
        } else {
            self.matter_store.store(key, data, buf)
        }
    }

    fn remove(&mut self, key: u16, buf: &mut [u8]) -> Result<(), Error> {
        if key == NETWORKS_KEY {
            self.wifi_store.remove(key, buf)
        } else {
            self.matter_store.remove(key, buf)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::store::PartitionedWifiStore;
    use rs_matter::error::{Error, ErrorCode};
    use rs_matter::persist::{KvBlobStore, BASIC_INFO_KEY, NETWORKS_KEY};

    #[derive(Default)]
    struct MemoryStore {
        key: Option<u16>,
        value: [u8; 32],
        len: usize,
        fail: bool,
    }

    impl KvBlobStore for MemoryStore {
        fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
            if self.fail {
                return Err(ErrorCode::Invalid.into());
            }
            if self.key != Some(key) {
                return Ok(None);
            }
            if buf.len() < self.len {
                return Err(ErrorCode::BufferTooSmall.into());
            }
            buf[..self.len].copy_from_slice(&self.value[..self.len]);
            Ok(Some(&buf[..self.len]))
        }
        fn store(&mut self, key: u16, data: &[u8], _buf: &mut [u8]) -> Result<(), Error> {
            if self.fail {
                return Err(ErrorCode::Invalid.into());
            }
            if data.len() > self.value.len() {
                return Err(ErrorCode::BufferTooSmall.into());
            }
            self.key = Some(key);
            self.value[..data.len()].copy_from_slice(data);
            self.len = data.len();
            Ok(())
        }
        fn remove(&mut self, key: u16, _buf: &mut [u8]) -> Result<(), Error> {
            if self.fail {
                return Err(ErrorCode::Invalid.into());
            }
            if self.key == Some(key) {
                self.key = None;
            }
            Ok(())
        }
    }

    #[test]
    fn networks_key_routes_only_to_wifi_for_all_operations() {
        assert_eq!(NETWORKS_KEY, 258);
        let mut store = PartitionedWifiStore::new(MemoryStore::default(), MemoryStore::default());
        let mut scratch = [0; 32];
        store.store(NETWORKS_KEY, b"wifi", &mut scratch).unwrap();
        assert_eq!(
            store.matter_store.load(NETWORKS_KEY, &mut scratch).unwrap(),
            None
        );
        assert_eq!(
            store.load(NETWORKS_KEY, &mut scratch).unwrap(),
            Some(b"wifi".as_slice())
        );
        store.remove(NETWORKS_KEY, &mut scratch).unwrap();
        assert_eq!(store.load(NETWORKS_KEY, &mut scratch).unwrap(), None);
    }

    #[test]
    fn wifi_store_errors_and_caller_buffer_limits_are_preserved() {
        let mut store = PartitionedWifiStore::new(MemoryStore::default(), MemoryStore::default());
        let mut scratch = [0; 32];
        store
            .store(NETWORKS_KEY, b"wifi-secret", &mut scratch)
            .unwrap();
        assert!(store.load(NETWORKS_KEY, &mut [0; 2]).is_err());
        store.wifi_store.fail = true;
        assert!(store.load(NETWORKS_KEY, &mut scratch).is_err());
        assert!(store.store(NETWORKS_KEY, b"x", &mut scratch).is_err());
        assert!(store.remove(NETWORKS_KEY, &mut scratch).is_err());
    }

    #[test]
    fn all_other_keys_route_to_matter_and_errors_propagate() {
        let mut store = PartitionedWifiStore::new(MemoryStore::default(), MemoryStore::default());
        let mut scratch = [0; 32];
        store
            .store(BASIC_INFO_KEY, b"matter", &mut scratch)
            .unwrap();
        assert_eq!(
            store.wifi_store.load(BASIC_INFO_KEY, &mut scratch).unwrap(),
            None
        );
        assert_eq!(
            store.load(BASIC_INFO_KEY, &mut scratch).unwrap(),
            Some(b"matter".as_slice())
        );
        store.matter_store.fail = true;
        assert!(store.load(BASIC_INFO_KEY, &mut scratch).is_err());
        assert!(store.store(BASIC_INFO_KEY, b"x", &mut scratch).is_err());
        assert!(store.remove(BASIC_INFO_KEY, &mut scratch).is_err());
    }
}
