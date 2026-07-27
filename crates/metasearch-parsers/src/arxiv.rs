use crate::ParserError;
use quick_xml::de::from_reader;
use serde::Deserialize;
use std::io::Cursor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArxivRecord {
    pub title: String,
    pub canonical_url: String,
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub pdf_url: Option<String>,
    pub doi: Option<String>,
    pub journal_reference: Option<String>,
    pub categories: Vec<String>,
    pub comments: Option<String>,
    pub published_at: String,
}

#[derive(Debug, Deserialize)]
struct Feed {
    #[serde(rename = "entry", default)]
    entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "$value", default)]
    children: Vec<EntryChild>,
}

#[derive(Debug, Deserialize)]
enum EntryChild {
    #[serde(rename = "title")]
    Title(String),
    #[serde(rename = "id")]
    Id(String),
    #[serde(rename = "summary")]
    Summary(String),
    #[serde(rename = "published")]
    Published(String),
    #[serde(rename = "updated")]
    Updated(String),
    #[serde(rename = "author")]
    Author(Author),
    #[serde(rename = "link")]
    Link(Link),
    #[serde(rename = "category")]
    Category(Category),
    #[serde(rename = "doi", alias = "arxiv:doi")]
    Doi(String),
    #[serde(rename = "journal_ref", alias = "arxiv:journal_ref")]
    JournalReference(String),
    #[serde(rename = "comment", alias = "arxiv:comment")]
    Comment(String),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Author {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Link {
    #[serde(rename = "@href")]
    href: String,
    #[serde(default, rename = "@title")]
    title: Option<String>,
    #[serde(default, rename = "@type")]
    content_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Category {
    #[serde(rename = "@term")]
    term: String,
}

fn clean(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_arxiv_atom(xml: &[u8]) -> Result<Vec<ArxivRecord>, ParserError> {
    let feed: Feed = from_reader(Cursor::new(xml))
        .map_err(|error| ParserError::InvalidXml(error.to_string()))?;
    let mut records = Vec::with_capacity(feed.entries.len());

    for entry in feed.entries {
        let mut title = None;
        let mut canonical_url = None;
        let mut abstract_text = None;
        let mut published = None;
        let mut updated = None;
        let mut authors = Vec::new();
        let mut links = Vec::new();
        let mut categories = Vec::new();
        let mut doi = None;
        let mut journal_reference = None;
        let mut comments = None;
        for child in entry.children {
            match child {
                EntryChild::Title(value) => title = Some(clean(&value)),
                EntryChild::Id(value) => canonical_url = Some(value),
                EntryChild::Summary(value) => abstract_text = Some(clean(&value)),
                EntryChild::Published(value) => published = Some(value),
                EntryChild::Updated(value) => updated = Some(value),
                EntryChild::Author(value) => authors.push(clean(&value.name)),
                EntryChild::Link(value) => links.push(value),
                EntryChild::Category(value) => categories.push(value.term),
                EntryChild::Doi(value) => doi = Some(clean(&value)),
                EntryChild::JournalReference(value) => {
                    journal_reference = Some(clean(&value));
                }
                EntryChild::Comment(value) => comments = Some(clean(&value)),
                EntryChild::Other => {}
            }
        }
        let title =
            title.ok_or_else(|| ParserError::InvalidXml("arXiv entry is missing title".into()))?;
        let canonical_url = canonical_url
            .ok_or_else(|| ParserError::InvalidXml("arXiv entry is missing id".into()))?;
        let abstract_text = abstract_text
            .ok_or_else(|| ParserError::InvalidXml("arXiv entry is missing summary".into()))?;
        if title.eq_ignore_ascii_case("error") || canonical_url.contains("/api/errors#") {
            return Err(ParserError::ProviderError(abstract_text));
        }

        let published_at = published.or(updated).ok_or_else(|| {
            ParserError::InvalidXml(
                "arXiv entry is missing published and updated timestamps".into(),
            )
        })?;
        let pdf_url = links
            .iter()
            .find(|link| {
                link.title.as_deref() == Some("pdf")
                    || link.content_type.as_deref() == Some("application/pdf")
            })
            .map(|link| link.href.clone());

        records.push(ArxivRecord {
            title,
            canonical_url,
            abstract_text,
            authors,
            pdf_url,
            doi,
            journal_reference,
            categories,
            comments,
            published_at,
        });
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_atom_metadata() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
          <entry>
            <id>https://arxiv.org/abs/1234.5678v1</id>
            <title> A useful paper </title>
            <summary> Abstract text. </summary>
            <published>2026-01-02T03:04:05Z</published>
            <author><name>Ada Example</name></author>
            <link title="pdf" href="https://arxiv.org/pdf/1234.5678v1" type="application/pdf" />
            <category term="cs.IR" />
            <arxiv:doi>10.1000/example</arxiv:doi>
            <arxiv:journal_ref>Example Journal</arxiv:journal_ref>
            <arxiv:comment>12 pages</arxiv:comment>
          </entry>
        </feed>"#;
        let records = parse_arxiv_atom(xml).unwrap();
        assert_eq!(records[0].authors, vec!["Ada Example"]);
        assert_eq!(records[0].doi.as_deref(), Some("10.1000/example"));
        assert_eq!(records[0].categories, vec!["cs.IR"]);
    }

    #[test]
    fn surfaces_arxiv_error_entries() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <id>http://arxiv.org/api/errors#malformed_query</id>
            <title>Error</title>
            <summary>Malformed query</summary>
            <updated>2026-07-26T00:00:00Z</updated>
            <author><name>arXiv api core</name></author>
          </entry>
        </feed>"#;
        assert_eq!(
            parse_arxiv_atom(xml).unwrap_err(),
            ParserError::ProviderError("Malformed query".into())
        );
    }

    #[test]
    fn accepts_interleaved_repeated_link_elements() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <id>https://arxiv.org/abs/1234.5678v1</id>
            <title>Interleaved links</title>
            <summary>Abstract text.</summary>
            <published>2026-01-02T03:04:05Z</published>
            <link href="https://arxiv.org/abs/1234.5678v1" />
            <author><name>Ada Example</name></author>
            <link title="pdf" href="https://arxiv.org/pdf/1234.5678v1" type="application/pdf" />
          </entry>
        </feed>"#;
        let records = parse_arxiv_atom(xml).unwrap();
        assert_eq!(
            records[0].pdf_url.as_deref(),
            Some("https://arxiv.org/pdf/1234.5678v1")
        );
    }
}
