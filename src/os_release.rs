use std::io::BufRead;

pub fn parse<R: BufRead>(file: R) -> Option<String> {
    const FIELD: &str = "PRETTY_NAME=";
    file.lines()
        .flat_map(|line| line)
        .find(|line| line.starts_with(FIELD))
        .map(|line| line[FIELD.len() + 1..line.len() - 1].into())
}
