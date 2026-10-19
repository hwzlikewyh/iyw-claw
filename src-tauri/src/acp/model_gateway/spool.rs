use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use axum::body::{Body, Bytes};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use super::error::GatewayError;
use super::limits::{MAX_BODY_BYTES, MEMORY_BODY_BYTES};

pub(super) enum Spool {
    Memory(Bytes),
    File(std::fs::File, u64),
}

impl Spool {
    pub async fn receive(body: Body) -> Result<Self, GatewayError> {
        let mut stream = body.into_data_stream();
        let mut memory = Vec::new();
        let mut disk = None;
        let mut size = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| GatewayError::input("Request body could not be read"))?;
            size += chunk.len() as u64;
            if size > MAX_BODY_BYTES {
                return Err(GatewayError::input(
                    "Request body exceeds the local gateway limit",
                ));
            }
            if disk.is_none() && size as usize > MEMORY_BODY_BYTES {
                let mut file = tokio::fs::File::from_std(tempfile::tempfile()?);
                file.write_all(&memory).await?;
                memory = Vec::new();
                disk = Some(file);
            }
            match &mut disk {
                Some(file) => file.write_all(&chunk).await?,
                None => memory.extend_from_slice(&chunk),
            }
        }
        match disk {
            Some(mut file) => {
                file.flush().await?;
                Ok(Self::File(file.into_std().await, size))
            }
            None => Ok(Self::Memory(memory.into())),
        }
    }

    pub fn len(&self) -> u64 {
        match self {
            Self::Memory(bytes) => bytes.len() as u64,
            Self::File(_, size) => *size,
        }
    }

    pub fn reader(&self) -> Result<Box<dyn Read + Send>, GatewayError> {
        self.reader_at(0)
    }

    pub fn reader_at(&self, offset: u64) -> Result<Box<dyn Read + Send>, GatewayError> {
        match self {
            Self::Memory(bytes) => {
                let mut cursor = Cursor::new(bytes.clone());
                cursor.set_position(offset);
                Ok(Box::new(cursor))
            }
            Self::File(file, _) => {
                let mut file = file.try_clone()?;
                file.seek(SeekFrom::Start(offset))?;
                Ok(Box::new(file))
            }
        }
    }

    pub fn may_contain_images(&self) -> Result<bool, GatewayError> {
        let mut reader = self.reader()?;
        let mut buffer = [0_u8; super::limits::IO_BUFFER_BYTES];
        let mut carry = Vec::new();
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                return Ok(false);
            }
            carry.extend_from_slice(&buffer[..read]);
            if image_marker(&carry) {
                return Ok(true);
            }
            let tail = carry.len().saturating_sub(8);
            carry.drain(..tail);
        }
    }

    pub fn into_body(self) -> Result<reqwest::Body, GatewayError> {
        match self {
            Self::Memory(bytes) => Ok(bytes.into()),
            Self::File(mut file, _) => {
                file.seek(SeekFrom::Start(0))?;
                Ok(reqwest::Body::wrap_stream(ReaderStream::new(
                    tokio::fs::File::from_std(file),
                )))
            }
        }
    }

    pub fn from_file(mut file: std::fs::File) -> Result<Self, GatewayError> {
        let size = file.stream_position()?;
        if size > MAX_BODY_BYTES {
            return Err(GatewayError::input("Prepared request is too large"));
        }
        file.flush()?;
        Ok(Self::File(file, size))
    }
}

fn image_marker(bytes: &[u8]) -> bool {
    bytes.iter().enumerate().any(|(index, byte)| {
        let suffix = &bytes[index..];
        match byte {
            b'd' => suffix.starts_with(b"data:"),
            b'b' => suffix.starts_with(b"base64"),
            b'\\' => suffix.starts_with(b"\\u00"),
            _ => false,
        }
    })
}
