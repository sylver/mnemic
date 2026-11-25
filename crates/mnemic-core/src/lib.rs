use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("{0} is empty")]
    Empty(&'static str),
}

pub fn require_non_empty<'a>(label: &'static str, value: &'a str) -> Result<&'a str, Error> {
    if value.is_empty() {
        Err(Error::Empty(label))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_value() {
        assert_eq!(require_non_empty("name", ""), Err(Error::Empty("name")));
    }

    #[test]
    fn keeps_non_empty_value() {
        assert_eq!(require_non_empty("name", "mnemic"), Ok("mnemic"));
    }
}

fn clippy_and_fmt() {
let value = 1;
    if value == 1 {
    if value == 1 {
        let _kept = value;
    }
    }
}
