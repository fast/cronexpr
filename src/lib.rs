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

//! `cronexpr` is a library to parse and drive the crontab expression.
//!
//! Here is a quick example that shows how to parse a cron expression and drive it with a timestamp:
//!
//!```rust
//! use std::str::FromStr;
//!
//! use cronexpr::MakeTimestamp;
//!
//! let crontab = cronexpr::parse_crontab("2 4 * * * Asia/Shanghai").unwrap();
//!
//! // case 0. match timestamp
//! assert!(crontab.matches("2024-09-24T04:02:00+08:00").unwrap());
//! assert!(!crontab.matches("2024-09-24T04:01:00+08:00").unwrap());
//!
//! // case 1. find next timestamp with timezone
//! assert_eq!(
//!     crontab
//!         .find_next("2024-09-24T10:06:52+08:00")
//!         .unwrap()
//!         .to_string(),
//!     "2024-09-25T04:02:00+08:00[Asia/Shanghai]"
//! );
//!
//! // case 2. iter over next timestamps without upper bound
//! let iter = crontab.iter_after("2024-09-24T10:06:52+08:00").unwrap();
//! assert_eq!(
//!     iter.take(5)
//!         .map(|ts| ts.map(|ts| ts.to_string()))
//!         .collect::<Result<Vec<_>, cronexpr::Error>>()
//!         .unwrap(),
//!     vec![
//!         "2024-09-25T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-26T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-27T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-28T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-29T04:02:00+08:00[Asia/Shanghai]",
//!     ]
//! );
//!
//! // case 3. iter over next timestamps with upper bound
//! let iter = crontab.iter_after("2024-09-24T10:06:52+08:00").unwrap();
//! let end = MakeTimestamp::from_str("2024-10-01T00:00:00+08:00").unwrap();
//! assert_eq!(
//!     iter.take_while(|ts| ts.as_ref().map(|ts| ts.timestamp() < end.0).unwrap_or(true))
//!         .map(|ts| ts.map(|ts| ts.to_string()))
//!         .collect::<Result<Vec<_>, cronexpr::Error>>()
//!         .unwrap(),
//!     vec![
//!         "2024-09-25T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-26T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-27T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-28T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-29T04:02:00+08:00[Asia/Shanghai]",
//!         "2024-09-30T04:02:00+08:00[Asia/Shanghai]",
//!     ]
//! );
//! ```
//!
//! For more complex and edge cases, read the [Edge cases](#edge-cases) section.
//!
//! # Syntax overview
//!
//! This crates supports all the syntax of [standard crontab] and most of the non-standard
//! extensions.
//!
//! The mainly difference is that this crate may accept an explicit timezone in the crontab
//! expression, which is necessary to determine the next timestamp. The timezone is required by
//! default. You can use [`parse_crontab_with`] to switch to the optional timezone mode.
//!
//! [standard crontab]: https://en.wikipedia.org/wiki/Cron#Cron_expression
//!
//! ```markdown
//! *    *    *    *    *    <timezone>
//! ┬    ┬    ┬    ┬    ┬    ────┬────
//! │    │    │    │    │        |
//! │    │    │    │    │        └──── timezone     UTC, Asia/Shanghai, and so on
//! │    │    │    │    └───────────── day of week  0-7, SUN-SAT (0 or 7 is Sunday)
//! │    │    │    └────────────────── month        1-12, JAN-DEC
//! │    │    └─────────────────────── day of month 1-31
//! │    └──────────────────────────── hour         0-23
//! └───────────────────────────────── minute       0-59
//! ```
//!
//! This crate also supports the following non-standard extensions:
//!
//! * [Last day of month (`L`)](#last-day-of-month-l)
//! * [Nearest weekday (`1W`, `15W`, etc.)](#nearest-weekday-1w-15w-etc)
//! * [Last day of week (`5L`)](#last-day-of-week-5l)
//! * [Nth day of week (`5#3`)](#nth-day-of-week-53)
//!
//! # Timezone
//!
//! Timezone is parsed internally by [`jiff::tz::TimeZone::get`][TimeZone::get]. It supports all the
//! timezone names in the IANA Time Zone Database. See [the list of time zones](https://en.wikipedia.org/wiki/List_of_tz_database_time_zones#List).
//!
//! # Single value
//!
//! Every field (except timezone) can be a single value.
//!
//! * For minutes, it can be from 0 to 59.
//! * For hours, it can be from 0 to 23.
//! * For days of month, it can be from 1 to 31.
//!
//! For months, it can be 1-12. Alternatively, it can be the first three letters of the English
//! name of the month (case-insensitive), such as `JAN`, `Feb`, etc. `JAN` will be mapped to 1,
//! `Feb` will be mapped to 2, and so on.
//!
//! For days of week, it can be 0-7, where both 0 and 7 represent Sunday. Alternatively, it can be
//! the first three letters of the English name of the day (case-insensitive), such as `SUN`, `Mon`,
//! etc. `SUN` will be mapped to 0, `Mon` will be mapped to 1, and so on.
//!
//! Days of week and days of month support extra syntax, read their dedicated sections below.
//!
//! # Asterisk
//!
//! Asterisks (also known as wildcard) represents "all". For example, using `* * * * *` will run
//! every minute. Using `* * * * 1` will run every minute only on Monday.
//!
//! # Range
//!
//! Hyphen (`-`) defines ranges. For example, `JAN-JUN` indicates every month from January to June,
//! _inclusive_.
//!
//! Range bound can be any valid [single value](#single-value), but the left bound must be less than
//! or equal to the right bound.
//!
//! # Step
//!
//! In Vixie's cron, slash (`/`) can be combined with ranges to specify step values.
//!
//! For example, `*/10` in the minutes field indicates every 10 minutes (see note below about
//! frequencies). It is shorthand for the more verbose POSIX form `00,10,20,30,40,50`.
//!
//! Note that frequencies in general cannot be expressed; only step values which evenly divide their
//! range express accurate frequencies (for minutes and seconds, that's `/2`, `/3`, `/4`, `/5`,
//! `/6`, `/10`, `/12`, `/15`, `/20` and `/30` because 60 is evenly divisible by those numbers; for
//! hours, that's `/2`, `/3`, `/4`, `/6`, `/8` and `/12`); all other possible "steps" and all other
//! fields yield inconsistent "short" periods at the end of the time-unit before it "resets" to the
//! next minute, hour, or day; for example, entering `*/5` for the day field sometimes executes
//! after 1, 2, or 3 days, depending on the month and leap year; this is because cron is stateless
//! (it does not remember the time of the last execution nor count the difference between it and
//! now, required for accurate frequency counting—instead, cron is a mere pattern-matcher).
//!
//! This crate requires the step value to be in the range of the field and not zero.
//!
//! The range to be stepped can be any valid [single value](#single-value), [asterisk](#asterisk),
//! or [range](#range).
//!
//! When it's a single value `v`, it's expanded to a range `v-<field range end>`. For example,
//! `15/XX` is the same as a Vixie's cron schedule of `15-59/10` in the minutes section. Similarly,
//! you can remove the extra `-23` from `0-23/XX`, `-31` from `1-31/XX`, and `-12` from `1-12/XX`
//! for hours, days, and months; respectively.
//!
//! Note that this is to support the existing widely adopted syntax, users are encouraged to use
//! the more explicit form.
//!
//! # List
//!
//! Commas (`,`) are used to separate items of a list. For example, using `MON,WED,FRI` in the 5th
//! field (day of week) means Mondays, Wednesdays and Fridays.
//!
//! The list can contain any valid [single value](#single-value), [asterisk](#asterisk),
//! [range](#range), or [step](#step). For days of week and days of month, it can also contain
//! extra syntax, read their dedicated sections below.
//!
//! List items are parsed delimited by commas. This takes the highest precedence in the parsing.
//! Thus, `1-10,40-50/2` is parsed as `1,2,3,4,5,6,7,8,9,10,40,42,44,46,48,50`.
//!
//! # Hashed value extension
//!
//! Starting from 1.1.0, the `H` character is allowed for all the fields (except timezone) when the
//! [`ParseOptions`]'s `hashed_value` field is not `None`.
//!
//! When the `hashed_value` is not `None`, the `H` character is treated as a single value that
//! maps `hashed_value` into the value range of that field:
//!
//!```rust
//! use std::ops::RangeInclusive;
//!
//! fn map_hash_into_range(hashed_value: u64, range: RangeInclusive<u8>) -> u8 {
//!     let modulo = range.end() - range.start() + 1;
//!     let hashed_value = hashed_value % modulo as u64;
//!     (range.start() + hashed_value as u8).min(*range.end())
//! }
//! ```
//!
//! `H` is firstly used in the Jenkins continuous integration system to indicate that a "hashed"
//! value is substituted. Thus instead of a fixed number such as '20 * * * *' which means at 20
//! minutes after the hour every hour, 'H * * * *' indicates that the task is performed every hour
//! at an unspecified but invariant time for each task. This allows spreading out tasks over time,
//! rather than having all of them start at the same time and compete for resources.
//!
//! # Day of month extension
//!
//! All the extensions below can be specified only alone or as a single item of a list, not in a
//! range or a step.
//!
//! ## Last day of month (`L`)
//!
//! The `L` character is allowed for the day-of-month field. This character specifies the last day
//! of the month.
//!
//! ## Nearest weekday (`1W`, `15W`, etc.)
//!
//! The `W` character is allowed for the day-of-month field. This character is used to specify the
//! weekday (Monday-Friday) nearest the given day. As an example, if `15W` is specified as the value
//! for the day-of-month field, the meaning is: "the nearest weekday to the 15th of the month." So,
//! if the 15th is a Saturday, the trigger fires on Friday the 14th. If the 15th is a Sunday, the
//! trigger fires on Monday the 16th. If the 15th is a Tuesday, then it fires on Tuesday the 15th.
//! However, if `1W` is specified as the value for day-of-month, and the 1st is a Saturday, the
//! trigger fires on Monday the 3rd, as it does not 'jump' over the boundary of a month's days.
//!
//! # Day of week extension
//!
//! All the extensions below can be specified only alone or as a single item of a list, not in a
//! range or a step.
//!
//! ## Last day of week (`5L`)
//!
//! The `L` character is allowed for the day-of-week field. This character specifies constructs such
//! as "the last Friday" (`5L`) of a given month.
//!
//! ## Nth day of week (`5#3`)
//!
//! The `#` character is allowed for the day-of-week field, and must be followed by a number between
//! one and five. It allows specifying constructs such as "the second Friday" of a given month. For
//! example, entering `5#3` in the day-of-week field corresponds to the third Friday of every month.
//!
//! # Edge cases
//!
//! ## The Vixie's cron bug became the de-facto standard
//!
//! Read [the article](https://crontab.guru/cron-bug.html) for more details.
//!
//! Typically, `0 12 *,10 * 2` is not equal to `0 12 10,* * 2`.
//!
//! ```rust
//! let crontab1 = cronexpr::parse_crontab("0 12 *,10 * 2 UTC").unwrap();
//! let crontab2 = cronexpr::parse_crontab("0 12 10,* * 2 UTC").unwrap();
//!
//! let ts = "2024-09-24T13:06:52Z";
//! assert_ne!(
//!     // "2024-10-01T12:00:00+00:00[UTC]"
//!     crontab1.find_next(ts).unwrap().to_string(),
//!     // "2024-09-25T12:00:00+00:00[UTC]"
//!     crontab2.find_next(ts).unwrap().to_string()
//! );
//! ```
//!
//! This crate implements the Vixie's cron behavior. That is,
//!
//! 1. Check if either the day of month or the day of week starts with asterisk (`*`).
//! 2. If so, match these two fields in interaction.
//! 3. If not, match these two fields in union.
//!
//! So, explain the example above:
//!
//! The first one's (`0 12 *,10 * 2 UTC`) day-of-month starts with an asterisk so cron uses
//! intersect. The schedule fires only on Tuesdays because `all-days-of-month ∩ Tuesday = Tuesday`.
//! It is the same schedule as `0 12 * * 2 UTC`.
//!
//! The second one's (`0 12 10,* * 2 UTC`) day-of-month has an asterisk in the day-of-month field,
//! but not as the first character. So cron uses union. The schedule fires every day because
//! `all-days-of-month ∪ Tuesday = all-days-of-month`. It is therefore the same as `0 12 * * * UTC`.
//!
//! Also, `0 12 1-31 * 2` is not equal to `0 12 * * 2`.
//!
//! ```rust
//! let crontab1 = cronexpr::parse_crontab("0 12 1-31 * 2 UTC").unwrap();
//! let crontab2 = cronexpr::parse_crontab("0 12 * * 2 UTC").unwrap();
//!
//! let ts = "2024-09-24T13:06:52Z";
//! assert_ne!(
//!     // "2024-09-25T12:00:00+00:00[UTC]"
//!     crontab1.find_next(ts).unwrap().to_string(),
//!     // "2024-10-01T12:00:00+00:00[UTC]"
//!     crontab2.find_next(ts).unwrap().to_string()
//! );
//! ```
//!
//! The first one fires every day (same as `0 12 1-31 * * UTC` or as `0 12 * * * UTC`), and the
//! second schedule fires only on Tuesdays.
//!
//! This bug is most likely to affect you when using step values. Quick reminder on step values:
//! `0-10/2` means every minute value from zero through ten (same as the list `0,2,4,6,8,10`), and
//! `*/3` means every third value. By using an asterisk with a step value for day-of-month or
//! day-of-week we put cron into the intersect mode producing unexpected results.
//!
//! Most of the time, we choose to use the wildcard to make the cron more legible. However, by now
//! you understand why `0 12 */2 * 0,6` does not run on every uneven day of the month plus on
//! Saturday and Sundays. Instead, due to this bug, it only runs if today is uneven and is also on a
//! weekend. To accomplish the former behaviour, you have to rewrite the schedule as `0 12 1-31/2 *
//! 0,6`.
//!
//!```rust
//! fn next(iter: &mut cronexpr::CronTimesIter) -> String {
//!     iter.next().unwrap().unwrap().to_string()
//! }
//!
//! let crontab1 = cronexpr::parse_crontab("0 12 */2 * 0,6 UTC").unwrap();
//! let mut iter1 = crontab1.iter_after("2024-09-24T13:06:52Z").unwrap();
//!
//! assert_eq!(next(&mut iter1), "2024-09-29T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter1), "2024-10-05T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter1), "2024-10-13T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter1), "2024-10-19T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter1), "2024-10-27T12:00:00+00:00[UTC]");
//!
//! let crontab2 = cronexpr::parse_crontab("0 12 1-31/2 * 0,6 UTC").unwrap();
//! let mut iter2 = crontab2.iter_after("2024-09-24T13:06:52Z").unwrap();
//!
//! assert_eq!(next(&mut iter2), "2024-09-25T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter2), "2024-09-27T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter2), "2024-09-28T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter2), "2024-09-29T12:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter2), "2024-10-01T12:00:00+00:00[UTC]");
//! ```
//!
//! ## Nearest weekday at the edge of the month
//!
//! Nearest weekday does not 'jump' over the boundary of a month's days.
//!
//! Thus, if `1W` is specified as the value for day-of-month, and the 1st is a Saturday, the trigger
//! fires on Monday the 3rd. (Although the nearest weekday to the 1st is the last day of the
//! previous month.)
//!
//! If `31W` is specified as the value for day-of-month, and the 31st is a Sunday, the trigger fires
//! on Friday the 29th. (Although the nearest weekday to the 31st is the 1st of the next month.)
//! This is the same for `30W`, `29W`, `28W`, etc. if the day is the last day of the month.
//!
//! If `31W` is specified as the value for day-of-month, the month does not have 31 days, the
//! trigger won't fire in the month. This is the same for `30W`, `29W`, etc.
//!
//! ## Nth day of week does not exist
//!
//! If the Nth day of week does not exist in the month, the trigger won't fire in the month.
//! This happens only when the month has less than five of the weekday.
//!
//! # FAQ
//!
//! ## Why do you create this crate?
//!
//! The other day when I was implementing the internal `CREATE JOB` statement in ScopeDB,
//! it comes to a requirement to support parsing and driving a crontab expression.
//!
//! Typically, the language interface looks like:
//!
//! ```sql
//! CREATE JOB do_retention
//! SCHEDULE = '* * * * * Asia/Shanghai'
//! AS
//!     DELETE FROM t WHERE now() - ts > 'PT10s'::interval;
//! ```
//!
//! The execution part of a traditional cron is the statement (`DELETE FROM ...`) here. Thus,
//! what I need is a library to parse the crontab expression and find the next timestamp to execute
//! the statement, without the need to execute the statement in the crontab library itself.
//!
//! There are several good candidates like [`croner`] and [`saffron`], but they are not suitable for
//! my use case. Both of them do _not_ support defining timezone in the expression which is
//! essential to my use case. Although `croner` support specific timezone later when matching, the
//! user experience is quite different. Also, the syntax that `croner` or `saffron` supports is
//! subtly different from my demand.
//!
//! [`croner`]: https://docs.rs/croner
//! [`saffron`]: https://docs.rs/saffron
//!
//! Other libraries are unmaintained or immature to use.
//!
//! Last, most candidates using [`chrono`] to processing datetime, while I'd prefer to extend the
//! [`jiff`] ecosystem.
//!
//! [`chrono`]: https://docs.rs/chrono
//!
//! ## Why does the crate require the timezone to be specified in the crontab expression?
//!
//! Mainly two reasons:
//!
//! 1. Without timezone information, you can _not_ perform daylight saving time (DST) arithmetic,
//!    and the result of the next timestamp may be incorrect.
//! 2. When define the crontab expression, people usually have a specific timezone in mind. It's
//!    more natural to specify the timezone in the expression, instead of having UTC as an implicit
//!    and forcing the user to convert the datetime to UTC.
//!
//! If there is a third reason, that is, it's how Snowflake does.
//!
//! Starting from 1.1.0, the timezone can be _optional_ by calling [`parse_crontab_with`] a
//! [`ParseOptions`] whose `fallback_timezone_option` is not [`None`](FallbackTimezoneOption::None).
//!
//! ## Why does [`Crontab::find_next`] and [`Crontab::iter_after`] only support exclusive bounds?
//!
//! Crontab jobs are schedule at most every minute. Bike-shedding the inclusive bounds is not
//! practical.
//!
//! If you'd like to try to match the boundary anyway, you can test it with [`Crontab::matches`]
//! before calling [`Crontab::find_next`] or [`Crontab::iter_after`].
//!
//! ## Why not support aliases like `@hourly` and `@reboot`?
//!
//! They are too handy to support and are totally different syntax in parsing.
//!
//! `@reboot` is meaningless since this crate only parse and drive a crontab expression, rather
//! than execute the command. Other aliases should be easily converted to the syntax this crate
//! supports.
//!
//! ## Why not support seconds and/or years?
//!
//! Crontab jobs are typically _not_ frequent tasks that run in seconds. Especially for scheduling
//! tasks in a distributed database, trying to specify a task in seconds is impractical.
//!
//! I don't actually schedule the task exactly at the timestamp, but record the previous timestamp,
//! and then schedule the task when `now` is greater than or equal to the next timestamp.
//!
//! For years, it's not a common use case for crontab jobs. This crate can already specify "every
//! year".
//!
//! ```rust
//! fn next(iter: &mut cronexpr::CronTimesIter) -> String {
//!     iter.next().unwrap().unwrap().to_string()
//! }
//!
//! // every year at 00:00:00 on January 1st
//! let crontab = cronexpr::parse_crontab("0 0 1 JAN * UTC").unwrap();
//! let mut iter = crontab.iter_after("2024-09-24T13:06:52Z").unwrap();
//!
//! assert_eq!(next(&mut iter), "2025-01-01T00:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter), "2026-01-01T00:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter), "2027-01-01T00:00:00+00:00[UTC]");
//! assert_eq!(next(&mut iter), "2028-01-01T00:00:00+00:00[UTC]");
//! ```
//!
//! If you need to match certain years, please do it externally.
//!
//! ## Why not support passing command to execute?
//!
//! The original purpose of this crate to provide a library to parse and drive the crontab
//! expression to find the next timestamp, while the execution part is scheduled outside.
//!
//! Note that a crontab library scheduling command can be built upon this crate.
//!
//! ## Why not support `?`, `%` and many other non-standard extensions?
//!
//! For `?`, it's a workaround to `*` and the famous cron bug. This crate implements the Vixie's
//! cron behavior, so `?` is not necessary.
//!
//! For `%`, it's coupled with command execution. This crate doesn't support executing so `%` is
//! meaningless.
//!
//! For `#` indicates comments, this crate doesn't support comments. It's too random for a library.

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fmt;
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

mod parser;
pub use parser::FallbackTimezoneOption;
pub use parser::ParseOptions;
pub use parser::normalize_crontab;
pub use parser::parse_crontab;
pub use parser::parse_crontab_with;

pub extern crate jiff;

/// An error that can occur in this crate.
#[derive(Debug, Clone)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

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

#[derive(Debug)]
enum PossibleValue {
    /// Literally match the value.
    ///
    /// For example, a possible literal of minute `15` matches when the minute is 15.
    Literal(u8),
    /// Parsed from `<day>W` in day-of-month field.
    ///
    /// The `W` character is allowed for the day-of-month field. This character is used to specify
    /// the weekday (Monday-Friday) nearest the given day. As an example, if "15W" is specified as
    /// the value for the day-of-month field, the meaning is: "the nearest weekday to the 15th of
    /// the month." So, if the 15th is a Saturday, the trigger fires on Friday the 14th. If the
    /// 15th is a Sunday, the trigger fires on Monday the 16th. If the 15th is a Tuesday, then it
    /// fires on Tuesday the 15th. However, if "1W" is specified as the value for day-of-month, and
    /// the 1st is a Saturday, the trigger fires on Monday the 3rd, as it does not 'jump' over the
    /// boundary of a month's days.
    NearestWeekday(u8),
    /// Parsed from '<day>L' in day-of-month field.
    ///
    /// 'L' stands for "last". When used in the day-of-month field, it specifies the last day of
    /// the month.
    LastDayOfMonth,
    /// Parsed from `<weekday>L` in day-of-week field.
    ///
    /// `L` stands for "last". When used in the day-of-week field, it allows specifying constructs
    /// such as "the last Friday" (`5L`) of a given month.
    LastDayOfWeek(Weekday),
    /// Parsed from `<weekday>#<nth>` in day-of-week field.
    ///
    /// `#` is allowed for the day-of-week field, and must be followed by a number between one and
    /// five. It allows specifying constructs such as "the second Friday" of a given month. For
    /// example, entering `5#3` in the day-of-week field corresponds to the third Friday of every
    /// month.
    NthDayOfWeek(u8, Weekday),
}

/// @see [PossibleValue::Literal]
#[derive(Debug, Clone)]
struct PossibleLiterals {
    values: BTreeSet<u8>,
}

impl PossibleLiterals {
    fn matches(&self, value: u8) -> bool {
        self.values.contains(&value)
    }
}

#[derive(Debug, Clone)]
struct ParsedDaysOfWeek {
    /// @see [PossibleValue::Literal]
    literals: BTreeSet<u8>,
    /// @see [PossibleValue::LastDayOfWeek]
    last_days_of_week: HashSet<Weekday>,
    /// @see [PossibleValue::NthDayOfWeek]
    nth_days_of_week: HashSet<(u8, Weekday)>,

    // to implement Vixie's cron behavior
    // ref - https://crontab.guru/cron-bug.html
    start_with_asterisk: bool,
}

impl ParsedDaysOfWeek {
    fn matches(&self, value: &Zoned) -> bool {
        if self.literals.contains(&(value.weekday() as u8)) {
            return true;
        }

        for weekday in self.last_days_of_week.iter() {
            if value.weekday() != *weekday {
                continue;
            }

            if (value + 1.week()).month() > value.month() {
                return true;
            }
        }

        for (nth, weekday) in self.nth_days_of_week.iter() {
            if value.weekday() != *weekday {
                continue;
            }

            if let Ok(nth_weekday) = value.nth_weekday_of_month(*nth as i8, *weekday)
                && nth_weekday.date() == value.date()
            {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
struct ParsedDaysOfMonth {
    /// @see [PossibleValue::Literal]
    literals: BTreeSet<u8>,
    /// @see [PossibleValue::LastDayOfMonth]
    last_day_of_month: bool,
    /// @see [PossibleValue::NearestWeekday]
    nearest_weekdays: BTreeSet<u8>,

    // to implement Vixie's cron behavior
    // ref - https://crontab.guru/cron-bug.html
    start_with_asterisk: bool,
}

impl ParsedDaysOfMonth {
    fn matches(&self, value: &Zoned) -> bool {
        if self.literals.contains(&(value.day() as u8)) {
            return true;
        }

        if self.last_day_of_month && (value + 1.day()).month() > value.month() {
            return true;
        }

        for day in self.nearest_weekdays.iter() {
            let day = *day as i8;

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

impl FromStr for Crontab {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_crontab(input)
    }
}

impl<'a> TryFrom<&'a str> for Crontab {
    type Error = Error;

    fn try_from(input: &'a str) -> Result<Self, Self::Error> {
        FromStr::from_str(input)
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
            .map_err(error_with_context("failed to parse timestamp"))
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
            .map_err(error_with_context("failed to make timestamp"))
    }

    pub fn from_millisecond(millisecond: i64) -> Result<Self, Error> {
        Timestamp::from_millisecond(millisecond)
            .map(MakeTimestamp)
            .map_err(error_with_context("failed to make timestamp"))
    }

    pub fn from_microsecond(microsecond: i64) -> Result<Self, Error> {
        Timestamp::from_microsecond(microsecond)
            .map(MakeTimestamp)
            .map_err(error_with_context("failed to make timestamp"))
    }

    pub fn from_nanosecond(nanosecond: i128) -> Result<Self, Error> {
        Timestamp::from_nanosecond(nanosecond)
            .map(MakeTimestamp)
            .map_err(error_with_context("failed to make timestamp"))
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
            .map_err(error_with_context("failed to parse start timestamp"))?;

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
            .map_err(error_with_context("failed to parse timestamp"))?;

        // checked at most 4 years to cover the leap year case
        let bound = &zoned + 4.years();

        // at least should be the next minutes
        let mut next = zoned;
        next = advance_time_and_round(next, 1.minute(), Some(Unit::Minute))?;

        loop {
            if next > bound {
                return Err(Error(format!(
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
            .map_err(error_with_context("failed to parse timestamp"))?;

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

    next = next.checked_add(span).map_err(error_with_context(&format!(
        "failed to advance timestamp; end with {next}"
    )))?;

    if let Some(unit) = unit {
        next = next
            .round(ZonedRound::new().mode(RoundMode::Trunc).smallest(unit))
            .map_err(error_with_context(&format!(
                "failed to round timestamp; end with {next}"
            )))?;
    }

    Ok(next)
}

fn error_with_context<E: std::error::Error>(context: &str) -> impl FnOnce(E) -> Error + '_ {
    move |error| Error(format!("{context}: {error}"))
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
