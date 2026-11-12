//! Parser for wptserve's `{{...}}` template syntax.
//!
//! wptserve finds templates with the regex `{{([^}]*)}}` and splits each one
//! into tokens with an `re.Scanner` (`ReplacementTokenizer` in `pipes.py`).
//! This does both in one pass and accepts the same input, except that an
//! argument list may contain a `}` (but not `}}`), so `{{f(a}b)}}` is one
//! template with the argument `a}b`.

/// A token of a template expression
#[derive(Debug, PartialEq)]
pub(crate) enum Token {
    /// `$name:`: assign the expression's value to the variable `$name`
    Var(String),
    /// `name` or `$name`
    Ident(String),
    /// `[key]`
    Index(Index),
    /// `(a, b)`
    Arguments(Vec<String>),
}

/// The key of an `[index]` token. Like wptserve, keys which Python's `int()`
/// accepts are integers.
#[derive(Debug, PartialEq)]
pub(crate) enum Index {
    Int(i64),
    Str(String),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Piece<'a> {
    /// Text outside any template
    Text(&'a [u8]),
    /// A template, with its source (including the braces) and tokens
    Template {
        source: &'a [u8],
        tokens: Vec<Token>,
    },
}

/// Split `content` into text and templates
pub(crate) fn parse(content: &[u8]) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut text_start = 0;
    let mut pos = 0;
    while let Some(start) = find_open(content, pos) {
        match parse_template(content, start + 2) {
            Some((tokens, end)) => {
                if text_start < start {
                    pieces.push(Piece::Text(&content[text_start..start]));
                }
                pieces.push(Piece::Template {
                    source: &content[start..end],
                    tokens,
                });
                text_start = end;
                pos = end;
            }
            // Like a regex search, retry from the next character
            None => pos = start + 1,
        }
    }
    if text_start < content.len() {
        pieces.push(Piece::Text(&content[text_start..]));
    }
    pieces
}

/// The position of the first `{{` at or after `pos`
fn find_open(content: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let start = pos + content[pos..].iter().position(|&b| b == b'{')?;
        if content.get(start + 1) == Some(&b'{') {
            return Some(start);
        }
        pos = start + 1;
    }
}

/// Parse the template whose expression starts at `pos` (after its `{{`),
/// returning its tokens and the position after its `}}`
fn parse_template(content: &[u8], mut pos: usize) -> Option<(Vec<Token>, usize)> {
    let mut tokens = Vec::new();
    while let Some((token, end)) = next_token(content, pos) {
        tokens.push(token);
        pos = end;
    }
    // Like `re.Scanner`, ignore the rest of the expression once no token
    // matches. Like the regex, it runs to the first `}`, which must start `}}`.
    let close = pos + content[pos..].iter().position(|&b| b == b'}')?;
    content[close..]
        .starts_with(b"}}")
        .then_some((tokens, close + 2))
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn word_len(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|&&b| is_word(b)).count()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The token at `pos` and the position after it
fn next_token(content: &[u8], pos: usize) -> Option<(Token, usize)> {
    let rest = &content[pos..];
    match *rest.first()? {
        b'$' => {
            let len = 1 + word_len(&rest[1..]);
            if len == 1 {
                return None;
            }
            if rest.get(len) == Some(&b':') {
                Some((Token::Var(text(&rest[..len])), pos + len + 1))
            } else {
                Some((Token::Ident(text(&rest[..len])), pos + len))
            }
        }
        b'[' => {
            // A `}` would end the regex match before the `]`
            let len = rest[1..].iter().position(|&b| b == b']' || b == b'}')?;
            if rest[1 + len] != b']' {
                return None;
            }
            let key = text(&rest[1..1 + len]);
            Some((Token::Index(parse_index(key)), pos + len + 2))
        }
        b'(' => {
            let len = rest[1..].iter().position(|&b| b == b')')?;
            let inner = &rest[1..1 + len];
            if inner.windows(2).any(|w| w == b"}}") {
                return None;
            }
            Some((
                Token::Arguments(split_arguments(&text(inner))),
                pos + len + 2,
            ))
        }
        _ => {
            let len = word_len(rest);
            (len > 0).then(|| (Token::Ident(text(&rest[..len])), pos + len))
        }
    }
}

/// `re.split(r",\s*", args)`, or no arguments if `args` is empty
fn split_arguments(args: &str) -> Vec<String> {
    if args.is_empty() {
        return Vec::new();
    }
    let mut parts = args.split(',');
    let first = parts.next().unwrap_or_default().to_string();
    std::iter::once(first)
        .chain(parts.map(|arg| arg.trim_start().to_string()))
        .collect()
}

/// An integer if Python's `int()` accepts `key`, else the string
fn parse_index(key: String) -> Index {
    let trimmed = key.trim();
    let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    let is_int = !digits.is_empty()
        && !digits.starts_with('_')
        && !digits.ends_with('_')
        && !digits.contains("__")
        && digits.bytes().all(|b| b.is_ascii_digit() || b == b'_');
    match is_int.then(|| trimmed.replace('_', "").parse()) {
        Some(Ok(int)) => Index::Int(int),
        _ => Index::Str(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(name: &str) -> Token {
        Token::Ident(name.into())
    }

    fn index(key: &str) -> Token {
        Token::Index(Index::Str(key.into()))
    }

    fn tokens(source: &str) -> Vec<Token> {
        match parse(source.as_bytes()).as_slice() {
            [Piece::Template { tokens, .. }] => tokens.iter().map(clone_token).collect(),
            pieces => panic!("{source:?} isn't one template: {pieces:?}"),
        }
    }

    fn clone_token(token: &Token) -> Token {
        match token {
            Token::Var(name) => Token::Var(name.clone()),
            Token::Ident(name) => Token::Ident(name.clone()),
            Token::Index(Index::Int(int)) => Token::Index(Index::Int(*int)),
            Token::Index(Index::Str(key)) => index(key),
            Token::Arguments(args) => Token::Arguments(args.clone()),
        }
    }

    /// The text and template sources `content` splits into
    fn split(content: &str) -> Vec<String> {
        parse(content.as_bytes())
            .into_iter()
            .map(|piece| match piece {
                Piece::Text(text) => format!("text:{}", String::from_utf8_lossy(text)),
                Piece::Template { source, .. } => {
                    format!("template:{}", String::from_utf8_lossy(source))
                }
            })
            .collect()
    }

    #[test]
    fn tokenizes_like_wptserve() {
        assert_eq!(
            tokens("{{$id:file_hash(sha256, a/b.js)}}"),
            [
                Token::Var("$id".into()),
                ident("file_hash"),
                Token::Arguments(vec!["sha256".into(), "a/b.js".into()]),
            ]
        );
        assert_eq!(
            tokens("{{hosts[][www]}}"),
            [ident("hosts"), index(""), index("www")]
        );
        assert_eq!(
            tokens("{{ports[http][ 1]}}"),
            [ident("ports"), index("http"), Token::Index(Index::Int(1))]
        );
        assert_eq!(
            tokens("{{f( a,b,  c)}}"),
            [
                ident("f"),
                Token::Arguments(vec![" a".into(), "b".into(), "c".into()])
            ]
        );
        assert_eq!(
            tokens("{{uuid()}}"),
            [ident("uuid"), Token::Arguments(vec![])]
        );
        assert_eq!(tokens("{{$id}}"), [ident("$id")]);
        // Tokenizing stops at the first character no token matches
        assert_eq!(tokens("{{host foo}}"), [ident("host")]);
        assert_eq!(tokens("{{host(}}"), [ident("host")]);
        assert_eq!(tokens("{{}}"), []);
    }

    #[test]
    fn finds_templates_like_the_regex() {
        assert_eq!(
            split("a{{host}}b{{GET[x]}}"),
            [
                "text:a",
                "template:{{host}}",
                "text:b",
                "template:{{GET[x]}}"
            ]
        );
        assert_eq!(split("{{{host}}}"), ["template:{{{host}}", "text:}"]);
        assert_eq!(split("{ {{a}b}} }}"), ["text:{ {{a}b}} }}"]);
        assert_eq!(split("{{GET[a}b]}}"), ["text:{{GET[a}b]}}"]);
        assert_eq!(split("{{host"), ["text:{{host"]);
        assert_eq!(split("{{x{{host}}"), ["template:{{x{{host}}"]);
        assert_eq!(
            split("f() {{ return 1; }}"),
            ["text:f() ", "template:{{ return 1; }}"]
        );
    }

    #[test]
    fn arguments_may_contain_a_brace() {
        assert_eq!(
            tokens("{{f(a}b, c)}}"),
            [ident("f"), Token::Arguments(vec!["a}b".into(), "c".into()])]
        );
        assert_eq!(split("{{f(a}})}}"), ["template:{{f(a}}", "text:)}}"]);
    }

    #[test]
    fn parses_indices_like_python_int() {
        for (key, int) in [
            ("0", 0),
            (" 12 ", 12),
            ("-1", -1),
            ("+2", 2),
            ("1_000", 1000),
        ] {
            assert_eq!(parse_index(key.into()), Index::Int(int), "{key:?}");
        }
        for key in ["", "http", "1__0", "_1", "1_", "--1", "1.0"] {
            assert_eq!(parse_index(key.into()), Index::Str(key.into()), "{key:?}");
        }
    }
}
