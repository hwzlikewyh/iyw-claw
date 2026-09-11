/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_runtime::fs_util::{home_dir, Os};
use aws_smithy_json::deserialize::token::skip_value;
use aws_smithy_json::deserialize::Token;
use aws_smithy_json::deserialize::{json_token_iter, EscapeError};
use aws_smithy_json::serialize::JsonObjectWriter;
use aws_smithy_types::date_time::{DateTimeFormatError, Format};
use aws_smithy_types::DateTime;
use aws_types::os_shim_internal::{Env, Fs};
use sha1::{Digest, Sha1};
use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;
use zeroize::Zeroizing;

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Clone)]
pub(super) struct CachedSsoToken {
    pub(super) access_token: Zeroizing<String>,
    pub(super) client_id: Option<String>,
    pub(super) client_secret: Option<Zeroizing<String>>,
    pub(super) expires_at: SystemTime,
    pub(super) refresh_token: Option<Zeroizing<String>>,
    pub(super) region: Option<String>,
    pub(super) registration_expires_at: Option<SystemTime>,
    pub(super) start_url: Option<String>,
}

impl CachedSsoToken {
    /// True if the information required to refresh this token is present.
    ///
    /// The expiration times are not considered by this function.
    pub(super) fn refreshable(&self) -> bool {
        self.client_id.is_some()
            && self.client_secret.is_some()
            && self.refresh_token.is_some()
            && self.registration_expires_at.is_some()
    }
}

impl fmt::Debug for CachedSsoToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachedSsoToken")
            .field("access_token", &"** redacted **")
            .field("client_id", &self.client_id)
            .field("client_secret", &"** redacted **")
            .field("expires_at", &self.expires_at)
            .field("refresh_token", &"** redacted **")
            .field("region", &self.region)
            .field("registration_expires_at", &self.registration_expires_at)
            .field("start_url", &self.start_url)
            .finish()
    }
}

#[derive(Debug)]
pub(super) enum CachedSsoTokenError {
    FailedToFormatDateTime {
        source: Box<dyn StdError + Send + Sync>,
    },
    InvalidField {
        field: &'static str,
        source: Box<dyn StdError + Send + Sync>,
    },
    IoError {
        what: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    JsonError(Box<dyn StdError + Send + Sync>),
    MissingField(&'static str),
    NoHomeDirectory,
    Other(Cow<'static, str>),
}

impl fmt::Display for CachedSsoTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedToFormatDateTime { .. } => write!(f, "failed to format date time"),
            Self::InvalidField { field, .. } => write!(
                f,
                "invalid value for the `{field}` field in the cached SSO token file"
            ),
            Self::IoError { what, path, .. } => write!(f, "failed to {what} `{}`", path.display()),
            Self::JsonError(_) => write!(f, "invalid JSON in cached SSO token file"),
            Self::MissingField(field) => {
                write!(f, "missing field `{field}` in cached SSO token file")
            }
            Self::NoHomeDirectory => write!(f, "couldn't resolve a home directory"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl StdError for CachedSsoTokenError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::FailedToFormatDateTime { source } => Some(source.as_ref()),
            Self::InvalidField { source, .. } => Some(source.as_ref()),
            Self::IoError { source, .. } => Some(source),
            Self::JsonError(source) => Some(source.as_ref()),
            Self::MissingField(_) => None,
            Self::NoHomeDirectory => None,
            Self::Other(_) => None,
        }
    }
}

impl From<EscapeError> for CachedSsoTokenError {
    fn from(err: EscapeError) -> Self {
        Self::JsonError(err.into())
    }
}

impl From<aws_smithy_json::deserialize::error::DeserializeError> for CachedSsoTokenError {
    fn from(err: aws_smithy_json::deserialize::error::DeserializeError) -> Self {
        Self::JsonError(err.into())
    }
}

impl From<DateTimeFormatError> for CachedSsoTokenError {
    fn from(value: DateTimeFormatError) -> Self {
        Self::FailedToFormatDateTime {
            source: value.into(),
        }
    }
}

/// Determine the SSO cached token path for a given identifier.
///
/// The `identifier` is the `sso_start_url` for credentials providers, and `sso_session_name` for token providers.
fn cached_token_path(identifier: &str, home: &str) -> PathBuf {
    // hex::encode returns a lowercase string
    let mut out = PathBuf::with_capacity(home.len() + "/.aws/sso/cache".len() + ".json".len() + 40);
    out.push(home);
    out.push(".aws/sso/cache");
    out.push(hex::encode(Sha1::digest(identifier.as_bytes())));
    out.set_extension("json");
    out
}

/// Load the token for `identifier` from `~/.aws/sso/cache/<hashofidentifier>.json`
///
/// The `identifier` is the `sso_start_url` for credentials providers, and `sso_session_name` for token providers.
pub(super) async fn load_cached_token(
    env: &Env,
    fs: &Fs,
    identifier: &str,
) -> Result<CachedSsoToken, CachedSsoTokenError> {
    let home = home_dir(env, Os::real()).ok_or(CachedSsoTokenError::NoHomeDirectory)?;
    let path = cached_token_path(identifier, &home);
    let data = Zeroizing::new(fs.read_to_end(&path).await.map_err(|source| {
        CachedSsoTokenError::IoError {
            what: "read",
            path,
            source,
        }
    })?);
    parse_cached_token(&data)
}

/// Parse SSO token JSON from input
fn parse_cached_token(
    cached_token_file_contents: &[u8],
) -> Result<CachedSsoToken, CachedSsoTokenError> {
    use CachedSsoTokenError as Error;

    let mut access_token = None;
    let mut expires_at = None;
    let mut client_id = None;
    let mut client_secret = None;
    let mut refresh_token = None;
    let mut region = None;
    let mut registration_expires_at = None;
    let mut start_url = None;
    json_parse_loop(cached_token_file_contents, |key, value| {
        match (key, value) {
            /*
            // Required fields:
            "accessToken": "string",
            "expiresAt": "2019-11-14T04:05:45Z",

            // Optional fields:
            "refreshToken": "string",
            "clientId": "ABCDEFG323242423121312312312312312",
            "clientSecret": "ABCDE123",
            "registrationExpiresAt": "2022-03-06T19:53:17Z",
            "region": "us-west-2",
            "startUrl": "https://d-abc123.awsapps.com/start"
            */
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("accessToken") => {
                access_token = Some(Zeroizing::new(value.to_unescaped()?.into_owned()));
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("expiresAt") => {
                expires_at = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("clientId") => {
                client_id = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("clientSecret") => {
                client_secret = Some(Zeroizing::new(value.to_unescaped()?.into_owned()));
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("refreshToken") => {
                refresh_token = Some(Zeroizing::new(value.to_unescaped()?.into_owned()));
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("region") => {
                region = Some(value.to_unescaped()?.into_owned());
            }
            (key, Token::ValueString { value, .. })
                if key.eq_ignore_ascii_case("registrationExpiresAt") =>
            {
                registration_expires_at = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("startUrl") => {
                start_url = Some(value.to_unescaped()?.into_owned());
            }
            _ => {}
        };
        Ok(())
    })?;

    Ok(CachedSsoToken {
        access_token: access_token.ok_or(Error::MissingField("accessToken"))?,
        expires_at: expires_at
            .ok_or(Error::MissingField("expiresAt"))
            .and_then(|expires_at| {
                DateTime::from_str(expires_at.as_ref(), Format::DateTime)
                    .map_err(|err| Error::InvalidField { field: "expiresAt", source: err.into() })
                    .and_then(|date_time| {
                        SystemTime::try_from(date_time).map_err(|_| {
                            Error::Other(
                                "SSO token expiration time cannot be represented by a SystemTime"
                                    .into(),
                            )
                        })
                    })
            })?,
        client_id: client_id.map(Cow::into_owned),
        client_secret,
        refresh_token,
        region,
        registration_expires_at: Ok(registration_expires_at).and_then(|maybe_expires_at| {
            if let Some(expires_at) = maybe_expires_at {
                Some(
                    DateTime::from_str(expires_at.as_ref(), Format::DateTime)
                        .map_err(|err| Error::InvalidField { field: "registrationExpiresAt", source: err.into()})
                        .and_then(|date_time| {
                            SystemTime::try_from(date_time).map_err(|_| {
                                Error::Other(
                                    "SSO registration expiration time cannot be represented by a SystemTime"
                                        .into(),
                                )
                            })
                        }),
                )
                .transpose()
            } else {
                Ok(None)
            }
        })?,
        start_url,
    })
}

fn json_parse_loop<'a>(
    input: &'a [u8],
    mut f: impl FnMut(Cow<'a, str>, &Token<'a>) -> Result<(), CachedSsoTokenError>,
) -> Result<(), CachedSsoTokenError> {
    use CachedSsoTokenError as Error;
    let mut tokens = json_token_iter(input).peekable();
    if !matches!(tokens.next().transpose()?, Some(Token::StartObject { .. })) {
        return Err(Error::Other(
            "expected a JSON document starting with `{`".into(),
        ));
    }
    loop {
        match tokens.next().transpose()? {
            Some(Token::EndObject { .. }) => break,
            Some(Token::ObjectKey { key, .. }) => {
                if let Some(Ok(token)) = tokens.peek() {
                    let key = key.to_unescaped()?;
                    f(key, token)?
                }
                skip_value(&mut tokens)?;
            }
            other => {
                return Err(Error::Other(
                    format!("expected object key, found: {other:?}").into(),
                ));
            }
        }
    }
    if tokens.next().is_some() {
        return Err(Error::Other(
            "found more JSON tokens after completing parsing".into(),
        ));
    }
    Ok(())
}

pub(super) async fn save_cached_token(
    env: &Env,
    fs: &Fs,
    identifier: &str,
    token: &CachedSsoToken,
) -> Result<(), CachedSsoTokenError> {
    let expires_at = DateTime::from(token.expires_at).fmt(Format::DateTime)?;
    let registration_expires_at = token
        .registration_expires_at
        .map(|time| DateTime::from(time).fmt(Format::DateTime))
        .transpose()?;

    let mut out = Zeroizing::new(String::new());
    let mut writer = JsonObjectWriter::new(&mut out);
    writer.key("accessToken").string(&token.access_token);
    writer.key("expiresAt").string(&expires_at);
    if let Some(refresh_token) = &token.refresh_token {
        writer.key("refreshToken").string(refresh_token);
    }
    if let Some(client_id) = &token.client_id {
        writer.key("clientId").string(client_id);
    }
    if let Some(client_secret) = &token.client_secret {
        writer.key("clientSecret").string(client_secret);
    }
    if let Some(registration_expires_at) = registration_expires_at {
        writer
            .key("registrationExpiresAt")
            .string(&registration_expires_at);
    }
    if let Some(region) = &token.region {
        writer.key("region").string(region);
    }
    if let Some(start_url) = &token.start_url {
        writer.key("startUrl").string(start_url);
    }
    writer.finish();

    let home = home_dir(env, Os::real()).ok_or(CachedSsoTokenError::NoHomeDirectory)?;
    let path = cached_token_path(identifier, &home);
    fs.write(&path, out.as_bytes())
        .await
        .map_err(|err| CachedSsoTokenError::IoError {
            what: "write",
            path,
            source: err,
        })?;
    Ok(())
}
