//! 后台提炼前的本地预过滤：过短且没有长期信号的用户输入不调用模型。

/// 低于该可见字符数的输入默认跳过模型提炼。
/// 依据历史采集记录：少于 15 字符的提炼轮次没有产出过候选，最短的有效样本为 21 字符。
const MIN_EXTRACTION_CHARS: usize = 15;

/// 短输入中仍然值得提炼的显式长期信号，按小写匹配。
const DURABLE_MARKERS: &[&str] = &[
    "记住",
    "记得",
    "以后",
    "今后",
    "每次",
    "默认",
    "总是",
    "永远",
    "不要再",
    "别再",
    "我是",
    "我叫",
    "叫我",
    "我的",
    "我们是",
    "我喜欢",
    "我习惯",
    "我不",
    "remember",
    "always",
    "never",
    "prefer",
    "call me",
    "my name",
    "from now on",
];

/// 判断用户输入是否值得发起模型提炼；只影响候选提炼，不影响经验信封解析。
pub(super) fn worth_extracting(input: &str) -> bool {
    let visible = input.chars().filter(|ch| !ch.is_whitespace()).count();
    if visible == 0 {
        return false;
    }
    if visible >= MIN_EXTRACTION_CHARS {
        return true;
    }
    let lowered = input.to_lowercase();
    DURABLE_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}
