const LOCAL_FILE_PREFIX: &str = "Local file path (use filesystem tools, not MCP resources): ";
const LOCAL_DIRECTORY_PREFIX: &str =
    "Local directory path (use filesystem tools, not MCP resources): ";
const MARKER_START: &str = "Local ";

/// 在截断前提取正文，避免文件路径和内部提示占满标题。
pub(crate) fn local_file_title_text(mut text: &str) -> String {
    let mut prose = String::new();
    let mut names = Vec::new();
    while let Some(start) = text.find(MARKER_START) {
        prose.push_str(&text[..start]);
        text = &text[start..];
        let Some(prefix) = [LOCAL_FILE_PREFIX, LOCAL_DIRECTORY_PREFIX]
            .into_iter()
            .find(|prefix| text.starts_with(prefix))
        else {
            prose.push_str(MARKER_START);
            text = &text[MARKER_START.len()..];
            continue;
        };
        let code = &text[prefix.len()..];
        if let Some((path, rest)) = read_inline_path(code) {
            let name = path
                .trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next();
            names.extend(name.filter(|name| !name.is_empty()).map(String::from));
            text = rest;
        } else {
            // 旧标题可能缺少闭合反引号，不把残缺路径当作正文。
            text = code.find('\n').map(|end| &code[end..]).unwrap_or_default();
        }
    }
    prose.push_str(text);
    if prose.trim().is_empty() {
        names.join(" ")
    } else {
        prose.trim().to_string()
    }
}

fn read_inline_path(text: &str) -> Option<(&str, &str)> {
    let fence_len = text.bytes().take_while(|byte| *byte == b'`').count();
    if fence_len == 0 {
        return None;
    }
    let fence = &text[..fence_len];
    let after_fence = &text[fence_len..];
    let end = after_fence.find(fence)?;
    let mut path = &after_fence[..end];
    if path.starts_with(' ') && path.ends_with(' ') && !path.trim().is_empty() {
        path = &path[1..path.len() - 1];
    }
    if !is_absolute_path(path) {
        return None;
    }
    Some((path, &after_fence[end + fence_len..]))
}

fn is_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'))
}
