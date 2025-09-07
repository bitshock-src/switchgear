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
    data_file: PathBuf,
    updated: Arc<Mutex<SystemTime>>,
    cache: Arc<Mutex<HashMap<DiscoveryBackendAddress, DiscoveryBackend>>>,
}

impl FileDiscoveryBackendStore {
    pub fn new<P: AsRef<Path>>(data_file: P) -> Self {
        Self {
            data_file: data_file.as_ref().to_path_buf(),
            updated: Arc::new(Mutex::new(SystemTime::UNIX_EPOCH)),
            cache: Arc::new(Default::default()),
        }
    }

    async fn open_origin_to_read(&self) -> Result<Option<File>, DiscoveryBackendStoreError> {
        match File::open(&self.data_file).await {
            Ok(file) => Ok(Some(file)),
            Err(e) => match e.kind() {
                ErrorKind::NotFound => Ok(None),
                _ => Err(DiscoveryBackendStoreError::io_error(
                    ServiceErrorSource::Internal,
                    format!("reading data file: {}", self.data_file.to_string_lossy()),
                    e,
                )),
            },
        }
    }

    async fn open_origin_to_write(&self) -> Result<File, DiscoveryBackendStoreError> {
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .append(false)
            .truncate(false)
            .create(true)
            .open(&self.data_file)
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::io_error(
                    ServiceErrorSource::Internal,
                    format!("writing data file: {}", self.data_file.to_string_lossy()),
                    e,
                )
            })?;
        Ok(file)
    }

    async fn get_origin_timestamp(origin: &File) -> Result<SystemTime, DiscoveryBackendStoreError> {
        let metadata = origin.metadata().await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "reading file metadata for data file",
                e,
            )
        })?;

        metadata.modified().map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "reading modified time for data file",
                e,
            )
        })
    }

    async fn read_through(&self) -> Result<bool, DiscoveryBackendStoreError> {
        let origin = self.open_origin_to_read().await?;
        match origin {
            // origin has been deleted
            None => {
                let mut cache = self.cache.lock().await;
                cache.clear();
                Ok(false)
            }
            Some(mut origin) => {
                origin.lock_shared().map_err(|e| {
                    DiscoveryBackendStoreError::io_error(
                        ServiceErrorSource::Internal,
                        "acquiring shared lock",
                        e,
                    )
                })?;
                let origin_timestamp = Self::get_origin_timestamp(&origin).await?;
                let mut updated = self.updated.lock().await;
                if *updated != origin_timestamp {
                    let mut cache = self.cache.lock().await;
                    Self::load_origin_into_cache(&mut origin, &mut cache).await?;
                    *updated = origin_timestamp;
                }
                Ok(true)
            }
        }
    }

    async fn acquire_write_through(&self) -> Result<File, DiscoveryBackendStoreError> {
        let mut origin = self.open_origin_to_write().await?;
        origin.lock_exclusive().map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "acquiring exclusive lock",
                e,
            )
        })?;

        let origin_timestamp = Self::get_origin_timestamp(&origin).await?;
        let mut updated = self.updated.lock().await;
        if *updated != origin_timestamp {
            let mut cache = self.cache.lock().await;
            Self::load_origin_into_cache(&mut origin, &mut cache).await?;
            *updated = origin_timestamp;
        }

        Ok(origin)
    }

    async fn write_through(&self, origin: &mut File) -> Result<(), DiscoveryBackendStoreError> {
        let cache = self.cache.lock().await;
        Self::write_cache_into_origin(origin, &cache).await?;
        let origin_timestamp = Self::get_origin_timestamp(origin).await?;
        let mut updated = self.updated.lock().await;
        *updated = origin_timestamp;
        Ok(())
    }

    async fn load_origin_into_cache(
        origin: &mut File,
        cache: &mut HashMap<DiscoveryBackendAddress, DiscoveryBackend>,
    ) -> Result<(), DiscoveryBackendStoreError> {
        let mut buf = String::new();
        origin.read_to_string(&mut buf).await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "reading data file",
                e,
            )
        })?;

        let discovery_backends: Vec<DiscoveryBackend> = if buf.is_empty() {
            vec![]
        } else {
            serde_json::from_str(&buf).map_err(|e| {
                DiscoveryBackendStoreError::json_serialization_error(
                    ServiceErrorSource::Internal,
                    "deserializing data file json",
                    e,
                )
            })?
        };

        let discovery_backends = discovery_backends
            .into_iter()
            .map(|b| (b.address.clone(), b));

        cache.clear();
        cache.extend(discovery_backends);

        Ok(())
    }

    async fn write_cache_into_origin(
        origin: &mut File,
        cache: &HashMap<DiscoveryBackendAddress, DiscoveryBackend>,
    ) -> Result<(), DiscoveryBackendStoreError> {
        let backends: Vec<DiscoveryBackend> = cache.values().cloned().collect();

        let content = serde_json::to_string_pretty(&backends).map_err(|e| {
            DiscoveryBackendStoreError::json_serialization_error(
                ServiceErrorSource::Internal,
                "serializing data",
                e,
            )
        })?;

        // Truncate the file to ensure no trailing content remains
        origin.set_len(0).await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "truncating data file",
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
                    "seeking to start of data file",
                    e,
                )
            })?;

        origin.write_all(content.as_bytes()).await.map_err(|e| {
            DiscoveryBackendStoreError::io_error(
                ServiceErrorSource::Internal,
                "writing data file",
                e,
            )
        })?;

        Ok(())
    }
}

#[async_trait]
impl DiscoveryBackendStore for FileDiscoveryBackendStore {
    type Error = DiscoveryBackendStoreError;

    async fn get(
        &self,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error> {
        if !self.read_through().await? {
            return Ok(None);
        }
        let cache = self.cache.lock().await;
        Ok(cache.get(addr).cloned())
    }

    async fn get_all(&self) -> Result<Vec<DiscoveryBackend>, Self::Error> {
        if !self.read_through().await? {
            return Ok(vec![]);
        }
        let cache = self.cache.lock().await;
        Ok(cache.values().cloned().collect())
    }

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error> {
        let mut origin = self.acquire_write_through().await?;
        let mut cache = self.cache.lock().await;
        if cache.contains_key(&backend.address) {
            return Ok(None);
        }
        let addr = backend.address.clone();
        cache.insert(addr.clone(), backend);
        drop(cache); // Release lock before write_through
        self.write_through(&mut origin).await?;
        Ok(Some(addr))
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let mut origin = self.acquire_write_through().await?;
        let mut cache = self.cache.lock().await;
        let was_new = cache.insert(backend.address.clone(), backend).is_none();
        drop(cache); // Release lock before write_through
        self.write_through(&mut origin).await?;
        Ok(was_new)
    }

    async fn delete(&self, addr: &DiscoveryBackendAddress) -> Result<bool, Self::Error> {
        let mut origin = self.acquire_write_through().await?;
        let mut cache = self.cache.lock().await;
        let was_found = cache.remove(addr).is_some();
        drop(cache); // Release lock before write_through
        self.write_through(&mut origin).await?;
        Ok(was_found)
    }
}
