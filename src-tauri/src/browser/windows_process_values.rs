#![cfg(target_os = "windows")]

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

pub(super) fn build_environment(overrides: &[(OsString, OsString)]) -> std::io::Result<Vec<u16>> {
    let mut values = BTreeMap::new();
    for (key, value) in std::env::vars_os().chain(overrides.iter().cloned()) {
        validate_environment(&key, &value)?;
        values.insert(key.to_string_lossy().to_uppercase(), (key, value));
    }
    let mut block = Vec::new();
    for (_, (key, value)) in values {
        block.extend(key.encode_wide());
        block.push('=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn validate_environment(key: &OsStr, value: &OsStr) -> std::io::Result<()> {
    let mut has_key = false;
    let invalid_key = key.encode_wide().enumerate().any(|(index, unit)| {
        has_key = true;
        unit == 0 || (unit == '=' as u16 && index > 0)
    });
    let invalid_value = value.encode_wide().any(|unit| unit == 0);
    if !has_key || invalid_key || invalid_value {
        return Err(io_error("browser environment contains an invalid value"));
    }
    Ok(())
}

pub(super) fn build_command_line(executable: &Path, args: &[OsString]) -> std::io::Result<String> {
    let mut command = String::new();
    quote_argument(&mut command, executable.as_os_str())?;
    for argument in args {
        command.push(' ');
        quote_argument(&mut command, argument)?;
    }
    Ok(command)
}

fn quote_argument(output: &mut String, argument: &OsStr) -> std::io::Result<()> {
    let value = argument
        .to_str()
        .ok_or_else(|| io_error("browser command contains invalid Unicode"))?;
    let needs_quotes = value.is_empty() || value.chars().any(char_requires_quotes);
    if !needs_quotes {
        output.push_str(value);
        return Ok(());
    }
    output.push('"');
    let mut slashes = 0;
    for character in value.chars() {
        if character == '\\' {
            slashes += 1;
        } else {
            write_quoted_character(output, character, &mut slashes);
        }
    }
    output.extend(std::iter::repeat('\\').take(slashes * 2));
    output.push('"');
    Ok(())
}

fn char_requires_quotes(character: char) -> bool {
    character.is_whitespace() || character == '"'
}

fn write_quoted_character(output: &mut String, character: char, slashes: &mut usize) {
    let count = if character == '"' {
        *slashes * 2 + 1
    } else {
        *slashes
    };
    output.extend(std::iter::repeat('\\').take(count));
    *slashes = 0;
    output.push(character);
}

pub(super) fn to_wide(value: &OsStr) -> std::io::Result<Vec<u16>> {
    let mut wide: Vec<u16> = value.encode_wide().collect();
    if wide.contains(&0) {
        return Err(io_error("browser launch value contains a null character"));
    }
    wide.push(0);
    Ok(wide)
}

fn io_error(message: impl Into<String>) -> std::io::Error {
    std::io::Error::other(message.into())
}
