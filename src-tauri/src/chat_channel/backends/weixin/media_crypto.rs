use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use base64::{engine::general_purpose::STANDARD, Engine};

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::failure;

const BLOCK_BYTES: usize = 16;

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn unhex(value: &str) -> Result<Vec<u8>, ChatChannelError> {
    if !value.is_ascii() || value.len() % 2 != 0 {
        return Err(failure("Invalid media key encoding"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let part = std::str::from_utf8(chunk).map_err(|_| failure("Invalid media key"))?;
            u8::from_str_radix(part, 16).map_err(|_| failure("Invalid media key"))
        })
        .collect()
}

pub(super) fn encrypt(bytes: &[u8], key: &[u8]) -> Result<Vec<u8>, ChatChannelError> {
    let cipher =
        aes::Aes128::new_from_slice(key).map_err(|_| failure("Invalid Weixin media key"))?;
    let padding = BLOCK_BYTES - bytes.len() % BLOCK_BYTES;
    let mut result = bytes.to_vec();
    result.resize(bytes.len() + padding, padding as u8);
    for chunk in result.chunks_exact_mut(BLOCK_BYTES) {
        cipher.encrypt_block(GenericArray::from_mut_slice(chunk));
    }
    Ok(result)
}

pub(super) fn decrypt(mut bytes: Vec<u8>, key: &str) -> Result<Vec<u8>, ChatChannelError> {
    let decoded = STANDARD
        .decode(key)
        .map_err(|_| failure("Invalid Weixin media key encoding"))?;
    let key = if decoded.len() == BLOCK_BYTES {
        decoded
    } else {
        unhex(std::str::from_utf8(&decoded).map_err(|_| failure("Invalid Weixin media key"))?)?
    };
    let cipher =
        aes::Aes128::new_from_slice(&key).map_err(|_| failure("Invalid Weixin media key"))?;
    if bytes.is_empty() || bytes.len() % BLOCK_BYTES != 0 {
        return Err(failure("Invalid Weixin encrypted media"));
    }
    for chunk in bytes.chunks_exact_mut(BLOCK_BYTES) {
        cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
    }
    let padding = bytes.last().copied().unwrap_or_default() as usize;
    if padding == 0
        || padding > BLOCK_BYTES
        || bytes.len() < padding
        || !bytes[bytes.len() - padding..]
            .iter()
            .all(|b| *b as usize == padding)
    {
        return Err(failure("Weixin media decryption failed"));
    }
    bytes.truncate(bytes.len() - padding);
    Ok(bytes)
}
