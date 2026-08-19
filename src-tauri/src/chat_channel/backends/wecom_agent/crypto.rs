use aes::Aes256;
use base64::{
    alphabet,
    engine::{general_purpose::STANDARD, GeneralPurpose, GeneralPurposeConfig},
    Engine as _,
};
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use quick_xml::events::{BytesCData, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use rand::RngCore;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use subtle::ConstantTimeEq;

use crate::chat_channel::error::ChatChannelError;

const RANDOM_PREFIX_BYTES: usize = 16;
const MESSAGE_LENGTH_BYTES: usize = 4;
const PKCS7_BLOCK_BYTES: usize = 32;
const AES_BLOCK_BYTES: usize = 16;

type Aes256CbcDecryptor = cbc::Decryptor<Aes256>;
type Aes256CbcEncryptor = cbc::Encryptor<Aes256>;

#[derive(Debug, Deserialize)]
pub struct EncryptedEnvelope {
    #[serde(rename = "Encrypt")]
    pub encrypt: String,
}

#[derive(Debug, Deserialize)]
pub struct WecomInboundMessage {
    #[serde(rename = "ToUserName")]
    pub to_user_name: String,
    #[serde(rename = "FromUserName")]
    pub from_user_name: String,
    #[serde(rename = "CreateTime")]
    pub create_time: i64,
    #[serde(rename = "MsgType")]
    pub msg_type: String,
    #[serde(rename = "Content", default)]
    pub content: String,
    #[serde(rename = "MsgId", default)]
    pub msg_id: String,
    #[serde(rename = "AgentID", default)]
    pub agent_id: String,
}

pub fn parse_envelope(xml: &str) -> Result<EncryptedEnvelope, ChatChannelError> {
    quick_xml::de::from_str(xml).map_err(|_| invalid("callback envelope is invalid XML"))
}

pub fn parse_message(xml: &str) -> Result<WecomInboundMessage, ChatChannelError> {
    quick_xml::de::from_str(xml).map_err(|_| invalid("callback message is invalid XML"))
}

pub fn signature(token: &str, timestamp: &str, nonce: &str, encrypted: &str) -> String {
    let mut parts = [token, timestamp, nonce, encrypted];
    parts.sort_unstable();
    let mut hasher = Sha1::new();
    for part in parts {
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub fn verify_signature(
    expected: &str,
    token: &str,
    timestamp: &str,
    nonce: &str,
    encrypted: &str,
) -> Result<(), ChatChannelError> {
    let actual = signature(token, timestamp, nonce, encrypted);
    if expected.len() != actual.len()
        || expected.as_bytes().ct_eq(actual.as_bytes()).unwrap_u8() != 1
    {
        return Err(ChatChannelError::AuthenticationFailed(
            "WeCom callback signature mismatch".to_string(),
        ));
    }
    Ok(())
}

pub fn decrypt(
    encoding_aes_key: &str,
    encrypted: &str,
    expected_receive_id: &str,
) -> Result<String, ChatChannelError> {
    let key = decode_aes_key(encoding_aes_key)?;
    let mut ciphertext = STANDARD
        .decode(encrypted)
        .map_err(|_| invalid("callback ciphertext is not valid base64"))?;
    if ciphertext.is_empty() || ciphertext.len() % AES_BLOCK_BYTES != 0 {
        return Err(invalid("callback ciphertext length is invalid"));
    }
    let iv = &key[..AES_BLOCK_BYTES];
    let plaintext = Aes256CbcDecryptor::new_from_slices(&key, iv)
        .map_err(|_| invalid("callback AES key is invalid"))?
        .decrypt_padded_mut::<NoPadding>(&mut ciphertext)
        .map_err(|_| invalid("callback ciphertext cannot be decrypted"))?;
    unpack_plaintext(plaintext, expected_receive_id)
}

pub fn encrypt(
    encoding_aes_key: &str,
    message: &str,
    receive_id: &str,
) -> Result<String, ChatChannelError> {
    let key = decode_aes_key(encoding_aes_key)?;
    let message_len =
        u32::try_from(message.len()).map_err(|_| invalid("callback message is too large"))?;
    let mut plaintext = vec![0_u8; RANDOM_PREFIX_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut plaintext);
    plaintext.extend_from_slice(&message_len.to_be_bytes());
    plaintext.extend_from_slice(message.as_bytes());
    plaintext.extend_from_slice(receive_id.as_bytes());
    apply_padding(&mut plaintext);

    let iv = &key[..AES_BLOCK_BYTES];
    let length = plaintext.len();
    let encrypted = Aes256CbcEncryptor::new_from_slices(&key, iv)
        .map_err(|_| invalid("callback AES key is invalid"))?
        .encrypt_padded_mut::<NoPadding>(&mut plaintext, length)
        .map_err(|_| invalid("callback message encryption failed"))?;
    Ok(STANDARD.encode(encrypted))
}

pub fn passive_text_xml(
    to_user: &str,
    from_user: &str,
    content: &str,
    timestamp: i64,
) -> Result<String, ChatChannelError> {
    let mut writer = Writer::new(Vec::new());
    writer
        .write_event(Event::Start(BytesStart::new("xml")))
        .map_err(xml_error)?;
    write_cdata(&mut writer, "ToUserName", to_user)?;
    write_cdata(&mut writer, "FromUserName", from_user)?;
    write_text(&mut writer, "CreateTime", &timestamp.to_string())?;
    write_cdata(&mut writer, "MsgType", "text")?;
    write_cdata(&mut writer, "Content", content)?;
    writer
        .write_event(Event::End(BytesEnd::new("xml")))
        .map_err(xml_error)?;
    String::from_utf8(writer.into_inner()).map_err(|_| invalid("response XML is invalid UTF-8"))
}

pub fn encrypted_response_xml(
    encrypted: &str,
    signature: &str,
    timestamp: &str,
    nonce: &str,
) -> Result<String, ChatChannelError> {
    let mut writer = Writer::new(Vec::new());
    writer
        .write_event(Event::Start(BytesStart::new("xml")))
        .map_err(xml_error)?;
    write_cdata(&mut writer, "Encrypt", encrypted)?;
    write_cdata(&mut writer, "MsgSignature", signature)?;
    write_text(&mut writer, "TimeStamp", timestamp)?;
    write_cdata(&mut writer, "Nonce", nonce)?;
    writer
        .write_event(Event::End(BytesEnd::new("xml")))
        .map_err(xml_error)?;
    String::from_utf8(writer.into_inner()).map_err(|_| invalid("response XML is invalid UTF-8"))
}

fn decode_aes_key(value: &str) -> Result<[u8; 32], ChatChannelError> {
    if value.len() != 43 || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(invalid(
            "EncodingAESKey must contain 43 alphanumeric characters",
        ));
    }
    // The official 90968 sample has non-zero discarded trailing bits. The
    // platform SDKs accept it, so match that decoder behavior for AES keys.
    let key_engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    );
    let decoded = key_engine
        .decode(format!("{value}="))
        .map_err(|_| invalid("EncodingAESKey is not valid base64"))?;
    decoded
        .try_into()
        .map_err(|_| invalid("EncodingAESKey must decode to 32 bytes"))
}

fn unpack_plaintext(plaintext: &[u8], receive_id: &str) -> Result<String, ChatChannelError> {
    let unpadded = remove_padding(plaintext)?;
    if unpadded.len() < RANDOM_PREFIX_BYTES + MESSAGE_LENGTH_BYTES {
        return Err(invalid("callback plaintext is too short"));
    }
    let length_start = RANDOM_PREFIX_BYTES;
    let length_end = length_start + MESSAGE_LENGTH_BYTES;
    let message_len = u32::from_be_bytes(
        unpadded[length_start..length_end]
            .try_into()
            .map_err(|_| invalid("callback message length is invalid"))?,
    ) as usize;
    let message_end = length_end
        .checked_add(message_len)
        .filter(|end| *end <= unpadded.len())
        .ok_or_else(|| invalid("callback message length exceeds plaintext"))?;
    let actual_receive_id = &unpadded[message_end..];
    if actual_receive_id.ct_eq(receive_id.as_bytes()).unwrap_u8() != 1 {
        return Err(ChatChannelError::AuthenticationFailed(
            "WeCom callback ReceiveId mismatch".to_string(),
        ));
    }
    String::from_utf8(unpadded[length_end..message_end].to_vec())
        .map_err(|_| invalid("callback message is not valid UTF-8"))
}

fn apply_padding(value: &mut Vec<u8>) {
    let padding = PKCS7_BLOCK_BYTES - (value.len() % PKCS7_BLOCK_BYTES);
    value.extend(std::iter::repeat(padding as u8).take(padding));
}

fn remove_padding(value: &[u8]) -> Result<&[u8], ChatChannelError> {
    let padding = *value
        .last()
        .ok_or_else(|| invalid("callback plaintext is empty"))? as usize;
    if !(1..=PKCS7_BLOCK_BYTES).contains(&padding) || padding > value.len() {
        return Err(invalid("callback PKCS#7 padding is invalid"));
    }
    let start = value.len() - padding;
    let invalid_padding = value[start..]
        .iter()
        .fold(0_u8, |acc, byte| acc | (byte ^ padding as u8));
    if invalid_padding != 0 {
        return Err(invalid("callback PKCS#7 padding is invalid"));
    }
    Ok(&value[..start])
}

fn write_cdata(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    value: &str,
) -> Result<(), ChatChannelError> {
    writer
        .write_event(Event::Start(BytesStart::new(name)))
        .map_err(xml_error)?;
    writer
        .write_event(Event::CData(BytesCData::new(value)))
        .map_err(xml_error)?;
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .map_err(xml_error)
}

fn write_text(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    value: &str,
) -> Result<(), ChatChannelError> {
    writer
        .write_event(Event::Start(BytesStart::new(name)))
        .map_err(xml_error)?;
    writer
        .write_event(Event::Text(BytesText::new(value)))
        .map_err(xml_error)?;
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .map_err(xml_error)
}

fn xml_error(_: std::io::Error) -> ChatChannelError {
    invalid("response XML generation failed")
}

fn invalid(message: &str) -> ChatChannelError {
    ChatChannelError::ConfigurationInvalid(message.to_string())
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
