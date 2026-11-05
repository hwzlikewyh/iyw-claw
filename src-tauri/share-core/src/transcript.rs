use chrono::NaiveDateTime;

pub(crate) fn message_count(text: &str) -> Option<usize> {
    let mut lines = text.lines();
    let mut previous = lines.next()?;
    if !previous.starts_with('·') || previous.len() <= '·'.len_utf8() {
        return None;
    }
    let mut count = 0usize;
    for line in lines {
        if previous.starts_with('·')
            && previous.len() > '·'.len_utf8()
            && NaiveDateTime::parse_from_str(line, "%Y年%m月%d日 %H:%M").is_ok()
        {
            count += 1;
        }
        previous = line;
    }
    (count > 0).then_some(count)
}
