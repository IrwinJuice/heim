use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct HeimError {
    pub kind: ErrorKind,
    pub message: &'static str,
}

#[derive(Debug)]
pub enum ErrorKind {
    ArtifactError,
}

impl Display for HeimError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}
