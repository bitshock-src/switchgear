use clap::ValueEnum;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum Level {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Level {
    pub const ALL: [Level; 6] = [
        Level::Off,
        Level::Error,
        Level::Warn,
        Level::Info,
        Level::Debug,
        Level::Trace,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Off => "off",
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
        }
    }
}

impl Display for Level {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct ParseLevelError(String);

impl Display for ParseLevelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown level {:?}, expected one of off, error, warn, info, debug, trace",
            self.0
        )
    }
}

impl Error for ParseLevelError {}

impl FromStr for Level {
    type Err = ParseLevelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Level::ALL
            .into_iter()
            .find(|l| s.eq_ignore_ascii_case(l.as_str()))
        {
            Some(level) => Ok(level),
            None => Err(ParseLevelError(s.to_owned())),
        }
    }
}

impl Serialize for Level {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        match text.parse() {
            Ok(level) => Ok(level),
            Err(e) => Err(serde::de::Error::custom(e)),
        }
    }
}

impl From<Level> for tracing_subscriber::filter::LevelFilter {
    fn from(level: Level) -> Self {
        match level {
            Level::Off => Self::OFF,
            Level::Error => Self::ERROR,
            Level::Warn => Self::WARN,
            Level::Info => Self::INFO,
            Level::Debug => Self::DEBUG,
            Level::Trace => Self::TRACE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::filter::LevelFilter;

    #[test]
    fn a_level_parses_from_its_own_name_in_any_case() {
        for level in Level::ALL {
            let name = level.as_str();
            for spelling in [name.to_owned(), name.to_uppercase(), first_upper(name)] {
                let parsed = spelling
                    .parse::<Level>()
                    .unwrap_or_else(|e| panic!("{spelling}: {e}"));
                assert_eq!(parsed, level, "{spelling}");
            }
        }
    }

    #[test]
    fn a_name_that_is_not_a_level_is_rejected() {
        assert!("verbose".parse::<Level>().is_err());
        assert!("".parse::<Level>().is_err());
    }

    #[test]
    fn every_level_converts_to_its_filter() {
        for (level, expected) in [
            (Level::Off, LevelFilter::OFF),
            (Level::Error, LevelFilter::ERROR),
            (Level::Warn, LevelFilter::WARN),
            (Level::Info, LevelFilter::INFO),
            (Level::Debug, LevelFilter::DEBUG),
            (Level::Trace, LevelFilter::TRACE),
        ] {
            assert_eq!(LevelFilter::from(level), expected, "{level}");
        }
    }

    #[test]
    fn clap_offers_every_level_under_the_name_it_is_written_with() {
        for level in Level::value_variants() {
            let value = level
                .to_possible_value()
                .unwrap_or_else(|| panic!("{level} has no clap value"));
            assert_eq!(value.get_name(), level.as_str());
        }
        assert_eq!(Level::value_variants().len(), Level::ALL.len());
    }

    fn first_upper(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}
