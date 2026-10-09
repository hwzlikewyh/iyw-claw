// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Words that mark a field as a secret, in the languages iyw-claw ships in.
pub(super) const SECRET_WORDS: &[&str] = &[
    "password",
    "passwd",
    "passcode",
    "passphrase",
    "secret",
    "pin code",
    "密码",
    "密碼",
    "口令",
    "パスワード",
    "비밀번호",
    "contraseña",
    "mot de passe",
    "passwort",
    "senha",
    "كلمة المرور",
];
