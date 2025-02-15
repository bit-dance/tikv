// Copyright 2021 TiKV Project Authors. Licensed under Apache-2.0.

use async_trait::async_trait;
use derive_more::Deref;
use kvproto::encryptionpb::MasterKeyKms;
use tikv_util::box_err;

use crate::error::{Error, KmsError, Result};

#[derive(Debug, Clone)]
pub struct Location {
    pub region: String,
    pub endpoint: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub key_id: KeyId,
    pub location: Location,
    pub vendor: String,
}

impl Config {
    pub fn from_proto(mk: MasterKeyKms) -> Result<Self> {
        Ok(Config {
            key_id: KeyId::new(mk.key_id)?,
            location: Location {
                region: mk.region,
                endpoint: mk.endpoint,
            },
            vendor: mk.vendor,
        })
    }
}

#[derive(PartialEq, Debug, Clone, Deref)]
pub struct KeyId(String);

// KeyID is a newtype to mark a String as an ID of a key
// This ID exists in a foreign system such as AWS
// The key id must be non-empty
impl KeyId {
    pub fn new(id: String) -> Result<KeyId> {
        if id.is_empty() {
            let msg = "KMS key id can not be empty";
            Err(Error::KmsError(KmsError::EmptyKey(msg.to_owned())))
        } else {
            Ok(KeyId(id))
        }
    }
}

// EncryptedKey is a newtype used to mark data as an encrypted key
// It requires the vec to be non-empty
#[derive(PartialEq, Clone, Debug, Deref)]
pub struct EncryptedKey(Vec<u8>);

impl EncryptedKey {
    pub fn new(key: Vec<u8>) -> Result<Self> {
        if key.is_empty() {
            Err(Error::KmsError(KmsError::EmptyKey(
                "Encrypted Key".to_owned(),
            )))
        } else {
            Ok(Self(key))
        }
    }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CryptographyType {
    Plain = 0,
    AesGcm256,
    // ..
}

impl CryptographyType {
    #[inline]
    pub fn target_key_size(&self) -> usize {
        match self {
            CryptographyType::Plain => 0, // Plain text has no limitation
            CryptographyType::AesGcm256 => 32,
        }
    }
}

// PlainKey is a newtype used to mark a vector a plaintext key.
// It requires the vec to be a valid AesGcmCrypter key.
pub struct PlainKey {
    tag: CryptographyType,
    key: Vec<u8>,
}

impl PlainKey {
    pub fn new(key: Vec<u8>, t: CryptographyType) -> Result<Self> {
        let limitation = t.target_key_size();
        if limitation > 0 && key.len() != limitation {
            Err(Error::KmsError(KmsError::Other(box_err!(
                "encryption method and key length mismatch, expect {} get
                    {}",
                limitation,
                key.len()
            ))))
        } else {
            Ok(Self { key, tag: t })
        }
    }

    pub fn key_tag(&self) -> CryptographyType {
        self.tag
    }
}

impl core::ops::Deref for PlainKey {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.key
    }
}

// Don't expose the key in a debug print
impl std::fmt::Debug for PlainKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PlainKey")
            .field(&"REDACTED".to_string())
            .finish()
    }
}

#[derive(Debug)]
pub struct DataKeyPair {
    pub encrypted: EncryptedKey,
    pub plaintext: PlainKey,
}

/// `Key Management Service Provider`, serving for managing master key on
/// different cloud.
#[async_trait]
pub trait KmsProvider: Sync + Send + 'static + std::fmt::Debug {
    async fn generate_data_key(&self) -> Result<DataKeyPair>;
    async fn decrypt_data_key(&self, data_key: &EncryptedKey) -> Result<Vec<u8>>;
    fn name(&self) -> &str;
}
