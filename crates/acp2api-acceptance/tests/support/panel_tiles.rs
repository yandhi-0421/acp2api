use crate::World;
use acp2api_acceptance::{panel_check, system_check};

#[derive(Default)]
pub struct TileState {
    pub pages: Vec<String>,
    pub calls_before: usize,
}
impl std::fmt::Debug for TileState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileState").finish_non_exhaustive()
    }
}

pub fn html(w: &World) -> String {
    let answer = w.panel.answer.as_ref().unwrap();
    assert_eq!(answer.status, 200, "FR-054 management page is available");
    panel_check::text(answer)
}

pub fn sections(html: &str) -> impl Iterator<Item = &str> {
    html.split("<section")
        .skip(1)
        .map(|part| part.split("</section>").next().unwrap())
}

pub fn visible(html: &str) -> String {
    let mut text = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                tag = true;
                text.push(' ');
            }
            '>' => {
                tag = false;
                text.push(' ');
            }
            _ if !tag => text.push(c),
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

pub fn tile<'a>(html: &'a str, title: &str) -> &'a str {
    let section = sections(html).find(|section| {
        section.split("<h2").skip(1).any(|heading| {
            heading.split_once('>').is_some_and(|(_, rest)| {
                visible(rest.split("</h2>").next().unwrap()).eq_ignore_ascii_case(title)
            })
        })
    });
    assert!(section.is_some(), "FR-054 missing tile: {title}");
    section.unwrap()
}

pub fn figure(section: &str, label: &str, value: &str) {
    let text = visible(section).to_ascii_lowercase();
    let label = label.to_ascii_lowercase();
    assert!(
        text.contains(&format!("{label} {value}")) || text.contains(&format!("{value} {label}")),
        "FR-054 missing or wrong figure: {label} = {value}"
    );
}

pub fn runtime_calls(w: &World) -> usize {
    system_check::requests(&w.scenario).len()
}
