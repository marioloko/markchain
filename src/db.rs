use std::marker::PhantomData;
use std::path::Path;

use serde::de::Deserialize;
use serde::ser::Serialize;

use crate::error::Result;

#[derive(Debug)]
pub struct Db {
    inner: sled::Db,
}

impl Db {
    pub fn open<P: AsRef<Path>>(db_path: &P) -> Result<Db> {
        Ok(Db {
            inner: sled::open(db_path)?,
        })
    }

    pub fn open_tree<K, V>(&self, namespace: &str) -> Result<Tree<K, V>> {
        Ok(Tree {
            inner: self.inner.open_tree(namespace)?,
            _key: PhantomData,
            _value: PhantomData,
        })
    }

    pub fn open_byte_tree(&self, namespace: &str) -> Result<ByteTree> {
        Ok(ByteTree {
            inner: self.inner.open_tree(namespace)?,
        })
    }
}

#[derive(Debug)]
pub struct Tree<K, V> {
    inner: sled::Tree,

    // Force types to be the same during the whole DB lifetime.
    // This is to ensure that get deserializes keys and values as the same type
    // they were inserted.
    _key: PhantomData<K>,
    _value: PhantomData<V>,
}

impl<K, V> Tree<K, V>
where
    K: Serialize,
    V: Serialize + for<'a> Deserialize<'a>,
{
    pub fn get(&self, key: &K) -> Result<Option<V>> {
        let key_bytes = serde_json::to_vec(&key)?;
        let value = match self.inner.get(key_bytes)? {
            None => None,
            Some(value_bytes) => Some(serde_json::from_slice(&value_bytes)?),
        };
        Ok(value)
    }

    pub fn insert(&self, key: &K, value: &V) -> Result<Option<V>> {
        let key_bytes = serde_json::to_vec(key)?;
        let value_bytes = serde_json::to_vec(value)?;
        let previous_value = match self.inner.insert(key_bytes, value_bytes)? {
            None => None,
            Some(value_bytes) => Some(serde_json::from_slice(&value_bytes)?),
        };
        Ok(previous_value)
    }

    pub fn last_value(&self) -> Result<Option<V>> {
        let last_value = match self.inner.last()? {
            None => None,
            Some((_, value_bytes)) => Some(serde_json::from_slice(&value_bytes)?),
        };

        Ok(last_value)
    }
}

#[derive(Debug)]
pub struct ByteTree {
    inner: sled::Tree,
}

impl ByteTree {
    pub fn get<K>(&self, key: K) -> Result<Option<sled::IVec>>
    where
        K: AsRef<[u8]>,
    {
        let value = self.inner.get(key.as_ref())?;
        Ok(value)
    }

    pub fn insert<K, V>(&self, key: K, value: V) -> Result<Option<sled::IVec>>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let previous_value = self.inner.insert(key.as_ref(), value.as_ref())?;
        Ok(previous_value)
    }
}
