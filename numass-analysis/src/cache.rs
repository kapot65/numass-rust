use std::{fmt::Display, marker::PhantomData, path::PathBuf};

use async_trait::async_trait;
use cached::IOCachedAsync;
use serde::{Serialize, de::DeserializeOwned};

pub struct CacacheBackend<K, V> {
    root: PathBuf,
    refresh: bool,
    _phantom: PhantomData<(K, V)>,
}

impl <K, V> CacacheBackend<K, V> {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            refresh: true,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<K, V> IOCachedAsync<K, V> for CacacheBackend<K, V> where
    K: Display + Send + Sync,
    V: Serialize + DeserializeOwned + Send + Sync 
{
    type Error = eyre::Error;

    async fn cache_get(&self, key: &K) -> Result<Option<V>, Self::Error> {

        match cacache::read(&self.root, key.to_string()).await {

            Ok(bytes) => {
                Ok(Some(bincode::deserialize::<V>(&bytes)?))
            },
            Err(cacache::Error::EntryNotFound(_, _)) => {
                Ok(None)
            }
            Err(err) => {
                Err(err.into())
            }
        }
    }
    
    async fn cache_set(&self, key: K, val: V) -> Result<Option<V>, Self::Error> {
        let prev = self.cache_get(&key).await?;

        let bytes = bincode::serialize(&val)?;
        cacache::write(&self.root, key.to_string(), bytes).await?;

        Ok(prev)
    }

    async fn cache_remove(&self, key: &K) -> Result<Option<V>, Self::Error> {
        let prev = self.cache_get(key).await?;
        cacache::remove(&self.root, key.to_string()).await?;
        Ok(prev)
    }

    fn cache_set_refresh(&mut self, refresh:bool) -> bool {
        let old = self.refresh;
        self.refresh = refresh;
        old
    }
}

#[tokio::test]
async fn test_cache() {
    use cached::proc_macro::io_cached;
    #[io_cached(
        map_error = r##"|e| e"##,
        type = "CacacheBackend<String, i32>",
        create = r#"{ CacacheBackend::new(PathBuf::from("cache_dir")) }"#,
        convert = r#"{ input.to_string() }"#
    )]
    async fn cached_fn(input: i32) -> eyre::Result<i32> {
        println!("called with {}", input);
        Ok(input * 2)
    }
    cached_fn(10).await.unwrap();
    cached_fn(10).await.unwrap();
}
