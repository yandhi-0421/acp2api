use acp2api_acceptance::Scenario;

pub fn lines(s: &Scenario) -> Vec<String> {
    acp2api_acceptance::journal_check::files(&s.root.path.join("data/logs"))
        .into_iter()
        .flat_map(|path| {
            std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}
