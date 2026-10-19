//! Lightweight colouring for text cards.
//!
//! Clipboard text never says what language it is, so instead of guessing a
//! grammar we colour the shapes that appear in nearly every snippet — quoted
//! strings, numbers, comments and a small keyword set. Prose is left alone:
//! [`looks_like_code`] decides whether a card is worth colouring at all.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    Str,
    Number,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "def",
    "default", "delete", "do", "else", "enum", "export", "extends", "false", "fn", "for", "from",
    "function", "if", "impl", "import", "in", "interface", "let", "loop", "match", "mod", "move",
    "mut", "new", "null", "of", "pub", "return", "self", "static", "struct", "super", "switch",
    "this", "throw", "trait", "true", "try", "type", "undefined", "use", "var", "where", "while",
];

/// Markers that essentially never show up in prose. `{` and `;` alone would be
/// false positives in a sentence, so they are paired with something else.
const CODE_MARKERS: &[&str] = &[
    "=>", "::", "();", "};", "</", "#include", "```", "{\"", "\":", "fn ", "pub ", "const ",
    "let ", "def ", "import ", "function ", " {",
];

/// Does this text look like source code rather than a sentence?
pub fn looks_like_code(text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    CODE_MARKERS.iter().any(|marker| text.contains(marker))
}

/// Split `text` into at most `max_lines` lines of at most `max_columns` chars,
/// each line broken into coloured runs.
pub fn snippet(text: &str, max_lines: usize, max_columns: usize) -> Vec<Vec<Token>> {
    text.lines()
        .take(max_lines)
        .map(|line| {
            let clipped: String = line.chars().take(max_columns).collect();
            let mut tokens = tokenize(&clipped);
            hoist_leading_space(&mut tokens);
            tokens
        })
        .collect()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn push(out: &mut Vec<Token>, kind: Kind, chars: &[char]) {
    if chars.is_empty() {
        return;
    }
    let text: String = chars.iter().collect();
    if let Some(last) = out.last_mut() {
        if last.kind == kind {
            last.text.push_str(&text);
            return;
        }
    }
    out.push(Token { kind, text });
}

fn tokenize(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        if c == '/' && chars.get(i + 1) == Some(&'/') {
            push(&mut out, Kind::Comment, &chars[i..]);
            break;
        }

        if c == '"' || c == '\'' || c == '`' {
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == '\\' {
                    j += 2;
                } else if chars[j] == c {
                    j += 1;
                    break;
                } else {
                    j += 1;
                }
            }
            let end = j.min(chars.len());
            push(&mut out, Kind::Str, &chars[i..end]);
            i = end;
            continue;
        }

        if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || matches!(chars[j], '.' | '_'))
            {
                j += 1;
            }
            push(&mut out, Kind::Number, &chars[i..j]);
            i = j;
            continue;
        }

        if is_word(c) {
            let mut j = i;
            while j < chars.len() && is_word(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let kind = if KEYWORDS.contains(&word.as_str()) {
                Kind::Keyword
            } else {
                Kind::Plain
            };
            push(&mut out, kind, &chars[i..j]);
            i = j;
            continue;
        }

        // Whitespace and punctuation are plain runs up to the next token.
        let mut j = i;
        while j < chars.len()
            && !is_word(chars[j])
            && !chars[j].is_ascii_digit()
            && !matches!(chars[j], '"' | '\'' | '`')
            && !(chars[j] == '/' && chars.get(j + 1) == Some(&'/'))
        {
            j += 1;
        }
        if j == i {
            j = i + 1;
        }
        push(&mut out, Kind::Plain, &chars[i..j]);
        i = j;
    }
    out
}

/// A run made only of spaces would render as a zero-width element, taking the
/// indentation with it, so lead it onto the token that follows.
fn hoist_leading_space(tokens: &mut Vec<Token>) {
    let is_blank = tokens
        .first()
        .is_some_and(|t| t.kind == Kind::Plain && t.text.trim().is_empty());
    if is_blank && tokens.len() > 1 {
        let space = tokens.remove(0).text;
        tokens[0].text.insert_str(0, &space);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_strings_numbers_and_keywords() {
        let line = snippet("let n = 42; // answer", 4, 80);
        let tokens = &line[0];
        assert_eq!(tokens[0].kind, Kind::Keyword);
        assert_eq!(tokens[0].text, "let");
        assert!(tokens.iter().any(|t| t.kind == Kind::Number && t.text == "42"));
        assert!(tokens.iter().any(|t| t.kind == Kind::Comment));
    }

    #[test]
    fn keeps_indentation_on_the_following_token() {
        let tokens = snippet("  return x", 4, 80);
        assert_eq!(tokens[0][0].text, "  return");
        assert_eq!(tokens[0][0].kind, Kind::Keyword);
    }

    #[test]
    fn prose_is_not_code() {
        assert!(!looks_like_code("今天下午三点开会，记得带上笔记本"));
        assert!(!looks_like_code("hello world"));
        assert!(looks_like_code("{\"name\": \"clip\"}"));
        assert!(looks_like_code("const x = 1;"));
    }

    #[test]
    fn clips_to_the_requested_budget() {
        let lines = snippet("a\nb\nc\nd", 2, 1);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][0].text, "a");
    }
}
