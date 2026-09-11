//! Parser and validator for NOAA-style GRIB2 `.idx` sidecars.

use std::path::Path;

use crate::error::{NcvError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRecord {
    pub line: usize,
    pub ordinal: usize,
    pub offset: u64,
    pub length: u64,
    pub raw: String,
    pub fields: Vec<String>,
}

pub fn parse_index(path: &Path, text: &str, source_len: u64) -> Result<Vec<IndexRecord>> {
    let mut parsed = Vec::new();
    let mut previous_offset = None;
    for (line_index, raw_line) in text.lines().enumerate() {
        let line = line_index + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fields = trimmed
            .split(':')
            .map(str::trim)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if fields.len() < 2 {
            return Err(index_error(path, line, "expected ordinal and byte offset"));
        }
        let ordinal = fields[0]
            .parse::<usize>()
            .map_err(|_| index_error(path, line, "invalid message ordinal"))?;
        if ordinal == 0 {
            return Err(index_error(path, line, "message ordinal must be one-based"));
        }
        if parsed.is_empty() && ordinal != 1 {
            return Err(index_error(
                path,
                line,
                "the first message ordinal must be one",
            ));
        }
        let offset = fields[1]
            .parse::<u64>()
            .map_err(|_| index_error(path, line, "invalid byte offset"))?;
        if offset >= source_len {
            return Err(index_error(
                path,
                line,
                "byte offset is outside the source object",
            ));
        }
        if let Some(previous) = previous_offset
            && offset <= previous
        {
            return Err(index_error(
                path,
                line,
                "byte offsets must be strictly increasing",
            ));
        }
        if let Some((_, previous_ordinal, _, _, _)) = parsed.last()
            && ordinal <= *previous_ordinal
        {
            return Err(index_error(
                path,
                line,
                "message ordinals must be strictly increasing",
            ));
        }
        previous_offset = Some(offset);
        parsed.push((line, ordinal, offset, trimmed.to_owned(), fields));
    }

    parsed
        .into_iter()
        .enumerate()
        .map(|(position, (line, ordinal, offset, raw, fields))| {
            let end = parsed_offset_after(text, position + 1).unwrap_or(source_len);
            let length = end
                .checked_sub(offset)
                .ok_or_else(|| index_error(path, line, "byte range length underflow"))?;
            if length == 0 {
                return Err(index_error(path, line, "byte range is empty"));
            }
            Ok(IndexRecord {
                line,
                ordinal,
                offset,
                length,
                raw,
                fields,
            })
        })
        .collect()
}

fn parsed_offset_after(text: &str, target_position: usize) -> Option<u64> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split(':').nth(1)?.trim().parse::<u64>().ok()
        })
        .nth(target_position)
}

fn index_error(path: &Path, line: usize, reason: &str) -> NcvError {
    NcvError::Grib2 {
        path: path.to_path_buf(),
        reason: format!(".idx line {line}: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_and_derives_terminal_lengths() {
        let records = parse_index(
            Path::new("sample.idx"),
            "# header\n1:0:d=2026091100:TMP:surface\n2:100:d=2026091100:RH:surface\n",
            150,
        )
        .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].length, 100);
        assert_eq!(records[1].length, 50);
    }

    #[test]
    fn rejects_non_monotonic_offsets() {
        let error = parse_index(Path::new("sample.idx"), "1:10:a\n2:10:b\n", 100).unwrap_err();
        assert!(error.to_string().contains("strictly increasing"));
    }
}
