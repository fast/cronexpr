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
#[derive(Clone)]
pub struct Error {
    text: String,
    parse: Option<ParseError>,
}

// Keep parse diagnostics structured until display so constructing an error only copies the input.
#[derive(Debug, Clone)]
struct ParseError {
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
    /// Creates a new error with the given message.
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            text: msg.into(),
            parse: None,
        }
    }

    pub(crate) fn with_context(context: impl fmt::Display, error: impl fmt::Display) -> Self {
        Self::new(format!("{context}: {error}"))
    }

    pub(crate) fn parse(input: &str, offset: usize, reason: ParseErrorReason) -> Self {
        Self {
            text: input.to_owned(),
            parse: Some(ParseError { offset, reason }),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(ParseError { offset, reason }) = &self.parse else {
            return f.write_str(&self.text);
        };

        write!(
            f,
            "failed to parse crontab expression:\n{}\n{:offset$}^ ",
            self.text, ""
        )?;
        match reason {
            ParseErrorReason::Message(message) => f.write_str(message),
            ParseErrorReason::UnknownTimezone => write!(
                f,
                "failed to find timezone {}; \
                for a list of time zones, see the list of tz database time zones on Wikipedia: \
                https://en.wikipedia.org/wiki/List_of_tz_database_time_zones#List",
                &self.text[*offset..]
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

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error({:?})", self.to_string())
    }
}

impl std::error::Error for Error {}
