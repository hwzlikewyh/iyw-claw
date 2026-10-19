use std::io::{self, Read};

pub(super) struct JsonStringReader<R>(pub R);

impl<R: Read> Read for JsonStringReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.0.read(output)?;
        if !output[..count].contains(&b'\\') {
            return Ok(count);
        }
        let mut length = 0;
        let mut offset = 0;
        while offset < count {
            let byte = output[offset];
            offset += 1;
            let byte = if byte == b'\\' {
                unescape(&mut self.0, &output[..count], &mut offset)?
            } else {
                byte
            };
            if !byte.is_ascii() || byte < b' ' {
                return Err(invalid());
            }
            output[length] = byte;
            length += 1;
        }
        Ok(length)
    }
}

fn next(reader: &mut impl Read, bytes: &[u8], offset: &mut usize) -> io::Result<u8> {
    if *offset < bytes.len() {
        let byte = bytes[*offset];
        *offset += 1;
        return Ok(byte);
    }
    let mut byte = [0];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn unescape(reader: &mut impl Read, bytes: &[u8], offset: &mut usize) -> io::Result<u8> {
    match next(reader, bytes, offset)? {
        b'/' => Ok(b'/'),
        b'u' => {
            let mut value = 0_u16;
            for _ in 0..4 {
                let byte = next(reader, bytes, offset)?;
                value = value * 16 + char::from(byte).to_digit(16).ok_or_else(invalid)? as u16;
            }
            u8::try_from(value).map_err(|_| invalid())
        }
        _ => Err(invalid()),
    }
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid encoded image escape")
}
