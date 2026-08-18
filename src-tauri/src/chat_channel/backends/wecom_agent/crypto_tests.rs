use super::*;

const CORP_ID: &str = "wx5823bf96d3bd56c7";
const TOKEN: &str = "QDG6eK";
const AES_KEY: &str = "jWmYm7qr5nMoAUwZRjGtBxmz3KA1tkAj3ykkR6q2B2C";
const ENCRYPTED: &str = "RypEvHKD8QQKFhvQ6QleEB4J58tiPdvo+rtK1I9qca6aM/wvqnLSV5zEPeusUiX5L5X/0lWfrf0QADHHhGd3QczcdCUpj911L3vg3W/sYYvuJTs3TUUkSUXxaccAS0qhxchrRYt66wiSpGLYL42aM6A8dTT+6k4aSknmPj48kzJs8qLjvd4Xgpue06DOdnLxAUHzM6+kDZ+HMZfJYuR+LtwGc2hgf5gsijff0ekUNXZiqATP7PF5mZxZ3Izoun1s4zG4LUMnvw2r+KqCKIw+3IQH03v+BCA9nMELNqbSf6tiWSrXJB3LAVGUcallcrw8V2t9EL4EhzJWrQUax5wLVMNS0+rUPA3k22Ncx4XXZS9o0MBH27Bo6BpNelZpS+/uh9KsNlY6bHCmJU9p8g7m3fVKn28H3KDYA5Pl/T8Z1ptDAVe0lXdQ2YoyyH2uyPIGHBZZIs2pDBS8R07+qN+E7Q==";

#[test]
fn decrypts_official_90968_vector() {
    verify_signature(
        "477715d11cdb4164915debcba66cb864d751f3e6",
        TOKEN,
        "1409659813",
        "1372623149",
        ENCRYPTED,
    )
    .unwrap();
    let plaintext = decrypt(AES_KEY, ENCRYPTED, CORP_ID).unwrap();
    let message = parse_message(&plaintext).unwrap();
    assert_eq!(message.from_user_name, "mycreate");
    assert_eq!(message.content, "hello");
    assert_eq!(message.agent_id, "218");
}

#[test]
fn rejects_signature_receive_id_and_malformed_lengths() {
    assert!(verify_signature("bad", TOKEN, "1", "2", ENCRYPTED).is_err());
    assert!(decrypt(AES_KEY, ENCRYPTED, "other-corp").is_err());
    assert!(decrypt(AES_KEY, "AA==", CORP_ID).is_err());
}

#[test]
fn encrypt_roundtrip_preserves_xml() {
    let xml = "<xml><Content><![CDATA[hello]]></Content></xml>";
    let encrypted = encrypt(AES_KEY, xml, CORP_ID).unwrap();
    assert_eq!(decrypt(AES_KEY, &encrypted, CORP_ID).unwrap(), xml);
}
