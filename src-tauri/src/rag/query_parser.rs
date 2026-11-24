// SPDX-License-Identifier: GPL-3.0-or-later
//! Search-box operators: `penalty ext:pdf folder:Contracts after:2025-01` becomes the free
//! text `penalty` plus a [`SearchFilter`]. Anything that does not parse as an operator stays text.

use std::fmt;
use std::path::Path;

use serde::{Serialize, Serializer};

use super::vector_store::{KindFilter, SearchFilter};
use crate::storage::documents::FolderInfo;

/// A parsed operator, shown read-only as a chip under the search input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "key", content = "value", rename_all = "camelCase")]
pub enum QueryFilter {
    Type(KindFilter),
    /// Lowercase, without the dot.
    Ext(String),
    /// Matches a folder label, or part of the folder's name, ignoring case.
    Folder(String),
    After(DatePrefix),
    Before(DatePrefix),
}

/// `YYYY`, `YYYY-MM`, or `YYYY-MM-DD`; both `after:` and `before:` compare against its first
/// instant, so `after:` is inclusive and `before:` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, type = "string"))]
pub struct DatePrefix {
    year: i32,
    month: Option<u32>,
    day: Option<u32>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    /// Words and phrase contents, for embedding and keyword search.
    pub text: String,
    /// Quoted phrases; a hit must contain each one.
    pub phrases: Vec<String>,
    pub filters: Vec<QueryFilter>,
}

pub fn parse(input: &str) -> ParsedQuery {
    let mut q = ParsedQuery::default();
    let mut text: Vec<&str> = Vec::new();
    let mut rest = input.trim_start();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix(QUOTES) {
            let (phrase, after) = quoted(r);
            if !phrase.trim().is_empty() {
                text.push(phrase);
                q.phrases.push(phrase.to_string());
            }
            rest = after.trim_start();
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (raw, value, after) = match rest[..end].split_once(':') {
            Some((key, v)) if v.starts_with(QUOTES) => {
                let open = v.chars().next().map_or(0, char::len_utf8);
                let (inner, after) = quoted(&rest[key.len() + 1 + open..]);
                (&rest[..rest.len() - after.len()], Some((key, inner)), after)
            }
            Some((key, v)) => (&rest[..end], Some((key, v)), &rest[end..]),
            None => (&rest[..end], None, &rest[end..]),
        };
        match value.and_then(|(k, v)| operator(k, v)) {
            Some(f) if !q.filters.contains(&f) => q.filters.push(f),
            Some(_) => {}
            None => text.push(raw),
        }
        rest = after.trim_start();
    }
    q.text = text.join(" ");
    q
}

/// Straight and curly double quotes; macOS text fields turn a typed `"` into `“` or `”`.
const QUOTES: [char; 3] = ['"', '\u{201C}', '\u{201D}'];

/// Splits at the closing quote; an unclosed quote runs to the end (the user is still typing).
fn quoted(s: &str) -> (&str, &str) {
    s.split_once(QUOTES).unwrap_or((s, ""))
}

fn operator(key: &str, value: &str) -> Option<QueryFilter> {
    let value = value.trim();
    let lower = value.to_lowercase();
    match key.to_lowercase().as_str() {
        "type" => Some(QueryFilter::Type(match lower.as_str() {
            "pdf" => KindFilter::Pdf,
            "doc" => KindFilter::Docx,
            "text" => KindFilter::Text,
            "image" => KindFilter::Image,
            _ => return None,
        })),
        "ext" => {
            let ext = lower.strip_prefix('.').unwrap_or(&lower);
            (!ext.is_empty() && ext.chars().all(char::is_alphanumeric))
                .then(|| QueryFilter::Ext(ext.to_string()))
        }
        "folder" => (!value.is_empty()).then(|| QueryFilter::Folder(value.to_string())),
        "after" => DatePrefix::parse(value).map(QueryFilter::After),
        "before" => DatePrefix::parse(value).map(QueryFilter::Before),
        _ => None,
    }
}

impl ParsedQuery {
    /// The filter for these operators. `None` when a `folder:` names no registered folder, so
    /// nothing can match. Dates resolve at local midnight, `utc_offset_minutes` east of UTC.
    pub fn filter(&self, folders: &[FolderInfo], utc_offset_minutes: i32) -> Option<SearchFilter> {
        let mut f = SearchFilter {
            phrases: self.phrases.clone(),
            ..SearchFilter::default()
        };
        let at = |d: &DatePrefix| d.unix_nanos(utc_offset_minutes);
        let mut wants_folder = false;
        for filter in &self.filters {
            match filter {
                QueryFilter::Type(k) => f.kinds.push(*k),
                QueryFilter::Ext(e) => f.exts.push(e.clone()),
                QueryFilter::Folder(term) => {
                    wants_folder = true;
                    let term = term.to_lowercase();
                    f.folders.extend(
                        folders
                            .iter()
                            .filter(|info| folder_matches(info, &term))
                            .map(|info| info.id),
                    );
                }
                QueryFilter::After(d) => {
                    f.modified_after = f.modified_after.max(Some(at(d)));
                }
                QueryFilter::Before(d) => {
                    f.modified_before = Some(f.modified_before.map_or(at(d), |b| b.min(at(d))));
                }
            }
        }
        (!wants_folder || !f.folders.is_empty()).then_some(f)
    }
}

fn folder_matches(info: &FolderInfo, term: &str) -> bool {
    let name = Path::new(&info.path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    info.label.to_lowercase() == term || name.contains(term)
}

impl DatePrefix {
    fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('-');
        let num = |p: Option<&str>, len: usize| -> Option<Option<u32>> {
            match p {
                None => Some(None),
                Some(p) if p.len() == len && p.bytes().all(|b| b.is_ascii_digit()) => {
                    p.parse().ok().map(Some)
                }
                Some(_) => None,
            }
        };
        let year = num(parts.next(), 4)??;
        let month = num(parts.next(), 2)?;
        let day = num(parts.next(), 2)?;
        if parts.next().is_some() {
            return None;
        }
        let year = i32::try_from(year).ok()?;
        if month.is_some_and(|m| !(1..=12).contains(&m))
            || day.is_some_and(|d| d < 1 || d > days_in_month(year, month.unwrap_or(1)))
        {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// The first instant of this period at local midnight, in ns since the Unix epoch.
    fn unix_nanos(&self, utc_offset_minutes: i32) -> i64 {
        let days = days_from_civil(self.year, self.month.unwrap_or(1), self.day.unwrap_or(1));
        (days * 86_400 - i64::from(utc_offset_minutes) * 60).saturating_mul(1_000_000_000)
    }
}

impl fmt::Display for DatePrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.year)?;
        if let Some(m) = self.month {
            write!(f, "-{m:02}")?;
        }
        if let Some(d) = self.day {
            write!(f, "-{d:02}")?;
        }
        Ok(())
    }
}

/// As typed, such as `2025-01`.
impl Serialize for DatePrefix {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::documents::FolderStatus;

    const NS: i64 = 1_000_000_000;

    fn date(year: i32, month: Option<u32>, day: Option<u32>) -> DatePrefix {
        DatePrefix { year, month, day }
    }

    fn folder(id: i64, path: &str, label: &str) -> FolderInfo {
        FolderInfo {
            id,
            path: path.into(),
            label: label.into(),
            status: FolderStatus::Active,
            enabled: true,
            documents: 0,
            failed: 0,
            skipped: 0,
        }
    }

    #[test]
    fn splits_the_example_into_text_and_filters() {
        let q = parse("penalty ext:pdf folder:Contracts after:2025-01");
        assert_eq!(q.text, "penalty");
        assert_eq!(
            q.filters,
            vec![
                QueryFilter::Ext("pdf".into()),
                QueryFilter::Folder("Contracts".into()),
                QueryFilter::After(date(2025, Some(1), None)),
            ]
        );
    }

    #[test]
    fn plain_text_has_no_filters() {
        let q = parse("  late   payment fee ");
        assert_eq!(q.text, "late payment fee");
        assert!(q.filters.is_empty() && q.phrases.is_empty());
    }

    #[test]
    fn maps_every_type() {
        let q = parse("type:pdf type:doc type:text type:image");
        assert_eq!(
            q.filters,
            [
                KindFilter::Pdf,
                KindFilter::Docx,
                KindFilter::Text,
                KindFilter::Image
            ]
            .map(QueryFilter::Type)
        );
        assert_eq!(q.text, "");
    }

    #[test]
    fn an_unknown_type_stays_text() {
        let q = parse("type:video clip");
        assert_eq!(q.text, "type:video clip");
        assert!(q.filters.is_empty());
    }

    #[test]
    fn keys_and_values_ignore_case() {
        let q = parse("TYPE:PDF Ext:.MD");
        assert_eq!(
            q.filters,
            vec![
                QueryFilter::Type(KindFilter::Pdf),
                QueryFilter::Ext("md".into())
            ]
        );
    }

    #[test]
    fn an_ext_with_symbols_or_no_value_stays_text() {
        let q = parse("ext:p*f ext:");
        assert_eq!(q.text, "ext:p*f ext:");
        assert!(q.filters.is_empty());
    }

    #[test]
    fn unknown_operators_and_urls_stay_text() {
        let q = parse("author:kim https://example.com 10:30");
        assert_eq!(q.text, "author:kim https://example.com 10:30");
        assert!(q.filters.is_empty());
    }

    #[test]
    fn quoted_phrases_are_text_and_must_match() {
        let q = parse("\"late fee\" invoice \"net 30\"");
        assert_eq!(q.text, "late fee invoice net 30");
        assert_eq!(q.phrases, vec!["late fee", "net 30"]);
    }

    #[test]
    fn curly_quotes_work_like_straight_ones() {
        let q = parse("\u{201C}late fee\u{201D} folder:\u{201C}Tax Returns\u{201D}");
        assert_eq!(q.phrases, vec!["late fee"]);
        assert_eq!(q.filters, vec![QueryFilter::Folder("Tax Returns".into())]);
        assert_eq!(q.text, "late fee");
    }

    #[test]
    fn an_operator_inside_quotes_is_part_of_the_phrase() {
        let q = parse("\"ext:pdf report\"");
        assert!(q.filters.is_empty());
        assert_eq!(q.phrases, vec!["ext:pdf report"]);
    }

    #[test]
    fn an_unclosed_quote_runs_to_the_end() {
        let q = parse("fee \"net 30 days");
        assert_eq!(q.text, "fee net 30 days");
        assert_eq!(q.phrases, vec!["net 30 days"]);
    }

    #[test]
    fn empty_quotes_are_dropped() {
        let q = parse("\"\" fee \"  \"");
        assert_eq!(q.text, "fee");
        assert!(q.phrases.is_empty());
    }

    #[test]
    fn a_quoted_folder_may_contain_spaces() {
        let q = parse("folder:\"Tax Returns\" refund");
        assert_eq!(q.filters, vec![QueryFilter::Folder("Tax Returns".into())]);
        assert_eq!(q.text, "refund");
    }

    #[test]
    fn parses_all_three_date_forms() {
        let q = parse("after:2024 before:2025-03 after:2024-02-29");
        assert_eq!(
            q.filters,
            vec![
                QueryFilter::After(date(2024, None, None)),
                QueryFilter::Before(date(2025, Some(3), None)),
                QueryFilter::After(date(2024, Some(2), Some(29))),
            ]
        );
    }

    #[test]
    fn invalid_dates_stay_text() {
        for bad in [
            "after:2025-13",
            "after:2025-02-29",
            "before:25-01",
            "before:2025-1",
            "after:2025-01-01-01",
            "after:2025/01",
            "after:yesterday",
        ] {
            let q = parse(bad);
            assert_eq!(q.text, bad, "{bad}");
            assert!(q.filters.is_empty(), "{bad}");
        }
    }

    #[test]
    fn duplicate_operators_give_one_chip() {
        let q = parse("ext:pdf ext:PDF");
        assert_eq!(q.filters, vec![QueryFilter::Ext("pdf".into())]);
    }

    #[test]
    fn non_ascii_text_and_full_width_spaces_split() {
        let q = parse("위약금\u{3000}ext:hwp 계약");
        assert_eq!(q.text, "위약금 계약");
        assert_eq!(q.filters, vec![QueryFilter::Ext("hwp".into())]);
    }

    #[test]
    fn chips_serialize_as_key_and_value() {
        let v = serde_json::to_value(parse("type:doc before:2025-01-05").filters).unwrap();
        assert_eq!(
            v,
            serde_json::json!([
                {"key": "type", "value": "doc"},
                {"key": "before", "value": "2025-01-05"},
            ])
        );
    }

    #[test]
    fn dates_resolve_to_local_midnight() {
        let q = parse("after:2025-01 before:2025-01-02");
        let utc = q.filter(&[], 0).unwrap();
        assert_eq!(utc.modified_after, Some(1_735_689_600 * NS));
        assert_eq!(utc.modified_before, Some((1_735_689_600 + 86_400) * NS));
        // Seoul is UTC+9, so its midnight is 9 hours earlier.
        let seoul = q.filter(&[], 540).unwrap();
        assert_eq!(seoul.modified_after, Some((1_735_689_600 - 9 * 3600) * NS));
    }

    #[test]
    fn repeated_dates_keep_the_narrowest_range_and_lists_union() {
        let f = parse("after:2020 after:2024 before:2026 before:2025 ext:md type:text type:pdf")
            .filter(&[], 0)
            .unwrap();
        assert_eq!(
            f.modified_after,
            Some(days_from_civil(2024, 1, 1) * 86_400 * NS)
        );
        assert_eq!(
            f.modified_before,
            Some(days_from_civil(2025, 1, 1) * 86_400 * NS)
        );
        assert_eq!(f.exts, vec!["md"]);
        assert_eq!(f.kinds, vec![KindFilter::Text, KindFilter::Pdf]);
    }

    #[test]
    fn folder_matches_a_label_or_part_of_a_name() {
        let folders = [
            folder(1, "/Users/u/Work Contracts", "Work Contracts"),
            folder(2, "/Users/u/Tax", "Receipts"),
            folder(3, "/Users/u/contracts-old", "/Users/u/contracts-old"),
        ];
        let ids = |q: &str| parse(q).filter(&folders, 0).map(|f| f.folders);
        assert_eq!(ids("folder:contract"), Some(vec![1, 3]));
        assert_eq!(ids("folder:receipts"), Some(vec![2]));
        // A label matches whole only; its path's parent never does.
        assert_eq!(ids("folder:Users"), None);
        assert_eq!(ids("folder:Tax folder:old"), Some(vec![2, 3]));
        assert_eq!(ids("fee").map(|f| f.is_empty()), Some(true));
    }
}
