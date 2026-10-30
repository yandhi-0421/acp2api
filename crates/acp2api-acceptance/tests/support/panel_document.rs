use acp2api_acceptance::panel_check;

pub fn html(answer: &acp2api_acceptance::client::Answer) -> String {
    assert_eq!(
        answer.status, 200,
        "the served management document is available"
    );
    panel_check::text(answer)
}

pub fn visible(html: &str) -> String {
    let mut document = html.to_owned();
    for name in ["head", "script", "style", "template"] {
        loop {
            let range = elements(&document, name)
                .next()
                .map(|element| (element.start, element.end));
            let Some((start, end)) = range else { break };
            document.replace_range(start..end, "");
        }
    }
    let mut text = String::new();
    let mut in_tag = false;
    for c in document.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct Element<'a> {
    pub tag: &'a str,
    pub content: &'a str,
    start: usize,
    end: usize,
}

// Read the semantic HTML elements used by the server's form interface. This is not a DOM or CSS engine.
pub fn elements<'a>(html: &'a str, name: &'a str) -> impl Iterator<Item = Element<'a>> {
    let lower = html.to_ascii_lowercase();
    html.match_indices('<').filter_map(move |(start, _)| {
        let rest = &lower[start + 1..];
        if !rest.starts_with(name)
            || !rest[name.len()..].starts_with(|c: char| c.is_ascii_whitespace() || c == '>')
        {
            return None;
        }
        let mut quote = None;
        let length = html[start..].char_indices().find_map(|(index, c)| {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                match c {
                    '\'' | '"' => quote = Some(c),
                    '>' => return Some(index + 1),
                    _ => {}
                }
            }
            None
        })?;
        let content_start = start + length;
        let (content_end, end) = if name == "input" {
            (content_start, content_start)
        } else {
            let closing = lower[content_start..].find(&format!("</{name}"))? + content_start;
            let end = lower[closing..].find('>')? + closing + 1;
            (closing, end)
        };
        Some(Element {
            tag: &html[start + 1 + name.len()..content_start - 1],
            content: &html[content_start..content_end],
            start,
            end,
        })
    })
}

pub fn attribute<'a>(mut tag: &'a str, name: &str) -> Option<&'a str> {
    while !tag.trim().is_empty() {
        tag = tag.trim_start();
        let end = tag
            .find(|c: char| c.is_ascii_whitespace() || c == '=')
            .unwrap_or(tag.len());
        let key = &tag[..end];
        tag = tag[end..].trim_start();
        if !tag.starts_with('=') {
            continue;
        }
        tag = tag[1..].trim_start();
        let value = if tag.starts_with(['\'', '"']) {
            let quote = tag.chars().next().unwrap();
            tag = &tag[1..];
            let end = tag.find(quote).expect("closed HTML attribute");
            let value = &tag[..end];
            tag = &tag[end + 1..];
            value
        } else {
            let end = tag.find(char::is_whitespace).unwrap_or(tag.len());
            let value = &tag[..end];
            tag = &tag[end..];
            value
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(value);
        }
    }
    None
}

pub fn external_addresses(document: &str, address: &str) -> Vec<String> {
    let mut external = Vec::new();
    for (index, _) in document.match_indices("//") {
        let authority: String = document[index + 2..]
            .chars()
            .take_while(|c| {
                !c.is_whitespace()
                    && !matches!(c, '/' | '"' | '\'' | '<' | '>' | ')' | '?' | '#' | '&')
            })
            .collect();
        let https = document[..index].ends_with("https:");
        if !authority.is_empty() && (authority != address || https) {
            external.push(authority);
        }
    }
    external
}
