#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VarEntry {
    pub name: String,
    pub value: String,
    pub is_secret: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VarGroup {
    pub name: String,
    pub variables: Vec<VarEntry>,
}

#[derive(Clone, Copy, Default, Debug)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
}

impl StatusMessage {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            text: message.into(),
            kind: StatusKind::Info,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            text: message.into(),
            kind: StatusKind::Error,
        }
    }
}
