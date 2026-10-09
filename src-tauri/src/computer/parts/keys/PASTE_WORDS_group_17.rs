// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Words a control's title uses for pasting, in the languages applications
/// commonly come in. Matched anywhere in a title, case aside: "Paste and
/// Match Style" is a paste too. Some of them also mean "insert" in their
/// language, and refusing an Insert menu there is the safe side.
pub(super) const PASTE_WORDS: &[&str] = &[
    "paste",
    "粘贴",
    "貼上",
    "ペースト",
    "貼り付け",
    "붙여넣기",
    "pegar",
    "coller",
    "colar",
    "einsetzen",
    "einfügen",
    "incolla",
    "plakken",
    "вставить",
    "вставка",
    "لصق",
    "yapıştır",
    "wklej",
    "klistra in",
    "indsæt",
    "lim inn",
    "liitä",
    "vložit",
    "beilleszt",
    "lipește",
    "הדבק",
    "tempel",
];
