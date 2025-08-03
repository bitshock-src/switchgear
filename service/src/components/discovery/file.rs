use crate::api::discovery::{DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendStore};
use crate::api::service::ServiceErrorSource;
use crate::components::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use fs4::tokio::AsyncFileExt;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct FileDiscoveryBackendStore {
    storage_dir: PathBuf,
    cache: Arc<Mutex<HashMap<String, FileDiscoveryBackendStoreCache>>>,
}

#[derive(Debug)]
struct FileDiscoveryBackendStoreCache {
    updated: SystemTime,
    store: HashMap<DiscoveryBackendAddress, DiscoveryBackend>,
}

impl FileDiscoveryBackendStore {
    pub fn new<P: AsRef<Path>>(storage_dir: P) -> Self {
        Self {
            storage_dir: storage_dir.as_ref().to_path_buf(),
            cache: Arc::new(Default::default()),
        }
    }

    async fn open_origin_to_read(
        &self,
        partition: &str,
    ) -> Result<Option<File>, DiscoveryBackendStoreError> {
        let path = self.storage_dir.join(format!("{partition}.json"));
        match File::open(&path).await {
            Ok(file) => Ok(Some(file)),
            Err(e) => match e.kind() {
                ErrorKind::NotFound => Ok(None),
                _ => Err(DiscoveryBackendStoreError::io_error(
                    ServiceErrorSource::Internal,
                    format!("reading partition file: {}", path.to_string_lossy()),
                    e,
                )),
            },
        }
    }

    async fn open_origin_to_write(
        &self,
        partition: &str,
    ) -> Result<File, DiscoveryBackendStoreError> {
        let path = self.storage_dir.join(format!("{partition}.json"));
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .append(false)
            .truncate(false)
            .create(true)
            .open(&path)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::io_error(
                    ServiceErrorSource::Internal,
                    format!("writing partition file: {}", path.to_string_lossy()),
                    e,
                )
            })?;
        Ok(file)
    }

    async fn get_origin_timestamp(origin: &File) -> Result<SystemTime, DiscoveryBackendStoreError> {
        let metadata = origin.metadata().await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "reading file metadata for partition file",
                e,
            )
        })?;

        metadata.modified().map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "reading modified time for partition file",
                e,
            )
        })
    }

    async fn read_through<'a>(
        &self,
        cache: &'a mut HashMap<String, FileDiscoveryBackendStoreCache>,
        partition: &str,
    ) -> Result<Option<&'a FileDiscoveryBackendStoreCache>, DiscoveryBackendStoreError> {
        let origin = self.open_origin_to_read(partition).await?;
        match origin {
            // origin has been deleted
            None => {
                cache.remove(partition);
                Ok(None)
            }
            Some(mut origin) => {
                origin.lock_shared().map_err(|e| {
                    DiscoveryBackendStoreError::io_error(
                        ServiceErrorSource::Internal,
                        format!("acquiring shared lock for partition: {partition}"),
                        e,
                    )
                })?;
                let store =
                    cache
                        .entry(partition.to_string())
                        .or_insert(FileDiscoveryBackendStoreCache {
                            updated: SystemTime::UNIX_EPOCH,
                            store: Default::default(),
                        });
                let origin_timestamp = Self::get_origin_timestamp(&origin).await?;
                if store.updated != origin_timestamp {
                    Self::load_origin_into_cache(&mut origin, store).await?;
                }
                Ok(Some(store))
            }
        }
    }

    async fn acquire_write_through<'a>(
        &self,
        cache: &'a mut HashMap<String, FileDiscoveryBackendStoreCache>,
        partition: &str,
    ) -> Result<(File, &'a mut FileDiscoveryBackendStoreCache), DiscoveryBackendStoreError> {
        let mut origin = self.open_origin_to_write(partition).await?;
        origin.lock_exclusive().map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                format!("acquiring exclusive lock for partition: {partition}"),
                e,
            )
        })?;

        let store = cache
            .entry(partition.to_string())
            .or_insert(FileDiscoveryBackendStoreCache {
                updated: SystemTime::UNIX_EPOCH,
                store: Default::default(),
            });

        let origin_timestamp = Self::get_origin_timestamp(&origin).await?;
        if store.updated != origin_timestamp {
            Self::load_origin_into_cache(&mut origin, store).await?;
        }

        Ok((origin, store))
    }

    async fn write_through<'a>(
        &self,
        origin: &mut File,
        store: &'a mut FileDiscoveryBackendStoreCache,
    ) -> Result<&'a mut FileDiscoveryBackendStoreCache, DiscoveryBackendStoreError> {
        Self::write_cache_into_origin(origin, store).await?;
        Ok(store)
    }

    async fn load_origin_into_cache(
        origin: &mut File,
        cache: &mut FileDiscoveryBackendStoreCache,
    ) -> Result<(), DiscoveryBackendStoreError> {
        let mut buf = String::new();
        origin.read_to_string(&mut buf).await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "reading partition file",
                e,
            )
        })?;

        let discovery_backends: Vec<DiscoveryBackend> = if buf.is_empty() {
            vec![]
        } else {
            serde_json::from_str(&buf).map_err(|e| {
                DiscoveryBackendStoreError::json_serialization_error(
                    ServiceErrorSource::Internal,
                    "deserializing partition json",
                    e,
                )
            })?
        };

        let discovery_backends = discovery_backends
            .into_iter()
            .map(|b| (b.address.clone(), b));

        cache.store.clear();
        cache.store.extend(discovery_backends);

        let timestamp = Self::get_origin_timestamp(origin).await?;
        cache.updated = timestamp;

        Ok(())
    }

    async fn write_cache_into_origin(
        origin: &mut File,
        cache: &mut FileDiscoveryBackendStoreCache,
    ) -> Result<(), DiscoveryBackendStoreError> {
        let backends: Vec<DiscoveryBackend> = cache.store.values().cloned().collect();

        let content = serde_json::to_string_pretty(&backends).map_err(|e| {
            DiscoveryBackendStoreError::json_serialization_error(
                ServiceErrorSource::Internal,
                "serializing partition",
                e,
            )
        })?;

        // Truncate the file to ensure no trailing content remains
        origin.set_len(0).await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "truncating partition file",
                e,
            )
        })?;

        // Seek to the beginning
        origin
            .seek(std::io::SeekFrom::Start(0))
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::io_error(
                    ServiceErrorSource::Internal,
                    "seeking to start of partition file",
                    e,
                )
            })?;

        origin.write_all(content.as_bytes()).await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "writing partition file",
                e,
            )
        })?;

        let origin_timestamp = Self::get_origin_timestamp(origin).await?;
        cache.updated = origin_timestamp;

        Ok(())
    }
}

#[async_trait]
impl DiscoveryBackendStore for FileDiscoveryBackendStore {
    type Error = DiscoveryBackendStoreError;

    async fn get(
        &self,
        partition: &str,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let mut cache = self.cache.lock().await;
        match self.read_through(&mut cache, partition).await? {
            None => Ok(None),
            Some(cache) => Ok(cache.store.get(addr).cloned()),
        }
    }

    async fn get_all(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        let mut cache = self.cache.lock().await;
        match self.read_through(&mut cache, partition).await? {
            None => Ok(vec![]),
            Some(cache) => Ok(cache.store.values().cloned().collect()),
        }
    }

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error> {
        let mut cache = self.cache.lock().await;
        let (mut origin, cache) = self
            .acquire_write_through(&mut cache, backend.partition.as_str())
            .await?;
        if cache.store.contains_key(&backend.address) {
            return Ok(None);
        }
        let addr = backend.address.clone();
        cache.store.insert(addr.clone(), backend);
        self.write_through(&mut origin, cache).await?;
        Ok(Some(addr))
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let mut cache = self.cache.lock().await;
        let (mut origin, cache) = self
            .acquire_write_through(&mut cache, backend.partition.as_str())
            .await?;
        let was_new = cache
            .store
            .insert(backend.address.clone(), backend)
            .is_none();
        self.write_through(&mut origin, cache).await?;
        Ok(was_new)
    }

    async fn delete(
        &self,
        partition: &str,
        addr: &DiscoveryBackendAddress,
    ) -> Result<bool, Self::Error> {
        let mut cache = self.cache.lock().await;
        let (mut origin, cache) = self.acquire_write_through(&mut cache, partition).await?;
        let was_found = cache.store.remove(addr).is_some();
        self.write_through(&mut origin, cache).await?;
        Ok(was_found)
    }
}
