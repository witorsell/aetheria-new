use base64::Engine;
use serde_json::Value;
use std::io::Read;

/// A card file is already on the device. The link in front of the server drops
/// any response past about 16KB, so posting the file and waiting for the same
/// JSON to come back fails, and the character is never created.
pub fn parse_card_bytes(bytes: &[u8]) -> Result<Value, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return card_from_png(bytes).ok_or_else(|| "No character card metadata or text chunk found in file".to_string());
    }
    let trimmed = trim_ascii_start(bytes);
    if trimmed.first() == Some(&b'{') || trimmed.first() == Some(&b'[') {
        return serde_json::from_slice(trimmed).map_err(|e| format!("Invalid JSON: {e}"));
    }
    find_in_raw(bytes).ok_or_else(|| "No character card metadata or text chunk found in file".to_string())
}

struct TextChunk {
    keyword: String,
    text: String,
}

fn card_from_png(bytes: &[u8]) -> Option<Value> {
    let chunks = png_text_chunks(bytes);
    select_card(&chunks).or_else(|| find_in_raw(bytes))
}

fn select_card(chunks: &[TextChunk]) -> Option<Value> {
    let mut ordered: Vec<&TextChunk> = Vec::new();
    for chunk in chunks {
        if chunk.keyword.eq_ignore_ascii_case("ccv3") {
            ordered.push(chunk);
        }
    }
    for chunk in chunks {
        let keyword = chunk.keyword.to_ascii_lowercase();
        if matches!(keyword.as_str(), "chara" | "character" | "chara_v2" | "tavern" | "pygmalion") {
            ordered.push(chunk);
        }
    }
    for chunk in chunks {
        ordered.push(chunk);
    }
    for chunk in ordered {
        if let Some(value) = decode_chara_payload(&chunk.text) {
            return Some(value);
        }
    }
    None
}

fn png_text_chunks(bytes: &[u8]) -> Vec<TextChunk> {
    let mut chunks = Vec::new();
    let mut pos = 8;
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap_or([0; 4])) as usize;
        let kind = &bytes[pos + 4..pos + 8];
        pos += 8;
        let Some(data_end) = pos.checked_add(len) else { break };
        let Some(next) = data_end.checked_add(4) else { break };
        if next > bytes.len() {
            break;
        }
        let data = &bytes[pos..data_end];
        pos = next;
        let parsed = if kind == b"tEXt" {
            parse_text(data)
        } else if kind == b"zTXt" {
            parse_ztxt(data)
        } else if kind == b"iTXt" {
            parse_itxt(data)
        } else {
            None
        };
        if let Some(chunk) = parsed {
            chunks.push(chunk);
        }
    }
    chunks
}

fn parse_text(data: &[u8]) -> Option<TextChunk> {
    let null = data.iter().position(|&b| b == 0)?;
    Some(TextChunk {
        keyword: String::from_utf8_lossy(&data[..null]).into_owned(),
        text: String::from_utf8_lossy(&data[null + 1..]).into_owned(),
    })
}

fn parse_ztxt(data: &[u8]) -> Option<TextChunk> {
    let null = data.iter().position(|&b| b == 0)?;
    let rest = data.get(null + 1..)?;
    if rest.first().copied() != Some(0) {
        return None;
    }
    Some(TextChunk {
        keyword: String::from_utf8_lossy(&data[..null]).into_owned(),
        text: inflate_zlib(&rest[1..])?,
    })
}

fn parse_itxt(data: &[u8]) -> Option<TextChunk> {
    let null = data.iter().position(|&b| b == 0)?;
    let rest = data.get(null + 1..)?;
    if rest.len() < 2 {
        return None;
    }
    let comp_flag = rest[0];
    let method = rest[1];
    let mut cursor = 2;
    for _ in 0..2 {
        let end = rest[cursor..].iter().position(|&b| b == 0)?;
        cursor += end + 1;
    }
    let text_bytes = rest.get(cursor..)?;
    let text = if comp_flag == 0 {
        String::from_utf8_lossy(text_bytes).into_owned()
    } else if method == 0 {
        inflate_zlib(text_bytes)?
    } else {
        return None;
    };
    Some(TextChunk {
        keyword: String::from_utf8_lossy(&data[..null]).into_owned(),
        text,
    })
}

fn inflate_zlib(data: &[u8]) -> Option<String> {
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut text = String::new();
    decoder.read_to_string(&mut text).ok()?;
    Some(text)
}

fn decode_chara_payload(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    let engines = [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    for engine in engines {
        let Ok(decoded) = engine.decode(&compact) else { continue };
        if let Ok(value) = serde_json::from_slice::<Value>(&decoded) {
            return Some(value);
        }
        if let Ok(text) = std::str::from_utf8(&decoded) {
            if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
                return Some(value);
            }
        }
    }
    None
}

fn find_in_raw(bytes: &[u8]) -> Option<Value> {
    let ascii = String::from_utf8_lossy(bytes);
    for piece in ascii.split(|c: char| !c.is_alphanumeric() && c != '+' && c != '/' && c != '=' && c != '-' && c != '_') {
        if piece.len() >= 20 {
            if let Some(value) = decode_chara_payload(piece) {
                return Some(value);
            }
        }
    }
    let mut start = None;
    let mut depth = 0i32;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'{' {
            if depth == 0 {
                start = Some(index);
            }
            depth += 1;
        } else if byte == b'}' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                if let Some(from) = start {
                    if let Ok(value) = serde_json::from_slice::<Value>(&bytes[from..=index]) {
                        if value.get("name").is_some()
                            || value.get("ch_name").is_some()
                            || value.get("char_name").is_some()
                            || value.get("data").is_some()
                        {
                            return Some(value);
                        }
                    }
                }
            }
        }
    }
    None
}

fn trim_ascii_start(bytes: &[u8]) -> &[u8] {
    let offset = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(bytes.len());
    &bytes[offset..]
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn png_with_text(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));
    bytes.extend(png_chunk(kind, data));
    bytes.extend(png_chunk(b"IEND", &[]));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use std::io::Write;

    fn card_json() -> &'static str {
        r#"{"spec":"chara_card_v2","data":{"name":"Ada"}}"#
    }

    #[test]
    fn json_card_parses_without_a_round_trip() {
        let value = parse_card_bytes(card_json().as_bytes()).unwrap();
        assert_eq!(value["data"]["name"], "Ada");
    }

    #[test]
    fn png_text_chunk_parses_the_embedded_card() {
        let mut data = b"chara\0".to_vec();
        data.extend(STANDARD.encode(card_json()).into_bytes());
        let value = parse_card_bytes(&png_with_text(b"tEXt", &data)).unwrap();
        assert_eq!(value["data"]["name"], "Ada");
    }

    #[test]
    fn png_compressed_chunk_parses_the_embedded_card() {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(card_json().as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut data = b"chara\0\0".to_vec();
        data.extend(compressed);
        let value = parse_card_bytes(&png_with_text(b"zTXt", &data)).unwrap();
        assert_eq!(value["data"]["name"], "Ada");
    }
}
