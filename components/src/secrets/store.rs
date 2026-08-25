use crate::secrets::error::SecretError;
use moka::sync::Cache;
use secrecy::{ExposeSecret, SecretBox, SecretSlice, SecretString};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use switchgear_error::{ErrorOrigin, ForeignContext};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SecretStoreConfig {
    pub ttl: f64,
    pub secrets: BTreeMap<String, SecretStoreSecretConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SecretStoreSecretConfig {
    pub path: PathBuf,
}

struct Inner {
    sources: BTreeMap<String, SecretStoreSecretConfig>,
    cache: Cache<String, SecretSlice<u8>>,
    ttl: Duration,
}

#[derive(Clone)]
pub struct SecretStore(Arc<Inner>);

impl std::fmt::Debug for SecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretStore")
            .field("names", &self.0.sources.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl SecretStore {
    pub fn create(config: &SecretStoreConfig) -> Self {
        let ttl = Duration::from_secs_f64(config.ttl);
        let cache = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(config.secrets.len().max(1) as u64)
            .build();

        Self(Arc::new(Inner {
            sources: config.secrets.clone(),
            cache,
            ttl,
        }))
    }

    pub fn ttl(&self) -> Duration {
        self.0.ttl
    }

    pub fn get_str(&self, name: &str) -> Result<SecretString, SecretError> {
        let bytes = self.load_or_cache(name)?;
        let s = std::str::from_utf8(bytes.expose_secret())
            .with_foreign_context(|| format!("decoding secret {name} as utf-8"), None)?;
        let trimmed = s
            .strip_suffix('\n')
            .map(|s| s.strip_suffix('\r').unwrap_or(s))
            .unwrap_or(s);
        Ok(SecretString::from(trimmed.to_owned()))
    }

    pub fn get_bytes(&self, name: &str) -> Result<SecretSlice<u8>, SecretError> {
        self.load_or_cache(name)
    }

    fn load_or_cache(&self, name: &str) -> Result<SecretSlice<u8>, SecretError> {
        if let Some(cached) = self.0.cache.get(name) {
            return Ok(cached);
        }
        let value = self.0.load(name)?;
        self.0.cache.insert(name.to_owned(), value.clone());
        Ok(value)
    }

    pub fn replace(&self, template: &str) -> Result<SecretString, SecretError> {
        let mut output = template.to_owned();
        for name in self.0.sources.keys() {
            let value = self.get_str(name)?;
            let placeholder = format!("{{{{{name}}}}}");
            output = output.replace(&placeholder, value.expose_secret());
        }
        Ok(SecretString::from(output))
    }
}

impl Inner {
    fn load(&self, name: &str) -> Result<SecretSlice<u8>, SecretError> {
        let path = &self
            .sources
            .get(name)
            .ok_or_else(|| {
                SecretError::unknown_secret(name.to_owned(), format!("loading secret {name}"))
            })?
            .path;
        let mut file = File::open(path)
            .with_foreign_context(|| format!("opening secret file {}", path.display()), None)?;
        let len = file
            .metadata()
            .with_foreign_context(|| format!("stat-ing secret file {}", path.display()), None)?
            .len();
        let len = usize::try_from(len).map_err(|_| {
            SecretError::message(
                ErrorOrigin::Downstream,
                format!(
                    "secret file {} is {len} bytes, exceeding this target's addressable memory",
                    path.display()
                ),
            )
        })?;
        let mut buf: Box<[u8]> = vec![0u8; len].into_boxed_slice();
        file.read_exact(&mut buf)
            .with_foreign_context(|| format!("reading secret file {}", path.display()), None)?;
        Ok(SecretBox::new(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::error::SecretErrorSourceKind;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, contents: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn write_bytes(dir: &TempDir, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn entry(name: &str, path: PathBuf) -> (String, SecretStoreSecretConfig) {
        (name.into(), SecretStoreSecretConfig { path })
    }

    #[test]
    fn store_ttl_reports_configured_value() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "pw", "hunter2\n");
        let config = SecretStoreConfig {
            ttl: 0.5,
            secrets: BTreeMap::from([entry("PW", path)]),
        };
        let store = SecretStore::create(&config);
        assert_eq!(store.ttl(), Duration::from_millis(500));
    }

    #[test]
    fn get_str_resolves_and_trims_trailing_newline() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "pw", "hunter2\n");
        let config = SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([entry("PW", path)]),
        };
        let store = SecretStore::create(&config);
        assert_eq!(store.get_str("PW").unwrap().expose_secret(), "hunter2");
    }

    #[test]
    fn get_str_trims_trailing_crlf() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "pw", "hunter2\r\n");
        let config = SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([entry("PW", path)]),
        };
        let store = SecretStore::create(&config);
        assert_eq!(store.get_str("PW").unwrap().expose_secret(), "hunter2");
    }

    #[test]
    fn ttl_expiry_reloads_value() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "pw", "one\n");
        let config = SecretStoreConfig {
            ttl: 0.1,
            secrets: BTreeMap::from([entry("K", path.clone())]),
        };
        let store = SecretStore::create(&config);
        assert_eq!(store.get_str("K").unwrap().expose_secret(), "one");
        fs::write(&path, "two\n").unwrap();
        assert_eq!(store.get_str("K").unwrap().expose_secret(), "one");
        std::thread::sleep(Duration::from_millis(200));
        store.0.cache.run_pending_tasks();
        assert_eq!(store.get_str("K").unwrap().expose_secret(), "two");
    }

    #[test]
    fn replace_returns_secret_string_with_substituted_values() {
        let dir = TempDir::new().unwrap();
        let user = write(&dir, "user", "root\n");
        let password = write(&dir, "password", "hunter2\n");
        let config = SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([entry("USER", user), entry("PASSWORD", password)]),
        };
        let store = SecretStore::create(&config);
        let out = store.replace("mysql://{{USER}}:{{PASSWORD}}@host").unwrap();
        assert_eq!(out.expose_secret(), "mysql://root:hunter2@host");
    }

    #[test]
    fn replace_leaves_unknown_placeholders_unchanged() {
        let dir = TempDir::new().unwrap();
        let a = write(&dir, "a", "x\n");
        let config = SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([entry("A", a)]),
        };
        let store = SecretStore::create(&config);
        let out = store.replace("{{A}}-{{MISSING}}").unwrap();
        assert_eq!(out.expose_secret(), "x-{{MISSING}}");
    }

    #[test]
    fn get_unknown_secret_returns_unknown_secret_error() {
        let dir = TempDir::new().unwrap();
        let a = write(&dir, "a", "x\n");
        let config = SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([entry("A", a)]),
        };
        let store = SecretStore::create(&config);
        let err = store.get_str("MISSING").unwrap_err();
        assert!(matches!(
            err.source_kind(),
            Some(SecretErrorSourceKind::UnknownSecret(n)) if n == "MISSING"
        ));
    }

    #[test]
    fn get_bytes_returns_raw_file_contents_including_trailing_newline() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "pw", "mysql\n");
        let config = SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([entry("PW", path)]),
        };
        let store = SecretStore::create(&config);
        let bytes = store.get_bytes("PW").unwrap();
        assert_eq!(bytes.expose_secret(), b"mysql\n");
        assert_eq!(store.get_str("PW").unwrap().expose_secret(), "mysql");
    }

    #[test]
    fn get_str_returns_utf8_error_for_non_utf8_value_file() {
        let dir = TempDir::new().unwrap();
        let path = write_bytes(&dir, "pw", &[0xFF, 0xFE, 0xFD]);
        let config = SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([entry("PW", path)]),
        };
        let store = SecretStore::create(&config);
        let err = store.get_str("PW").unwrap_err();
        assert!(matches!(
            err.source_kind(),
            Some(SecretErrorSourceKind::Utf8(_))
        ));
        assert_eq!(
            store.get_bytes("PW").unwrap().expose_secret(),
            &[0xFF, 0xFE, 0xFD]
        );
    }

    #[test]
    fn get_bytes_ttl_expiry_reloads_bytes() {
        let dir = TempDir::new().unwrap();
        let path = write(&dir, "pw", "one\n");
        let config = SecretStoreConfig {
            ttl: 0.1,
            secrets: BTreeMap::from([entry("K", path.clone())]),
        };
        let store = SecretStore::create(&config);
        assert_eq!(store.get_bytes("K").unwrap().expose_secret(), b"one\n");
        fs::write(&path, "two\n").unwrap();
        assert_eq!(store.get_bytes("K").unwrap().expose_secret(), b"one\n");
        std::thread::sleep(Duration::from_millis(200));
        store.0.cache.run_pending_tasks();
        assert_eq!(store.get_bytes("K").unwrap().expose_secret(), b"two\n");
    }
}
