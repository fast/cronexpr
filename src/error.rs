// Copyright 2024 tison <wander4096@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt;

/// An error that can occur in this crate.
#[derive(Debug, Clone)]
pub struct Error(ErrorKind);

#[derive(Debug, Clone)]
enum ErrorKind {
    Message(String),
    Parse(ParseError),
}

#[derive(Debug, Clone)]
struct ParseError {
    input: String,
    offset: usize,
    reason: ParseErrorReason,
}

#[derive(Debug, Clone)]
pub(crate) enum ParseErrorReason {
    Message(&'static str),
    UnknownTimezone,
    RangeNotAscending {
        start: u8,
        end: u8,
    },
    OutOfRange {
        subject: &'static str,
        min: u8,
        max: u8,
        value: u64,
    },
}

impl Error {
    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self(ErrorKind::Message(message.into()))
    }

    pub(crate) fn parse(input: &str, offset: usize, reason: ParseErrorReason) -> Self {
        Self(ErrorKind::Parse(ParseError {
            input: input.to_owned(),
            offset,
            reason,
        }))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ErrorKind::Message(message) => f.write_str(message),
            ErrorKind::Parse(error) => error.fmt(f),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            input,
            offset,
            reason,
        } = self;

        write!(
            f,
            "failed to parse crontab expression:\n{}\n{:offset$}^ ",
            input, ""
        )?;
        match reason {
            ParseErrorReason::Message(message) => f.write_str(message),
            ParseErrorReason::UnknownTimezone => write!(
                f,
                "failed to find timezone {}; \
                for a list of time zones, see the list of tz database time zones on Wikipedia: \
                https://en.wikipedia.org/wiki/List_of_tz_database_time_zones#List",
                &input[*offset..]
            ),
            ParseErrorReason::RangeNotAscending { start, end } => {
                write!(f, "range must be in ascending order; found {start}-{end}")
            }
            ParseErrorReason::OutOfRange {
                subject,
                min,
                max,
                value,
            } => {
                write!(f, "{subject} must be in range {min}..={max}; found {value}")
            }
        }
    }
}

impl std::error::Error for Error {}
