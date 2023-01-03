use crate::error::Result;
use serde::de::Deserialize;
use serde::ser::Serialize;
use std::marker::PhantomData;
use std::path::Path;

#[derive(Debug)]
pub struct Db<K, V> {
    db: sled::Db,

    // Force types to be the same during the whole DB lifetime.
    // This is to ensure that get deserializes keys and values as the same type
    // they were inserted.
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> Db<K, V>
where
    K: Serialize,
    V: Serialize + for<'a> Deserialize<'a>,
{
    pub fn open<P: AsRef<Path>>(db_path: &P) -> Result<Db<K, V>> {
        Ok(Db {
            db: sled::open(db_path)?,
            _phantom: PhantomData,
        })
    }

    pub fn get(&self, key: &K) -> Result<Option<V>> {
        let key_bytes = serde_json::to_vec(&key)?;
        let value = match self.db.get(key_bytes)? {
            None => None,
            Some(value_bytes) => Some(serde_json::from_slice(&value_bytes)?),
        };
        Ok(value)
    }

    pub fn insert(&self, key: &K, value: &V) -> Result<Option<V>> {
        let key_bytes = serde_json::to_vec(key)?;
        let value_bytes = serde_json::to_vec(value)?;
        let previous_value = match self.db.insert(key_bytes, value_bytes)? {
            None => None,
            Some(value_bytes) => Some(serde_json::from_slice(&value_bytes)?),
        };
        Ok(previous_value)
    }

    pub fn last_value(&self) -> Result<Option<V>> {
        let last_value = match self.db.last()? {
            None => None,
            Some((_, value_bytes)) => Some(serde_json::from_slice(&value_bytes)?),
        };

        Ok(last_value)
    }
}
