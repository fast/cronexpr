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

use std::str::FromStr;

use jiff::RoundMode;
use jiff::Span;
use jiff::Timestamp;
use jiff::ToSpan;
use jiff::Unit;
use jiff::Zoned;
use jiff::ZonedRound;
use jiff::civil::Weekday;
use jiff::tz::TimeZone;

use crate::error::Error;
use crate::literal_set::LiteralSet;

/// A data struct representing the crontab expression.
#[derive(Debug, Clone)]
pub struct Crontab {
    minutes: PossibleLiterals,
    hours: PossibleLiterals,
    months: PossibleLiterals,
    days_of_month: ParsedDaysOfMonth,
    days_of_week: ParsedDaysOfWeek,
    timezone: TimeZone,
}

impl Crontab {
    pub(crate) fn new(
        minutes: PossibleLiterals,
        hours: PossibleLiterals,
        months: PossibleLiterals,
        days_of_month: ParsedDaysOfMonth,
        days_of_week: ParsedDaysOfWeek,
        timezone: TimeZone,
    ) -> Self {
        Self {
            minutes,
            hours,
            months,
            days_of_month,
            days_of_week,
            timezone,
        }
    }
}

/// Literal values accepted by a cron field.
#[derive(Debug, Clone)]
pub struct PossibleLiterals {
    values: LiteralSet,
}

impl PossibleLiterals {
    pub(crate) fn new(values: LiteralSet) -> Self {
        Self { values }
    }

    fn matches(&self, value: u8) -> bool {
        self.values.contains(value)
    }
}

#[derive(Debug, Clone)]
pub struct ParsedDaysOfWeek {
    /// Literal weekdays accepted by this field.
    literals: LiteralSet,
    /// Weekdays selected by the `<weekday>L` extension.
    last_days_of_week: LiteralSet,
    /// Ordinal weekdays selected by the `<weekday>#<nth>` extension.
    ///
    /// Each pair occupies the bit returned by [`encode_nth_weekday`].
    nth_days_of_week: LiteralSet,

    // to implement Vixie's cron behavior
    // ref - https://crontab.guru/cron-bug.html
    start_with_asterisk: bool,
}

impl ParsedDaysOfWeek {
    pub(crate) fn new(
        literals: LiteralSet,
        last_days_of_week: LiteralSet,
        nth_days_of_week: LiteralSet,
        start_with_asterisk: bool,
    ) -> Self {
        Self {
            literals,
            last_days_of_week,
            nth_days_of_week,
            start_with_asterisk,
        }
    }

    fn matches(&self, value: &Zoned) -> bool {
        let weekday = value.weekday();
        if self.literals.contains(weekday as u8) {
            return true;
        }

        if self.last_days_of_week.contains(weekday as u8)
            && (value + 1.week()).month() > value.month()
        {
            return true;
        }

        let nth = ((value.day() - 1) / 7 + 1) as u8;
        if self
            .nth_days_of_week
            .contains(encode_nth_weekday(nth, weekday))
        {
            return true;
        }

        false
    }
}

/// Map the 5 possible ordinals and 7 weekdays into the 35 bits starting at zero.
pub(crate) fn encode_nth_weekday(nth: u8, weekday: Weekday) -> u8 {
    debug_assert!((1..=5).contains(&nth));
    (nth - 1) * 7 + weekday as u8 - 1
}

#[derive(Debug, Clone)]
pub struct ParsedDaysOfMonth {
    /// Literal days accepted by this field.
    literals: LiteralSet,
    /// Whether the `L` extension is present.
    last_day_of_month: bool,
    /// Days selected by the `<day>W` extension.
    nearest_weekdays: LiteralSet,

    // to implement Vixie's cron behavior
    // ref - https://crontab.guru/cron-bug.html
    start_with_asterisk: bool,
}

impl ParsedDaysOfMonth {
    pub(crate) fn new(
        literals: LiteralSet,
        last_day_of_month: bool,
        nearest_weekdays: LiteralSet,
        start_with_asterisk: bool,
    ) -> Self {
        Self {
            literals,
            last_day_of_month,
            nearest_weekdays,
            start_with_asterisk,
        }
    }

    fn matches(&self, value: &Zoned) -> bool {
        if self.literals.contains(value.day() as u8) {
            return true;
        }

        if self.last_day_of_month && (value + 1.day()).month() > value.month() {
            return true;
        }

        for day in self.nearest_weekdays.iter() {
            let day = day as i8;

            match value.weekday() {
                // 'nearest weekday' matcher can never match weekends
                Weekday::Saturday | Weekday::Sunday => {
                    continue;
                }
                // if today is Tuesday, Wednesday, or Thursday, only if the day matches today can
                // today be the nearest weekday
                Weekday::Tuesday | Weekday::Wednesday | Weekday::Thursday => {
                    if value.day() == day {
                        return true;
                    }
                }
                Weekday::Monday => {
                    // if the day matches today, today is the nearest weekday
                    if value.day() == day {
                        return true;
                    }

                    // matches the last Sunday
                    if value.day() - 1 == day {
                        return true;
                    }

                    // matches the edge case: 1W and the 1st is Saturday
                    if value.day() == 3 && day == 1 {
                        return true;
                    }
                }
                Weekday::Friday => {
                    // if the day matches today, today is the nearest weekday
                    if value.day() == day {
                        return true;
                    }

                    let last_day_of_this_month = value.days_in_month();

                    // matches the next Saturday
                    if value.day() + 1 == day && day <= last_day_of_this_month {
                        return true;
                    }

                    // matches the edge case: last day of month is Sunday
                    if value.day() + 2 == day && day == last_day_of_this_month {
                        return true;
                    }
                }
            }
        }

        false
    }
}

/// A helper struct to construct a [`Timestamp`]. This is useful to avoid version lock-in to
/// [`jiff`].
///
/// # Examples
///
/// ## Make timestamp from String
///
/// The `MakeTimestamp` struct can be created by parsing a string representation of a timestamp.
///
/// ```rust
/// use cronexpr::MakeTimestamp;
///
/// // FromStr
/// let make_timestamp: MakeTimestamp = "2024-01-01T00:00:00Z".parse().unwrap();
/// assert_eq!("2024-01-01T00:00:00Z", make_timestamp.0.to_string());
///
/// // TryFrom<&str>
/// let make_timestamp = MakeTimestamp::try_from("2024-01-01T00:00:00Z").unwrap();
/// assert_eq!("2024-01-01T00:00:00Z", make_timestamp.0.to_string());
/// ```
///
/// ## Make timestamp from duration to [`UNIX_EPOCH`](std::time::UNIX_EPOCH)
///
/// ```rust
/// use cronexpr::MakeTimestamp;
///
/// let make_timestamp = MakeTimestamp::from_second(-1704067200).unwrap();
/// assert_eq!("1916-01-02T00:00:00Z", make_timestamp.0.to_string());
///
/// let make_timestamp = MakeTimestamp::from_second(1704067200).unwrap();
/// assert_eq!("2024-01-01T00:00:00Z", make_timestamp.0.to_string());
///
/// let make_timestamp = MakeTimestamp::from_millisecond(1704067200000).unwrap();
/// assert_eq!("2024-01-01T00:00:00Z", make_timestamp.0.to_string());
///
/// let make_timestamp = MakeTimestamp::from_microsecond(1704067200000000).unwrap();
/// assert_eq!("2024-01-01T00:00:00Z", make_timestamp.0.to_string());
///
/// let make_timestamp = MakeTimestamp::from_nanosecond(1704067200000000000).unwrap();
/// assert_eq!("2024-01-01T00:00:00Z", make_timestamp.0.to_string());
/// ```
///
/// ## Make timestamp from jiff's [`Timestamp`]
///
/// You can create a `MakeTimestamp` instance from an existing jiff's `Timestamp` by using the
/// `From` trait implementation:
///
/// ```rust
/// let timestamp = jiff::Timestamp::from_second(1704067200).unwrap();
/// let make_timestamp = cronexpr::MakeTimestamp::from(timestamp);
///
/// assert_eq!("2024-01-01T00:00:00Z", make_timestamp.0.to_string());
/// ```
#[derive(Debug, Copy, Clone)]
pub struct MakeTimestamp(pub Timestamp);

impl From<Timestamp> for MakeTimestamp {
    fn from(timestamp: Timestamp) -> Self {
        MakeTimestamp(timestamp)
    }
}

impl FromStr for MakeTimestamp {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Timestamp::from_str(input)
            .map(MakeTimestamp)
            .map_err(|error| Error::message(format!("failed to parse timestamp: {error}")))
    }
}

impl<'a> TryFrom<&'a str> for MakeTimestamp {
    type Error = Error;

    fn try_from(input: &'a str) -> Result<Self, Self::Error> {
        FromStr::from_str(input)
    }
}

impl MakeTimestamp {
    pub fn from_second(second: i64) -> Result<Self, Error> {
        Timestamp::from_second(second)
            .map(MakeTimestamp)
            .map_err(|error| Error::message(format!("failed to make timestamp: {error}")))
    }

    pub fn from_millisecond(millisecond: i64) -> Result<Self, Error> {
        Timestamp::from_millisecond(millisecond)
            .map(MakeTimestamp)
            .map_err(|error| Error::message(format!("failed to make timestamp: {error}")))
    }

    pub fn from_microsecond(microsecond: i64) -> Result<Self, Error> {
        Timestamp::from_microsecond(microsecond)
            .map(MakeTimestamp)
            .map_err(|error| Error::message(format!("failed to make timestamp: {error}")))
    }

    pub fn from_nanosecond(nanosecond: i128) -> Result<Self, Error> {
        Timestamp::from_nanosecond(nanosecond)
            .map(MakeTimestamp)
            .map_err(|error| Error::message(format!("failed to make timestamp: {error}")))
    }
}

impl Crontab {
    /// Create an infinite iterator over next timestamps after `start`.
    ///
    /// # Errors
    ///
    /// This returns an error if fail to make timestamp from the input of `start`.
    ///
    /// For more usages, see [the top-level documentation][crate].
    pub fn iter_after<T>(&self, start: T) -> Result<CronTimesIter, Error>
    where
        T: TryInto<MakeTimestamp>,
        T::Error: std::error::Error,
    {
        let start = start
            .try_into()
            .map_err(|error| Error::message(format!("failed to parse start timestamp: {error}")))?;

        Ok(CronTimesIter {
            crontab: self.clone(),
            timestamp: start.0,
        })
    }

    /// Find the next timestamp after the given timestamp.
    ///
    /// # Errors
    ///
    /// This returns an error if fail to make timestamp from the input of `timestamp`. Or fail to
    /// advance the timestamp.
    ///
    /// For more usages, see [the top-level documentation][crate].
    pub fn find_next<T>(&self, timestamp: T) -> Result<Zoned, Error>
    where
        T: TryInto<MakeTimestamp>,
        T::Error: std::error::Error,
    {
        let zoned = timestamp
            .try_into()
            .map(|ts| ts.0.to_zoned(self.timezone.clone()))
            .map_err(|error| Error::message(format!("failed to parse timestamp: {error}")))?;

        // checked at most 4 years to cover the leap year case
        let bound = &zoned + 4.years();

        // at least should be the next minutes
        let mut next = zoned;
        next = advance_time_and_round(next, 1.minute(), Some(Unit::Minute))?;

        loop {
            if next > bound {
                return Err(Error::message(format!(
                    "failed to find next timestamp in four years; end with {next}"
                )));
            }

            match self.matches_or_next(next)? {
                Ok(matched) => break Ok(matched),
                Err(candidate) => next = candidate,
            }
        }
    }

    /// Returns whether this crontab matches the given timestamp.
    ///
    /// The function checks each cron field (minutes, hours, day of month, month) against the
    /// provided `timestamp` to determine if it aligns with the crontab expression. Each field is
    /// checked for a match, and all fields must match for the entire pattern to be considered a
    /// match.
    ///
    /// ## Errors
    ///
    /// This returns an error if fail to make timestamp from the input of `timestamp`. Or fail to
    /// advance the timestamp.
    ///
    /// If you're sure the input is valid, you can treat the error as `false`.
    ///
    /// ```rust
    /// let crontab = cronexpr::parse_crontab("*/10 0 * OCT MON UTC").unwrap();
    /// assert!(crontab.matches("2020-10-19T00:20:00Z").unwrap());
    /// assert!(crontab.matches("2020-10-19T00:30:00Z").unwrap());
    /// assert!(!crontab.matches("2020-10-20T00:31:00Z").unwrap());
    /// assert!(!crontab.matches("2020-10-20T01:30:00Z").unwrap());
    /// assert!(!crontab.matches("2020-10-20T00:30:00Z").unwrap());
    /// ```
    ///
    /// For more usages, see [the top-level documentation][crate].
    pub fn matches<T>(&self, timestamp: T) -> Result<bool, Error>
    where
        T: TryInto<MakeTimestamp>,
        T::Error: std::error::Error,
    {
        let zoned = timestamp
            .try_into()
            .map(|ts| ts.0.to_zoned(self.timezone.clone()))
            .map_err(|error| Error::message(format!("failed to parse timestamp: {error}")))?;

        Ok(self.matches_or_next(zoned)?.is_ok())
    }

    /// The inner result returns [`Ok`] if `ts` matches the crontab. Otherwise, returns [`Err`] that
    /// contains the next [`Zoned`] to test.
    fn matches_or_next(&self, zdt: Zoned) -> Result<Result<Zoned, Zoned>, Error> {
        if !self.months.matches(zdt.month() as u8) {
            let rest_days = zdt.days_in_month() - zdt.day() + 1;
            return advance_time_and_round(zdt, rest_days.days(), Some(Unit::Day)).map(Err);
        }

        // implement Vixie's cron bug: https://crontab.guru/cron-bug.html
        if self.days_of_month.start_with_asterisk || self.days_of_week.start_with_asterisk {
            // 1. use intersection if any of the two fields start with '*'
            let cond = self.days_of_month.matches(&zdt) && self.days_of_week.matches(&zdt);
            if !cond {
                return advance_time_and_round(zdt, 1.day(), Some(Unit::Day)).map(Err);
            }
        } else {
            // 2. otherwise, use union
            let cond = self.days_of_month.matches(&zdt) || self.days_of_week.matches(&zdt);
            if !cond {
                return advance_time_and_round(zdt, 1.day(), Some(Unit::Day)).map(Err);
            }
        }

        if !self.hours.matches(zdt.hour() as u8) {
            return advance_time_and_round(zdt, 1.hour(), Some(Unit::Hour)).map(Err);
        }

        if !self.minutes.matches(zdt.minute() as u8) {
            return advance_time_and_round(zdt, 1.minute(), Some(Unit::Minute)).map(Err);
        }

        Ok(Ok(zdt)) // zdt matches this crontab
    }
}

/// An iterator over the times matching the contained cron value. Created with
/// [`Crontab::iter_after`].
#[derive(Debug)]
pub struct CronTimesIter {
    /// The crontab to find the next timestamp.
    crontab: Crontab,
    /// The current timestamp; mutable.
    timestamp: Timestamp,
}

impl Iterator for CronTimesIter {
    type Item = Result<Zoned, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.crontab.find_next(self.timestamp) {
            Ok(zoned) => {
                self.timestamp = zoned.timestamp();
                Some(Ok(zoned))
            }
            Err(err) => Some(Err(err)),
        }
    }
}

fn advance_time_and_round(zdt: Zoned, span: Span, unit: Option<Unit>) -> Result<Zoned, Error> {
    let mut next = zdt;

    next = next.checked_add(span).map_err(|error| {
        Error::message(format!(
            "failed to advance timestamp; end with {next}: {error}"
        ))
    })?;

    if let Some(unit) = unit {
        next = next
            .round(ZonedRound::new().mode(RoundMode::Trunc).smallest(unit))
            .map_err(|error| {
                Error::message(format!(
                    "failed to round timestamp; end with {next}: {error}"
                ))
            })?;
    }

    Ok(next)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use insta::assert_snapshot;
    use jiff::Zoned;

    use crate::CronTimesIter;
    use crate::Crontab;

    fn make_iter(crontab: &str, timestamp: &str) -> CronTimesIter {
        let crontab = Crontab::from_str(crontab).unwrap();
        crontab.iter_after(timestamp).unwrap()
    }

    fn next(iter: &mut CronTimesIter) -> Zoned {
        iter.next().unwrap().unwrap()
    }

    #[test]
    fn test_next_timestamp() {
        let mut iter = make_iter("0 0 1 1 * Asia/Shanghai", "2024-01-01T00:00:00+08:00");
        assert_snapshot!(next(&mut iter), @"2025-01-01T00:00:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("2 4 * * * Asia/Shanghai", "2024-09-11T19:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-09-12T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-13T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-14T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-15T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-16T04:02:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("0 0 31 * * Asia/Shanghai", "2024-09-11T19:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-10-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-12-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-01-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-03-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-05-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-07-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-08-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-10-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-12-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-01-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-03-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-05-31T00:00:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("0 18 * * 1-5 Asia/Shanghai", "2024-09-11T19:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-09-12T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-13T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-16T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-17T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-18T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-09-19T18:00:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("0 18 * * TUE#1 Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-10-01T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-11-05T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-12-03T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-01-07T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-02-04T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-03-04T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-04-01T18:00:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("4 2 * * 1L Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-09-30T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-10-28T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-11-25T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-01-27T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-02-24T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-03-31T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-04-28T02:04:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("0 18 * * FRI#5 Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-11-29T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-01-31T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-05-30T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-08-29T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-10-31T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-01-30T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-05-29T18:00:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter(
            "3 11 L JAN-FEB,5 * Asia/Shanghai",
            "2024-09-24T00:08:35+08:00",
        );
        assert_snapshot!(next(&mut iter), @"2025-01-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-02-28T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-05-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-01-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-02-28T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2026-05-31T11:03:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("3 11 17W,L * * Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-09-30T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-10-17T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-10-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-11-18T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-11-30T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-12-17T11:03:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("3 11 1W * * Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-10-01T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-11-01T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-12-02T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-01-01T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-02-03T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-03-03T11:03:00+08:00[Asia/Shanghai]");

        let mut iter = make_iter("3 11 31W * * Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(next(&mut iter), @"2024-10-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2024-12-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-01-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-03-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-05-30T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-07-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-08-29T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(next(&mut iter), @"2025-10-31T11:03:00+08:00[Asia/Shanghai]");
    }
}
