use std::io::{self, BufRead, BufReader};
use std::fs::File;

lazy_static! {
    pub static ref OS_RELEASE: OsRelease = OsRelease::new()
        .expect("unable to find /etc/os-release");
}

macro_rules! starts_with_match {
    ($item:expr, { $($pat:expr => $field:expr),+ }) => {{
        $(
            if $item.starts_with($pat) {
                $field = parse_line($item, $pat.len()).into();
                continue;
            }
        )+
    }};
}

fn parse_line(line: &str, skip: usize) -> &str {
    let line = line[skip..].trim();
    if line.starts_with('"') && line.ends_with('"') {
        &line[1.. line.len() - 1]
    } else {
        line
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct OsRelease {
    pub name: String,
    pub version: String,
    pub id: String,
    pub id_like: String,
    pub pretty_name: String,
    pub version_id: String,
    pub home_url: String,
    pub support_url: String,
    pub bug_report_url: String,
    pub privacy_policy_url: String,
    pub version_codename: String,
}

impl OsRelease {
    pub fn from_iter<I: Iterator<Item = String>>(lines: I) -> OsRelease {
        let mut os_release = OsRelease::default();

        for line in lines {
            starts_with_match!(line.as_str(), {
                "NAME=" => os_release.name,
                "VERSION=" => os_release.version,
                "ID=" => os_release.id,
                "ID_LIKE=" => os_release.id_like,
                "PRETTY_NAME=" => os_release.pretty_name,
                "VERSION_ID=" => os_release.version_id,
                "HOME_URL=" => os_release.home_url,
                "SUPPORT_URL=" => os_release.support_url,
                "BUG_REPORT_URL=" => os_release.bug_report_url,
                "PRIVACY_POLICY_URL=" => os_release.privacy_policy_url,
                "VERSION_CODENAME=" => os_release.version_codename
            });
        }

        os_release
    }

    pub fn new() -> io::Result<OsRelease> {
        let file = BufReader::new(File::open("/etc/os-release")?);
        Ok(OsRelease::from_iter(file.lines().flat_map(|line| line)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"NAME="Pop!_OS"
VERSION="18.04 LTS"
ID=ubuntu
ID_LIKE=debian
PRETTY_NAME="Pop!_OS 18.04 LTS"
VERSION_ID="18.04"
HOME_URL="https://system76.com/pop"
SUPPORT_URL="http://support.system76.com"
BUG_REPORT_URL="https://github.com/pop-os/pop/issues"
PRIVACY_POLICY_URL="https://system76.com/privacy"
VERSION_CODENAME=bionic"#;

    #[test]
    fn os_release() {
        let os_release = OsRelease::from_iter(EXAMPLE.lines().map(|x| x.into()));

        assert_eq!(os_release, OsRelease {
            name: "Pop!_OS".into(),
            version: "18.04 LTS".into(),
            id: "ubuntu".into(),
            id_like: "debian".into(),
            pretty_name: "Pop!_OS 18.04 LTS".into(),
            version_id: "18.04".into(),
            home_url: "https://system76.com/pop".into(),
            support_url: "http://support.system76.com".into(),
            bug_report_url: "https://github.com/pop-os/pop/issues".into(),
            privacy_policy_url: "https://system76.com/privacy".into(),
            version_codename: "bionic".into()
        })
    }
}
