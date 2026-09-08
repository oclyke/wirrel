//! The commit-subject grammar: `<type>: <description>`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitKind {
    Create,
    Update,
    Move,
    Meta,
}

impl CommitKind {
    pub fn label(&self) -> &'static str {
        match self {
            CommitKind::Create => "create",
            CommitKind::Update => "update",
            CommitKind::Move => "move",
            CommitKind::Meta => "meta",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub kind: CommitKind,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

pub fn parse_subject(subject: &str) -> Result<Subject, ParseError> {
    let (ty, description) = subject
        .split_once(": ")
        .ok_or_else(|| ParseError(format!("missing `<type>: ` prefix in {subject:?}")))?;

    let kind = match ty {
        "create" => CommitKind::Create,
        "update" => CommitKind::Update,
        "move" => CommitKind::Move,
        "meta" => CommitKind::Meta,
        other => return Err(ParseError(format!("unknown type {other:?}"))),
    };

    if description.trim().is_empty() {
        return Err(ParseError("empty description".into()));
    }

    Ok(Subject { kind, description: description.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_known_types() {
        assert_eq!(parse_subject("create: x").unwrap().kind, CommitKind::Create);
        assert_eq!(parse_subject("move: x").unwrap().kind, CommitKind::Move);
    }

    #[test]
    fn rejects_bad_subjects() {
        assert!(parse_subject("feat: nope").is_err()); // unknown type
        assert!(parse_subject("create").is_err()); // no prefix
        assert!(parse_subject("create:  ").is_err()); // empty description
    }
}
