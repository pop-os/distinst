extern crate distinst_utils as misc;
#[macro_use]
extern crate lazy_static;

use std::io::{self, BufRead, BufReader};
use std::iter::FromIterator;
use std::path::Path;

lazy_static! {
    /// The OS release detected on this host's environment.
    ///
    /// # Panics
    /// This will panic if the host does not have an `/etc/os-release` file.
    pub static ref OS_RELEASE: OsRelease = OsRelease::new().expect("unable to find /etc/os-release");
}

macro_rules! map_keys {
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
        &line[1..line.len() - 1]
    } else {
        line
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OsRelease {
    pub bug_report_url:     String,
    pub home_url:           String,
    pub id_like:            String,
    pub id:                 String,
    pub name:               String,
    pub pretty_name:        String,
    pub privacy_policy_url: String,
    pub support_url:        String,
    pub version_codename:   String,
    pub version_id:         String,
    pub version:            String,
}

impl OsRelease {
    pub fn new() -> io::Result<OsRelease> {
        let file = BufReader::new(misc::open("/etc/os-release")?);
        Ok(OsRelease::from_iter(file.lines().flat_map(|line| line)))
    }

    pub fn new_from<P: AsRef<Path>>(path: P) -> io::Result<OsRelease> {
        let file = BufReader::new(misc::open(&path)?);
        Ok(OsRelease::from_iter(file.lines().flat_map(|line| line)))
    }
}

impl FromIterator<String> for OsRelease {
    fn from_iter<I: IntoIterator<Item = String>>(lines: I) -> Self {
        let mut os_release = Self::default();

        for line in lines {
            map_keys!(line.as_str(), {
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

        assert_eq!(
            os_release,
            OsRelease {
                name:               "Pop!_OS".into(),
                version:            "18.04 LTS".into(),
                id:                 "ubuntu".into(),
                id_like:            "debian".into(),
                pretty_name:        "Pop!_OS 18.04 LTS".into(),
                version_id:         "18.04".into(),
                home_url:           "https://system76.com/pop".into(),
                support_url:        "http://support.system76.com".into(),
                bug_report_url:     "https://github.com/pop-os/pop/issues".into(),
                privacy_policy_url: "https://system76.com/privacy".into(),
                version_codename:   "bionic".into(),
            }
        )
    }
}
