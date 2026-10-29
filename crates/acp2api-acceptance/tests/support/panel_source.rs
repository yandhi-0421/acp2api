pub fn source_line(html: &str) -> Option<&str> {
    html.split("<p>").find_map(|paragraph| {
        paragraph
            .strip_prefix("Source: ")
            .map(|source| source.split("</p>").next().unwrap())
    })
}
