use std::borrow::Cow;

use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, PartialEq)]
pub enum Fragment {
    Str(String),
    Name,
    Hash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassNamePattern(Vec<Fragment>);

impl ClassNamePattern {
    pub fn apply(&self, classname: &str, hash: &str) -> String {
        self.0
            .iter()
            .map(|v| match v {
                Fragment::Str(s) => s,
                Fragment::Name => classname,
                Fragment::Hash => hash,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

impl Default for ClassNamePattern {
    fn default() -> Self {
        Self(vec![
            Fragment::Name,
            Fragment::Str("-".into()),
            Fragment::Hash,
        ])
    }
}

impl<'de> Deserialize<'de> for ClassNamePattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<str> = Deserialize::deserialize(deserializer)?;

        match parse::parse_pattern(&s) {
            Ok(v) => Ok(v),
            Err(e) => Err(serde::de::Error::custom(e)),
        }
    }
}

mod parse {
    use super::*;
    use winnow::{
        combinator::{alt, repeat},
        error::{ContextError, ParseError},
        token::take_till,
        PResult, Parser,
    };
    fn fragment(input: &mut &str) -> PResult<Fragment> {
        alt((
            "[name]".value(Fragment::Name),
            "[hash]".value(Fragment::Hash),
            take_till(1.., '[').map(|s: &str| Fragment::Str(s.into())),
        ))
        .parse_next(input)
    }

    fn pattern(input: &mut &str) -> PResult<Vec<Fragment>> {
        repeat(0.., fragment).parse_next(input)
    }

    pub fn parse_pattern(input: &str) -> Result<ClassNamePattern, ParseError<&str, ContextError>> {
        Ok(ClassNamePattern(pattern.parse(input)?))
    }
}

#[cfg(test)]
mod test {

    use crate::class_name_pattern::ClassNamePattern;

    #[test]
    fn test_pattern_deserialize() {
        let pattern: ClassNamePattern =
            serde_json::from_str("\"test-[name]-[hash]\"").expect("should deserialize");

        assert_eq!("test-my-class-12345", pattern.apply("my-class", "12345"));
    }
}
