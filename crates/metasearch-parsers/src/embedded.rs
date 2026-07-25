use crate::ParserError;
use serde_json::Value;

pub fn extract_embedded_json(
    source: &str,
    marker: &str,
    max_source_len: usize,
    max_depth: usize,
) -> Result<Value, ParserError> {
    if source.len() > max_source_len {
        return Err(ParserError::SourceTooLarge);
    }
    let marker_index = source.find(marker).ok_or(ParserError::EmbeddedDataNotFound)?;
    let remainder = source[marker_index + marker.len()..].trim_start();
    let first = remainder.chars().next().ok_or(ParserError::InvalidEmbeddedData)?;
    if !matches!(first, '{' | '[') {
        return Err(ParserError::InvalidEmbeddedData);
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = None;
    for (index, character) in remainder.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        match character {
            '"' => in_string = true,
            '{' | '[' => {
                depth += 1;
                if depth > max_depth {
                    return Err(ParserError::NestingTooDeep);
                }
            }
            '}' | ']' => {
                if depth == 0 {
                    return Err(ParserError::InvalidEmbeddedData);
                }
                depth -= 1;
                if depth == 0 {
                    end = Some(index + character.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }

    let end = end.ok_or(ParserError::InvalidEmbeddedData)?;
    serde_json::from_str(&remainder[..end]).map_err(|_| ParserError::InvalidEmbeddedData)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_without_executing_source() {
        let value = extract_embedded_json(
            r#"window.state = {"nested":[{"value":"};"}]}; alert(1)"#,
            "window.state =",
            1_024,
            8,
        )
        .unwrap();
        assert_eq!(value["nested"][0]["value"], "};");
    }

    #[test]
    fn enforces_depth() {
        assert_eq!(
            extract_embedded_json("x=[[[1]]]", "x=", 100, 2),
            Err(ParserError::NestingTooDeep)
        );
    }
}
