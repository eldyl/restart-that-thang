use std::ffi::OsStr;
use std::fmt::Display;

#[derive(Debug, Clone, Hash, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContainerName(String);

impl ContainerName {
    pub fn new(name_str: impl Into<String>) -> Self {
        Self(name_str.into())
    }
}

impl AsRef<str> for ContainerName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<OsStr> for ContainerName {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}

impl From<String> for ContainerName {
    fn from(value: String) -> Self {
        ContainerName::new(value)
    }
}

impl From<&str> for ContainerName {
    fn from(value: &str) -> Self {
        ContainerName::new(value)
    }
}

impl Display for ContainerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
