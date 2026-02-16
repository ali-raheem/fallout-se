use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gender {
    Male,
    Female,
    Unknown(i32),
}

impl Gender {
    pub const MALE_RAW: i32 = 0;
    pub const FEMALE_RAW: i32 = 1;

    pub fn from_raw(raw: i32) -> Self {
        match raw {
            Self::MALE_RAW => Self::Male,
            Self::FEMALE_RAW => Self::Female,
            other => Self::Unknown(other),
        }
    }

    pub fn raw(&self) -> i32 {
        match *self {
            Self::Male => Self::MALE_RAW,
            Self::Female => Self::FEMALE_RAW,
            Self::Unknown(other) => other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match *self {
            Self::Male => "Male",
            Self::Female => "Female",
            Self::Unknown(_) => "Unknown",
        }
    }
}

impl fmt::Display for Gender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Unknown(v) => write!(f, "Unknown ({})", v),
            _ => f.write_str(self.as_str()),
        }
    }
}
