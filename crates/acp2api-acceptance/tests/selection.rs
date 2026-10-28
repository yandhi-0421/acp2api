use cucumber::{
    Parser,
    feature::Ext as _,
    gherkin::{Feature, GherkinEnv, tagexpr::TagOperation},
    tag::Ext as _,
};
use std::path::Path;
#[derive(Clone, Debug)]
pub struct Selected {
    tags: TagOperation,
}
impl Selected {
    pub fn new(args: &[String], default: &str) -> Self {
        let tag = args
            .windows(2)
            .find(|p| p[0] == "--tags" || p[0] == "-t")
            .map(|p| p[1].as_str())
            .or_else(|| args.iter().find_map(|s| s.strip_prefix("--tags=")));
        let tag = tag
            .map(str::to_owned)
            .or_else(|| std::env::var("CUCUMBER_FILTER_TAGS").ok())
            .unwrap_or_else(|| default.into());
        Self {
            tags: tag.parse().expect("tag expression"),
        }
    }
    fn source(&self, text: &str) -> Option<String> {
        let mut lines: Vec<_> = text.lines().map(str::to_owned).collect();
        let mut docstring = false;
        let mut starts = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let text = line.trim_start();
            if text.starts_with("\"\"\"") || text.starts_with("```") {
                docstring = !docstring;
            } else if !docstring && text.starts_with("@FR-") {
                starts.push(index);
            }
        }
        let mut selected = false;
        for (index, start) in starts.iter().enumerate() {
            let end = starts.get(index + 1).copied().unwrap_or(lines.len());
            let tags: Vec<_> = lines[*start]
                .split_whitespace()
                .filter_map(|s| s.strip_prefix('@'))
                .collect();
            if self.tags.eval(&tags) {
                selected = true;
            } else {
                for line in &mut lines[*start..end] {
                    *line = String::new();
                }
            }
        }
        selected.then(|| lines.join("\n") + "\n")
    }
}
impl<I: AsRef<Path>> Parser<I> for Selected {
    type Cli = cucumber::cli::Empty;
    type Output = futures_util::stream::Iter<std::vec::IntoIter<cucumber::parser::Result<Feature>>>;
    fn parse(self, input: I, _: Self::Cli) -> Self::Output {
        let mut paths: Vec<_> = std::fs::read_dir(input)
            .expect("feature directory")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "feature"))
            .collect();
        paths.sort();
        let features = paths
            .into_iter()
            .filter_map(|path| {
                let text = std::fs::read_to_string(&path).expect("feature source");
                let source = self.source(&text)?;
                Some(
                    Feature::parse(source, GherkinEnv::default())
                        .map_err(|source| {
                            cucumber::gherkin::ParseFileError::Parsing {
                                path: path.clone(),
                                error: None,
                                source,
                            }
                            .into()
                        })
                        .and_then(|mut feature| {
                            feature.path = Some(path);
                            for scenario in feature.scenarios.iter_mut().chain(
                                feature
                                    .rules
                                    .iter_mut()
                                    .flat_map(|rule| &mut rule.scenarios),
                            ) {
                                let names: std::collections::BTreeSet<_> = scenario
                                    .steps
                                    .iter()
                                    .flat_map(|step| step.value.split('<').skip(1))
                                    .filter_map(|part| {
                                        part.split_once('>').map(|(name, _)| name.to_owned())
                                    })
                                    .collect();
                                for table in scenario
                                    .examples
                                    .iter_mut()
                                    .filter_map(|example| example.table.as_mut())
                                {
                                    if let Some((header, rows)) = table.rows.split_first_mut() {
                                        for name in &names {
                                            if !header.contains(name) {
                                                header.push(name.clone());
                                                for row in rows.iter_mut() {
                                                    row.push(format!("<{name}>"));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            feature.expand_examples().map_err(Into::into)
                        }),
                )
            })
            .collect::<Vec<_>>();
        futures_util::stream::iter(features)
    }
}
#[cfg(test)]
mod tests {

    #[test]
    fn preserves_selected_scenario_and_source_lines() {
        let parser = super::Selected::new(&[], "not @wip");
        let text = "Feature: X\n @FR-001 @wip\n Scenario: unfinished\n  Then x\n @FR-002\n Scenario: ready\n  Then y\n";
        let selected = parser.source(text).unwrap();
        assert_eq!(selected.lines().nth(5), Some(" Scenario: ready"));
        assert!(!selected.contains("unfinished"));
        assert!(
            super::Selected::new(&["--tags".into(), "@FR-001".into()], "not @wip")
                .source(text)
                .unwrap()
                .contains("unfinished")
        );
    }
}
