// Objective-C message expression 的 grammar 形态不够统一，selector 解析需要从
// 文本中识别 receiver 与关键字参数，同时避开嵌套 `[]`、`()`, `{}` 和字符串。

pub(super) fn objc_message_receiver_and_selector(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?.trim();
    let receiver_end = objc_receiver_end(inner)?;
    let receiver = inner[..receiver_end].trim().to_owned();
    let selector = objc_selector_from_rest(inner[receiver_end..].trim())?;
    Some((receiver, selector))
}

pub(super) fn objc_protocol_names_from_header(header: &str) -> Vec<String> {
    let Some(start) = header.find('<') else {
        return Vec::new();
    };
    let Some(end_rel) = header[start + 1..].find('>') else {
        return Vec::new();
    };
    header[start + 1..start + 1 + end_rel]
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter(|name| name.chars().next().is_some_and(is_objc_ident_start))
        .map(str::to_owned)
        .collect()
}

pub(super) fn objc_receiver_end(input: &str) -> Option<usize> {
    let mut seen = false;
    let mut state = ObjcScanState::default();
    for (idx, ch) in input.char_indices() {
        if state.consume(ch) {
            continue;
        }
        if ch.is_whitespace() && state.is_top_level() && seen {
            return Some(idx);
        }
        if !ch.is_whitespace() {
            seen = true;
        }
    }
    None
}

pub(super) fn objc_selector_from_rest(rest: &str) -> Option<String> {
    let keywords = objc_selector_keywords(rest);
    if !keywords.is_empty() {
        return Some(
            keywords
                .into_iter()
                .map(|keyword| format!("{keyword}:"))
                .collect::<String>(),
        );
    }
    objc_first_selector_token(rest)
}

pub(super) fn objc_selector_keywords(rest: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    let mut state = ObjcScanState::default();
    for (idx, ch) in rest.char_indices() {
        if state.consume(ch) {
            continue;
        }
        if ch == ':'
            && state.is_top_level()
            && let Some(keyword) = objc_identifier_before(rest, idx)
        {
            keywords.push(keyword);
        }
    }
    keywords
}

pub(super) fn objc_identifier_before(input: &str, colon_idx: usize) -> Option<String> {
    let before = input[..colon_idx].trim_end();
    let end = before.len();
    if end == 0 {
        return None;
    }
    let start = before[..end]
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_objc_ident_char(*ch))
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    let ident = before[start..end].trim();
    (!ident.is_empty() && ident.chars().next().is_some_and(is_objc_ident_start))
        .then(|| ident.to_owned())
}

pub(super) fn objc_first_selector_token(rest: &str) -> Option<String> {
    let mut state = ObjcScanState::default();
    let trimmed = rest.trim_start();
    for (idx, ch) in trimmed.char_indices() {
        if state.consume(ch) {
            continue;
        }
        if (ch.is_whitespace() || ch == ':') && state.is_top_level() {
            let token = trimmed[..idx].trim();
            return (!token.is_empty()).then(|| token.to_owned());
        }
    }
    (!trimmed.is_empty()).then(|| trimmed.trim().to_owned())
}

pub(super) fn is_objc_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

pub(super) fn is_objc_ident_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[derive(Default)]
struct ObjcScanState {
    paren: usize,
    bracket: usize,
    brace: usize,
    quote: Option<char>,
    escaped: bool,
}

impl ObjcScanState {
    fn consume(&mut self, ch: char) -> bool {
        // 返回 true 表示当前字符属于字符串或已由状态机消费，调用方不应把它当成
        // 顶层分隔符。
        if let Some(quote) = self.quote {
            if self.escaped {
                self.escaped = false;
                return true;
            }
            if ch == '\\' {
                self.escaped = true;
                return true;
            }
            if ch == quote {
                self.quote = None;
            }
            return true;
        }
        match ch {
            '"' | '\'' => {
                self.quote = Some(ch);
                true
            }
            '(' => {
                self.paren += 1;
                false
            }
            ')' => {
                self.paren = self.paren.saturating_sub(1);
                false
            }
            '[' => {
                self.bracket += 1;
                false
            }
            ']' => {
                self.bracket = self.bracket.saturating_sub(1);
                false
            }
            '{' => {
                self.brace += 1;
                false
            }
            '}' => {
                self.brace = self.brace.saturating_sub(1);
                false
            }
            _ => false,
        }
    }

    fn is_top_level(&self) -> bool {
        self.paren == 0 && self.bracket == 0 && self.brace == 0 && self.quote.is_none()
    }
}
