use anyhow::{Result, bail};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Field(String),
    Index(usize),
    Wildcard,
}

pub fn validate(path: &str) -> Result<()> {
    parse(path).map(|_| ())
}

pub fn select<'a>(root: &'a Value, path: &str) -> Result<Vec<&'a Value>> {
    let segments = parse(path)?;
    let mut current = vec![root];
    for segment in segments {
        let mut next = Vec::new();
        for value in current {
            match &segment {
                Segment::Field(field) => {
                    if let Some(child) = value.get(field) {
                        next.push(child);
                    }
                }
                Segment::Index(index) => {
                    if let Some(child) = value.get(*index) {
                        next.push(child);
                    }
                }
                Segment::Wildcard => match value {
                    Value::Array(values) => next.extend(values),
                    Value::Object(values) => next.extend(values.values()),
                    _ => {}
                },
            }
        }
        current = next;
    }
    Ok(current)
}

fn parse(path: &str) -> Result<Vec<Segment>> {
    let path = path.trim();
    if !path.starts_with('$') {
        bail!("JSONPath must start with '$'");
    }
    let chars: Vec<char> = path.chars().collect();
    let mut index = 1usize;
    let mut segments = Vec::new();
    while index < chars.len() {
        match chars[index] {
            '.' => {
                index += 1;
                let start = index;
                if !chars
                    .get(index)
                    .is_some_and(|ch| *ch == '_' || ch.is_ascii_alphabetic() || *ch >= '\u{80}')
                {
                    bail!(
                        "invalid field name in JSONPath: {path}; use bracket notation for special names"
                    );
                }
                while index < chars.len()
                    && (chars[index] == '_'
                        || chars[index].is_ascii_alphanumeric()
                        || chars[index] >= '\u{80}')
                {
                    index += 1;
                }
                segments.push(Segment::Field(chars[start..index].iter().collect()));
            }
            '[' => {
                index += 1;
                if index >= chars.len() {
                    bail!("unterminated bracket in JSONPath: {path}");
                }
                if chars[index] == '*' {
                    index += 1;
                    if chars.get(index) != Some(&']') {
                        bail!("invalid wildcard in JSONPath: {path}");
                    }
                    index += 1;
                    segments.push(Segment::Wildcard);
                } else if matches!(chars[index], '\'' | '"') {
                    let quote = chars[index];
                    index += 1;
                    let start = index;
                    while index < chars.len() && chars[index] != quote {
                        if chars[index] == '\\' || chars[index].is_control() {
                            bail!(
                                "escape sequences and control characters in JSONPath fields are unsupported: {path}"
                            );
                        }
                        index += 1;
                    }
                    if index >= chars.len() {
                        bail!("unterminated quoted field in JSONPath: {path}");
                    }
                    let field: String = chars[start..index].iter().collect();
                    index += 1;
                    if chars.get(index) != Some(&']') {
                        bail!("missing closing bracket in JSONPath: {path}");
                    }
                    index += 1;
                    segments.push(Segment::Field(field));
                } else {
                    let start = index;
                    while index < chars.len() && chars[index].is_ascii_digit() {
                        index += 1;
                    }
                    if start == index || chars.get(index) != Some(&']') {
                        bail!("invalid array index in JSONPath: {path}");
                    }
                    let raw: String = chars[start..index].iter().collect();
                    index += 1;
                    segments.push(Segment::Index(raw.parse()?));
                }
            }
            other => bail!("unsupported JSONPath token '{other}' in {path}"),
        }
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wildcard_selection() {
        let value = json!({"items": [{"name":"a"}, {"name":"b"}]});
        let selected = select(&value, "$.items[*].name").unwrap_or_default();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0], "a");
    }

    #[test]
    fn validates_supported_paths_and_rejects_malformed_syntax() {
        for path in [
            "$",
            "$.items[*].name",
            "$[0]['display-name']",
            "$[\"\"]",
            "$.名前",
        ] {
            assert!(validate(path).is_ok(), "{path}");
        }
        for path in [
            "",
            "items",
            "$.",
            "$..name",
            "$.items[",
            "$.items]",
            "$.items[*",
            "$.items[-1]",
            "$.items[?(@.id)]",
            "$.1name",
            "$.name value",
            "$['name\\n']",
            "$['name\n']",
        ] {
            assert!(validate(path).is_err(), "{path}");
            assert!(select(&Value::Null, path).is_err(), "{path}");
        }
    }
}
