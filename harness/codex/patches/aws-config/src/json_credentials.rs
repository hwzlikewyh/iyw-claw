/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */
use aws_smithy_json::deserialize::token::skip_value;
use aws_smithy_json::deserialize::{json_token_iter, EscapeError, Token};
use aws_smithy_types::date_time::Format;
use aws_smithy_types::DateTime;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::time::SystemTime;

#[derive(Debug)]
pub(crate) enum InvalidJsonCredentials {
    /// The response did not contain valid JSON
    JsonError(Box<dyn Error + Send + Sync>),
    /// The response was missing a required field
    MissingField(&'static str),

    /// A field was invalid
    InvalidField {
        field: &'static str,
        err: Box<dyn Error + Send + Sync>,
    },

    /// Another unhandled error occurred
    Other(Cow<'static, str>),
}

impl From<EscapeError> for InvalidJsonCredentials {
    fn from(err: EscapeError) -> Self {
        InvalidJsonCredentials::JsonError(err.into())
    }
}

impl From<aws_smithy_json::deserialize::error::DeserializeError> for InvalidJsonCredentials {
    fn from(err: aws_smithy_json::deserialize::error::DeserializeError) -> Self {
        InvalidJsonCredentials::JsonError(err.into())
    }
}

impl Display for InvalidJsonCredentials {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidJsonCredentials::JsonError(json) => {
                write!(f, "invalid JSON in response: {json}")
            }
            InvalidJsonCredentials::MissingField(field) => {
                write!(f, "Expected field `{field}` in response but it was missing",)
            }
            InvalidJsonCredentials::Other(msg) => write!(f, "{msg}"),
            InvalidJsonCredentials::InvalidField { field, err } => {
                write!(f, "Invalid field in response: `{field}`. {err}")
            }
        }
    }
}

impl Error for InvalidJsonCredentials {}

#[derive(PartialEq, Eq)]
pub(crate) struct RefreshableCredentials<'a> {
    pub(crate) access_key_id: Cow<'a, str>,
    pub(crate) secret_access_key: Cow<'a, str>,
    pub(crate) session_token: Cow<'a, str>,
    pub(crate) account_id: Option<Cow<'a, str>>,
    pub(crate) expiration: SystemTime,
}

impl fmt::Debug for RefreshableCredentials<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("RefreshableCredentials");
        debug
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"** redacted **")
            .field("session_token", &"** redacted **");
        if let Some(account_id) = &self.account_id {
            debug.field("account_id", account_id);
        }
        debug.field("expiration", &self.expiration).finish()
    }
}

#[non_exhaustive]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum JsonCredentials<'a> {
    RefreshableCredentials(RefreshableCredentials<'a>),
    Error {
        code: Cow<'a, str>,
        message: Cow<'a, str>,
    }, // TODO(https://github.com/awslabs/aws-sdk-rust/issues/340): Add support for static credentials:
       //  {
       //    "AccessKeyId" : "MUA...",
       //    "SecretAccessKey" : "/7PC5om...."
       //  }

       // TODO(https://github.com/awslabs/aws-sdk-rust/issues/340): Add support for Assume role credentials:
       //   {
       //     // fields to construct STS client:
       //     "Region": "sts-region-name",
       //     "AccessKeyId" : "MUA...",
       //     "Expiration" : "2016-02-25T06:03:31Z", // optional
       //     "SecretAccessKey" : "/7PC5om....",
       //     "Token" : "AQoDY....=", // optional
       //     // fields controlling the STS role:
       //     "RoleArn": "...", // required
       //     "RoleSessionName": "...", // required
       //     // and also: DurationSeconds, ExternalId, SerialNumber, TokenCode, Policy
       //     ...
       //   }
}

/// Deserialize an IMDS response from a string
///
/// There are two levels of error here: the top level distinguishes between a successfully parsed
/// response from the credential provider vs. something invalid / unexpected. The inner error
/// distinguishes between a successful response that contains credentials vs. an error with a code and
/// error message.
///
/// Keys are case insensitive.
pub(crate) fn parse_json_credentials(
    credentials_response: &str,
) -> Result<JsonCredentials<'_>, InvalidJsonCredentials> {
    let mut code = None;
    let mut access_key_id = None;
    let mut secret_access_key = None;
    let mut session_token = None;
    let mut account_id = None;
    let mut expiration = None;
    let mut message = None;
    json_parse_loop(credentials_response.as_bytes(), |key, value| {
        match (key, value) {
            /*
             "Code": "Success",
             "Type": "AWS-HMAC",
             "AccessKeyId" : "accessKey",
             "SecretAccessKey" : "secret",
             "Token | SessionToken" : "token",
             "AccountId" : "111122223333",
             "Expiration | ExpiresAt" : "....",
             "LastUpdated" : "2009-11-23T00:00:00Z"
            */
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("Code") => {
                code = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("AccessKeyId") => {
                access_key_id = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. })
                if key.eq_ignore_ascii_case("SecretAccessKey") =>
            {
                secret_access_key = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. })
                if key.eq_ignore_ascii_case("Token")
                    || key.eq_ignore_ascii_case("SessionToken") =>
            {
                session_token = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("AccountId") => {
                account_id = Some(value.to_unescaped()?);
            }
            (key, Token::ValueString { value, .. })
                if key.eq_ignore_ascii_case("Expiration")
                    || key.eq_ignore_ascii_case("ExpiresAt") =>
            {
                expiration = Some(value.to_unescaped()?);
            }

            // Error case handling: message will be set
            (key, Token::ValueString { value, .. }) if key.eq_ignore_ascii_case("Message") => {
                message = Some(value.to_unescaped()?);
            }
            _ => {}
        };
        Ok(())
    })?;
    match code {
        // IMDS does not appear to reply with a `Code` missing, but documentation indicates it
        // may be possible
        None | Some(Cow::Borrowed("Success")) => {
            let access_key_id =
                access_key_id.ok_or(InvalidJsonCredentials::MissingField("AccessKeyId"))?;
            let secret_access_key =
                secret_access_key.ok_or(InvalidJsonCredentials::MissingField("SecretAccessKey"))?;
            let session_token =
                session_token.ok_or(InvalidJsonCredentials::MissingField("Token"))?;
            let expiration =
                expiration.ok_or(InvalidJsonCredentials::MissingField("Expiration"))?;
            let expiration = SystemTime::try_from(
                DateTime::from_str(expiration.as_ref(), Format::DateTime).map_err(|err| {
                    InvalidJsonCredentials::InvalidField {
                        field: "Expiration",
                        err: err.into(),
                    }
                })?,
            )
            .map_err(|_| {
                InvalidJsonCredentials::Other(
                    "credential expiration time cannot be represented by a SystemTime".into(),
                )
            })?;
            Ok(JsonCredentials::RefreshableCredentials(
                RefreshableCredentials {
                    access_key_id,
                    secret_access_key,
                    session_token,
                    account_id,
                    expiration,
                },
            ))
        }
        Some(other) => Ok(JsonCredentials::Error {
            code: other,
            message: message.unwrap_or_else(|| "no message".into()),
        }),
    }
}

pub(crate) fn json_parse_loop<'a>(
    input: &'a [u8],
    mut f: impl FnMut(Cow<'a, str>, &Token<'a>) -> Result<(), InvalidJsonCredentials>,
) -> Result<(), InvalidJsonCredentials> {
    let mut tokens = json_token_iter(input).peekable();
    if !matches!(tokens.next().transpose()?, Some(Token::StartObject { .. })) {
        return Err(InvalidJsonCredentials::JsonError(
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
                return Err(InvalidJsonCredentials::Other(
                    format!("expected object key, found: {other:?}").into(),
                ));
            }
        }
    }
    if tokens.next().is_some() {
        return Err(InvalidJsonCredentials::Other(
            "found more JSON tokens after completing parsing".into(),
        ));
    }
    Ok(())
}
