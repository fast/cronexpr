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

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::ops::RangeInclusive;

use jiff::civil::Weekday;
use jiff::fmt::temporal::DateTimeParser;
use winnow::ModalParser;
use winnow::ModalResult;
use winnow::Parser;
use winnow::ascii::dec_uint;
use winnow::combinator::alt;
use winnow::combinator::eof;
use winnow::combinator::fail;
use winnow::combinator::separated;
use winnow::error::ContextError;
use winnow::error::ErrMode;
use winnow::error::FromExternalError;
use winnow::stream::Stream;
use winnow::token::take_while;

use crate::Crontab;
use crate::Error;
use crate::ParsedDaysOfMonth;
use crate::ParsedDaysOfWeek;
use crate::PossibleLiterals;
use crate::PossibleValue;

/// Determine the timezone to fallback when the timezone part is missing.
///
/// See also examples in the [`parse_crontab_with`] documentation.
#[non_exhaustive]
#[derive(Debug, Copy, Clone)]
pub enum FallbackTimezoneOption {
    /// Do not fall back to any timezone. This means the timezone part is required.
    None,
    /// Fall back to [the system timezone](jiff::tz::TimeZone::system).
    System,
    /// Fall back to [`UTC`](jiff::tz::TimeZone::UTC).
    UTC,
}

/// Options to manipulate the parsing manner.
///
/// See also examples in the [`parse_crontab_with`] documentation.
#[non_exhaustive]
#[derive(Debug, Copy, Clone)]
pub struct ParseOptions {
    /// Whether fallback to a certain timezone when the timezone part is missing.
    ///
    /// Default to [`FallbackTimezoneOption::None`].
    pub fallback_timezone_option: FallbackTimezoneOption,

    /// The hashed value to replace `H` in the crontab expression. If [`None`], `H` is not allowed.
    ///
    /// Default to [`None`].
    pub hashed_value: Option<u64>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            fallback_timezone_option: FallbackTimezoneOption::None,
            hashed_value: None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct ParseContext {
    range_fn: fn() -> RangeInclusive<u8>,
    hashed_value: Option<u64>,
}

/// Normalize a crontab expression to compact form.
///
/// ```rust
/// use cronexpr::normalize_crontab;
///
/// assert_eq!(
///     normalize_crontab("  *   * * * * Asia/Shanghai  "),
///     "* * * * * Asia/Shanghai"
/// );
/// assert_eq!(
///     normalize_crontab("  2\t4 * * *\nAsia/Shanghai  "),
///     "2 4 * * * Asia/Shanghai"
/// );
/// ```
pub fn normalize_crontab(input: &str) -> String {
    input
        .split_ascii_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a crontab expression to [`Crontab`]. See [the top-level documentation][crate] for the full
/// syntax definitions.
///
/// ```rust
/// use cronexpr::FallbackTimezoneOption;
/// use cronexpr::ParseOptions;
/// use cronexpr::parse_crontab_with;
///
/// let mut options = ParseOptions::default();
/// parse_crontab_with("* * * * * Asia/Shanghai", options).unwrap();
/// parse_crontab_with("2 4 * * * Asia/Shanghai", options).unwrap();
/// parse_crontab_with("2 4 * * 0-6 Asia/Shanghai", options).unwrap();
/// parse_crontab_with("2 4 */3 * 0-6 Asia/Shanghai", options).unwrap();
///
/// parse_crontab_with("* * * * *", options).unwrap_err();
/// options.fallback_timezone_option = FallbackTimezoneOption::UTC;
/// parse_crontab_with("* * * * *", options).unwrap();
/// options.fallback_timezone_option = FallbackTimezoneOption::System;
/// parse_crontab_with("* * * * *", options).unwrap();
///
/// options.fallback_timezone_option = FallbackTimezoneOption::None;
/// parse_crontab_with("H * * * * UTC", options).unwrap_err();
/// options.hashed_value = Some(42);
/// parse_crontab_with("H * * * * UTC", options).unwrap();
/// ```
pub fn parse_crontab_with(input: &str, options: ParseOptions) -> Result<Crontab, Error> {
    let normalized = normalize_crontab(input);
    if normalized.is_empty() {
        return Err(format_error(&normalized, "", "cannot be empty"));
    }

    fn find_next_part(input: &str, start: usize, next_part: &str) -> Result<usize, Error> {
        if start < input.len() {
            Ok(input[start..]
                .find(' ')
                .map(|end| start + end)
                .unwrap_or(input.len()))
        } else {
            Err(format_incomplete_error(input, next_part))
        }
    }

    let minutes_start = 0;
    let minutes_end = normalized.find(' ').unwrap_or(normalized.len());
    let minutes = parse_minutes(options)
        .parse(&normalized[..minutes_end])
        .map_err(|err| format_parse_error(&normalized, minutes_start, err))?;

    let hours_start = minutes_end + 1;
    let hours_end = find_next_part(&normalized, hours_start, "hours")?;
    let hours = parse_hours(options)
        .parse(&normalized[hours_start..hours_end])
        .map_err(|err| format_parse_error(&normalized, hours_start, err))?;

    let days_of_month_start = hours_end + 1;
    let days_of_month_end = find_next_part(&normalized, days_of_month_start, "days of month")?;
    let days_of_month = parse_days_of_month(options)
        .parse(&normalized[days_of_month_start..days_of_month_end])
        .map_err(|err| format_parse_error(&normalized, days_of_month_start, err))?;

    let months_start = days_of_month_end + 1;
    let months_end = find_next_part(&normalized, months_start, "months")?;
    let months_part = &normalized[months_start..months_end];
    let months = parse_months(options)
        .parse(months_part)
        .map_err(|err| format_parse_error(&normalized, months_start, err))?;

    let days_of_week_start = months_end + 1;
    let days_of_week_end = find_next_part(&normalized, days_of_week_start, "days of week")?;
    let days_of_week = parse_days_of_week(options)
        .parse(&normalized[days_of_week_start..days_of_week_end])
        .map_err(|err| format_parse_error(&normalized, days_of_week_start, err))?;

    let timezone_start = days_of_week_end + 1;
    let timezone = if timezone_start < normalized.len() {
        let timezone_end = normalized.len();
        let timezone_part = &normalized[timezone_start..timezone_end];
        parse_timezone
            .parse(timezone_part)
            .map_err(|err| format_parse_error(&normalized, timezone_start, err))?
    } else {
        match options.fallback_timezone_option {
            FallbackTimezoneOption::System => jiff::tz::TimeZone::system(),
            FallbackTimezoneOption::UTC => jiff::tz::TimeZone::UTC,
            FallbackTimezoneOption::None => {
                return Err(format_incomplete_error(&normalized, "timezone"));
            }
        }
    };

    Ok(Crontab {
        minutes,
        hours,
        days_of_month,
        months,
        days_of_week,
        timezone,
    })
}

/// Parse a crontab expression to [`Crontab`] with the default [`ParseOptions`]. See
/// [the top-level documentation][crate] for the full syntax definitions.
///
/// ```rust
/// use cronexpr::parse_crontab;
///
/// parse_crontab("* * * * * Asia/Shanghai").unwrap();
/// parse_crontab("2 4 * * * Asia/Shanghai").unwrap();
/// parse_crontab("2 4 * * 0-6 Asia/Shanghai").unwrap();
/// parse_crontab("2 4 */3 * 0-6 Asia/Shanghai").unwrap();
/// ```
pub fn parse_crontab(input: &str) -> Result<Crontab, Error> {
    parse_crontab_with(input, ParseOptions::default())
}

fn format_error(input: &str, indent: &str, reason: &str) -> Error {
    let context = "failed to parse crontab expression";
    Error(format!("{context}:\n{input}\n{indent}^ {reason}"))
}

fn format_incomplete_error(input: &str, next_part: &str) -> Error {
    let indent = " ".repeat(input.len());
    format_error(input, &indent, &format!("missing {next_part}"))
}

fn format_parse_error(
    input: &str,
    start: usize,
    parse_error: winnow::error::ParseError<&str, ContextError>,
) -> Error {
    let offset = start + parse_error.offset();
    let indent = " ".repeat(offset);

    let error = parse_error.into_inner().to_string();
    let error = if error.is_empty() {
        "malformed expression"
    } else {
        &error
    };

    format_error(input, &indent, error)
}

fn parse_minutes<'a>(
    options: ParseOptions,
) -> impl ModalParser<&'a str, PossibleLiterals, ContextError> {
    let context = ParseContext {
        range_fn: || 0..=59,
        hashed_value: options.hashed_value,
    };
    move |input: &mut &str| do_parse_number_only(context, input)
}

fn parse_hours<'a>(
    options: ParseOptions,
) -> impl ModalParser<&'a str, PossibleLiterals, ContextError> {
    let context = ParseContext {
        range_fn: || 0..=23,
        hashed_value: options.hashed_value,
    };
    move |input: &mut &str| do_parse_number_only(context, input)
}

fn parse_months<'a>(
    options: ParseOptions,
) -> impl ModalParser<&'a str, PossibleLiterals, ContextError> {
    let context = ParseContext {
        range_fn: || 1..=12,
        hashed_value: options.hashed_value,
    };

    fn parse_single_month<'a>(
        context: ParseContext,
    ) -> impl ModalParser<&'a str, u8, ContextError> {
        alt((
            alt((
                "JAN".map(|_| 1),
                "FEB".map(|_| 2),
                "MAR".map(|_| 3),
                "APR".map(|_| 4),
                "MAY".map(|_| 5),
                "JUN".map(|_| 6),
            )),
            alt((
                "JUL".map(|_| 7),
                "AUG".map(|_| 8),
                "SEP".map(|_| 9),
                "OCT".map(|_| 10),
                "NOV".map(|_| 11),
                "DEC".map(|_| 12),
            )),
            parse_single_number(context),
        ))
    }

    move |input: &mut &str| {
        let values = parse_list(alt((
            parse_step(context, parse_single_month).map(|r| {
                r.into_iter()
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
            parse_range(context, parse_single_month).map(|r| {
                r.into_iter()
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
            parse_single_month(context).map(|n| vec![PossibleValue::Literal(n)]),
            parse_hashed_value(context).map(|n| vec![PossibleValue::Literal(n)]),
            parse_asterisk(context).map(|r| {
                r.into_iter()
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
        )))
        .parse_next(input)?;

        let mut literals = BTreeSet::new();
        for value in values {
            match value {
                PossibleValue::Literal(value) => {
                    literals.insert(value);
                }
                _ => unreachable!("unexpected value: {value:?}"),
            }
        }
        Ok(PossibleLiterals { values: literals })
    }
}

fn parse_days_of_week<'a>(
    options: ParseOptions,
) -> impl ModalParser<&'a str, ParsedDaysOfWeek, ContextError> {
    let context = ParseContext {
        range_fn: || 0..=7,
        hashed_value: options.hashed_value,
    };

    fn norm_sunday(n: u8) -> u8 {
        if n != 0 { n } else { 7 }
    }

    fn make_weekday(n: u8) -> Weekday {
        let weekday = norm_sunday(n) as i8;
        Weekday::from_monday_one_offset(weekday)
            .unwrap_or_else(|err| panic!("{weekday} must be in range 1..=7: {err:?}"))
    }

    fn parse_single_day_of_week<'a>(
        context: ParseContext,
    ) -> impl ModalParser<&'a str, u8, ContextError> {
        alt((
            "SUN".map(|_| 0),
            "MON".map(|_| 1),
            "TUE".map(|_| 2),
            "WED".map(|_| 3),
            "THU".map(|_| 4),
            "FRI".map(|_| 5),
            "SAT".map(|_| 6),
            parse_single_number(context),
        ))
    }

    fn parse_single_day_of_week_ext<'a>(
        context: ParseContext,
    ) -> impl ModalParser<&'a str, PossibleValue, ContextError> {
        alt((
            (parse_single_day_of_week(context), "L")
                .map(|(n, _)| PossibleValue::LastDayOfWeek(make_weekday(n))),
            (
                parse_single_day_of_week(context),
                "#",
                parse_single_number(ParseContext {
                    range_fn: || 1..=5,
                    hashed_value: None,
                }),
            )
                .map(|(n, _, nth)| PossibleValue::NthDayOfWeek(nth, make_weekday(n))),
            parse_single_day_of_week(context).map(|n| PossibleValue::Literal(norm_sunday(n))),
            parse_hashed_value(context).map(|n| PossibleValue::Literal(norm_sunday(n))),
        ))
    }

    move |input: &mut &str| {
        let start_with_asterisk = input.starts_with('*');

        let values = parse_list(alt((
            parse_step(context, parse_single_day_of_week).map(|r| {
                r.into_iter()
                    .map(norm_sunday)
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
            parse_range(context, parse_single_day_of_week).map(|r| {
                r.into_iter()
                    .map(norm_sunday)
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
            parse_single_day_of_week_ext(context).map(|n| vec![n]),
            parse_asterisk(context).map(|r| {
                r.into_iter()
                    .map(norm_sunday)
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
        )))
        .parse_next(input)?;

        let mut literals = BTreeSet::new();
        let mut last_days_of_week = HashSet::new();
        let mut nth_days_of_week = HashSet::new();
        for value in values {
            match value {
                PossibleValue::Literal(value) => {
                    literals.insert(value);
                }
                PossibleValue::LastDayOfWeek(weekday) => {
                    last_days_of_week.insert(weekday);
                }
                PossibleValue::NthDayOfWeek(nth, weekday) => {
                    nth_days_of_week.insert((nth, weekday));
                }
                _ => unreachable!("unexpected value: {value:?}"),
            }
        }
        Ok(ParsedDaysOfWeek {
            literals,
            last_days_of_week,
            nth_days_of_week,
            start_with_asterisk,
        })
    }
}

fn parse_days_of_month<'a>(
    options: ParseOptions,
) -> impl ModalParser<&'a str, ParsedDaysOfMonth, ContextError> {
    let context = ParseContext {
        range_fn: || 1..=31,
        hashed_value: options.hashed_value,
    };

    fn parse_single_day_of_month_ext<'a>(
        context: ParseContext,
    ) -> impl ModalParser<&'a str, PossibleValue, ContextError> {
        alt((
            (parse_single_number(context), "W").map(|(n, _)| PossibleValue::NearestWeekday(n)),
            parse_single_number(context).map(PossibleValue::Literal),
            "L".map(|_| PossibleValue::LastDayOfMonth),
            parse_hashed_value(context).map(PossibleValue::Literal),
        ))
    }

    move |input: &mut &str| {
        let start_with_asterisk = input.starts_with('*');

        let values = parse_list(alt((
            parse_step(context, parse_single_number).map(|r| {
                r.into_iter()
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
            parse_range(context, parse_single_number).map(|r| {
                r.into_iter()
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
            parse_single_day_of_month_ext(context).map(|n| vec![n]),
            parse_asterisk(context).map(|r| {
                r.into_iter()
                    .map(PossibleValue::Literal)
                    .collect::<Vec<_>>()
            }),
        )))
        .parse_next(input)?;

        let mut literals = BTreeSet::new();
        let mut last_day_of_month = false;
        let mut nearest_weekdays = BTreeSet::new();
        for value in values {
            match value {
                PossibleValue::Literal(value) => {
                    literals.insert(value);
                }
                PossibleValue::LastDayOfMonth => {
                    last_day_of_month = true;
                }
                PossibleValue::NearestWeekday(day) => {
                    nearest_weekdays.insert(day);
                }
                _ => unreachable!("unexpected value: {value:?}"),
            }
        }
        Ok(ParsedDaysOfMonth {
            literals,
            last_day_of_month,
            nearest_weekdays,
            start_with_asterisk,
        })
    }
}

fn parse_timezone(input: &mut &str) -> ModalResult<jiff::tz::TimeZone> {
    take_while(0.., |_| true)
        .try_map_cut(|timezone| {
            static PARSER: DateTimeParser = DateTimeParser::new();
            PARSER.parse_time_zone(timezone).map_err(|_| {
                Error(format!(
                    "failed to find timezone {timezone}; \
                for a list of time zones, see the list of tz database time zones on Wikipedia: \
                https://en.wikipedia.org/wiki/List_of_tz_database_time_zones#List"
                ))
            })
        })
        .parse_next(input)
}

// number only = minutes, hours, or months
fn do_parse_number_only(context: ParseContext, input: &mut &str) -> ModalResult<PossibleLiterals> {
    let values = parse_list(alt((
        parse_step(context, parse_single_number).map(|r| {
            r.into_iter()
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
        parse_range(context, parse_single_number).map(|r| {
            r.into_iter()
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
        parse_single_number(context).map(|n| vec![PossibleValue::Literal(n)]),
        parse_hashed_value(context).map(|n| vec![PossibleValue::Literal(n)]),
        parse_asterisk(context).map(|r| {
            r.into_iter()
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
    )))
    .parse_next(input)?;

    let mut literals = BTreeSet::new();
    for value in values {
        match value {
            PossibleValue::Literal(value) => {
                literals.insert(value);
            }
            _ => unreachable!("unexpected value: {value:?}"),
        }
    }
    Ok(PossibleLiterals { values: literals })
}

fn parse_hashed_value<'a>(context: ParseContext) -> impl ModalParser<&'a str, u8, ContextError> {
    move |input: &mut &str| {
        if let Some(hashed_value) = context.hashed_value {
            let range = (context.range_fn)();
            let hashed_value = map_hash_into_range(hashed_value, range);
            "H".map(move |_| hashed_value).parse_next(input)
        } else {
            fail(input)
        }
    }
}

fn parse_asterisk<'a>(context: ParseContext) -> impl ModalParser<&'a str, Vec<u8>, ContextError> {
    let range = context.range_fn;
    "*".map(move |_| range().collect())
}

fn parse_single_number<'a>(context: ParseContext) -> impl ModalParser<&'a str, u8, ContextError> {
    let range = context.range_fn;
    dec_uint.try_map_cut(move |n: u64| {
        let range = range();

        if n > u8::MAX as u64 {
            return Err(Error(format!(
                "value must be in range {range:?}; found {n}"
            )));
        }

        let n = n as u8;
        if range.contains(&n) {
            Ok(n)
        } else {
            Err(Error(format!(
                "value must be in range {range:?}; found {n}"
            )))
        }
    })
}

fn parse_range<'a, P>(
    context: ParseContext,
    parse_single_range_bound: fn(context: ParseContext) -> P,
) -> impl ModalParser<&'a str, Vec<u8>, ContextError>
where
    P: ModalParser<&'a str, u8, ContextError>,
{
    let range = context.range_fn;
    (
        parse_single_range_bound(context),
        "-",
        parse_single_range_bound(context),
    )
        .try_map_cut(move |(lo, _, hi): (u8, _, u8)| {
            let range = range();

            if lo > hi {
                return Err(Error(format!(
                    "range must be in ascending order; found {lo}-{hi}"
                )));
            }

            if range.contains(&lo) && range.contains(&hi) {
                Ok((lo..=hi).collect())
            } else {
                Err(Error(format!(
                    "range must be in range {range:?}; found {lo}-{hi}"
                )))
            }
        })
}

fn parse_step<'a, P>(
    context: ParseContext,
    parse_single_range_bound: fn(context: ParseContext) -> P,
) -> impl ModalParser<&'a str, Vec<u8>, ContextError>
where
    P: ModalParser<&'a str, u8, ContextError>,
{
    let range = context.range_fn;
    let range_end = *range().end();

    let possible_values = alt((
        parse_asterisk(context),
        parse_range(context, parse_single_range_bound),
        parse_single_range_bound(context).map(move |n| (n..=range_end).collect()),
    ));

    (possible_values, "/", dec_uint).try_map_cut(move |(candidates, _, step): (Vec<u8>, _, u64)| {
        let range = range();

        if step == 0 {
            return Err(Error("step must be greater than 0".to_string()));
        }

        if step > u8::MAX as u64 {
            return Err(Error(format!(
                "step must be in range {range:?}; found {step}"
            )));
        }

        let step = step as u8;
        if !range.contains(&step) {
            return Err(Error(format!(
                "step must be in range {range:?}; found {step}"
            )));
        }

        let mut values = Vec::new();
        for n in candidates.into_iter().step_by(step as usize) {
            values.push(n);
        }
        Ok(values)
    })
}

fn parse_list<'a, P>(
    parse_list_item: P,
) -> impl ModalParser<&'a str, Vec<PossibleValue>, ContextError>
where
    P: ModalParser<&'a str, Vec<PossibleValue>, ContextError>,
{
    (separated(1.., parse_list_item, ","), eof)
        .map(move |(ns, _): (Vec<Vec<PossibleValue>>, _)| ns.into_iter().flatten().collect())
}

fn map_hash_into_range(hashed_value: u64, range: RangeInclusive<u8>) -> u8 {
    let modulo = range.end() - range.start() + 1;
    let hashed_value = hashed_value % modulo as u64;
    (range.start() + hashed_value as u8).min(*range.end())
}

trait ModalParserExt<I, O, E>: ModalParser<I, O, E> {
    #[inline(always)]
    fn try_map_cut<G, O2, E2>(self, map: G) -> TryMapCut<Self, G, I, O, O2, E, E2>
    where
        Self: Sized,
        G: FnMut(O) -> Result<O2, E2>,
        I: Stream,
        E: FromExternalError<I, E2>,
    {
        TryMapCut {
            parser: self,
            map,
            i: Default::default(),
            o: Default::default(),
            o2: Default::default(),
            e: Default::default(),
            e2: Default::default(),
        }
    }
}

struct TryMapCut<F, G, I, O, O2, E, E2>
where
    F: ModalParser<I, O, E>,
    G: FnMut(O) -> Result<O2, E2>,
    I: Stream,
    E: FromExternalError<I, E2>,
{
    parser: F,
    map: G,
    i: core::marker::PhantomData<I>,
    o: core::marker::PhantomData<O>,
    o2: core::marker::PhantomData<O2>,
    e: core::marker::PhantomData<E>,
    e2: core::marker::PhantomData<E2>,
}

impl<F, G, I, O, O2, E, E2> Parser<I, O2, ErrMode<E>> for TryMapCut<F, G, I, O, O2, E, E2>
where
    F: ModalParser<I, O, E>,
    G: FnMut(O) -> Result<O2, E2>,
    I: Stream,
    E: FromExternalError<I, E2>,
{
    #[inline]
    fn parse_next(&mut self, input: &mut I) -> ModalResult<O2, E> {
        let start = input.checkpoint();
        let o = self.parser.parse_next(input)?;

        (self.map)(o).map_err(|err| {
            input.reset(&start);
            ErrMode::from_external_error(input, err).cut()
        })
    }
}

impl<I, O, E, P> ModalParserExt<I, O, E> for P where P: ModalParser<I, O, E> {}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn test_parse_crontab_success() {
        // snapshot files are ordered; for new cases, please add to the end
        assert_debug_snapshot!(parse_crontab("* * * * * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("2 4 * * * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("2 4 * * 0-6 Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("2 4 */3 * 0-6 Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("*/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-29/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-30/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1,2,10 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-10,2,10,50 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-10,2,10,50 1 * 1 TUE Asia/Shanghai").unwrap());

        // optional timezone
        let options = ParseOptions {
            fallback_timezone_option: FallbackTimezoneOption::UTC,
            ..Default::default()
        };
        assert_debug_snapshot!(parse_crontab_with("0 0 1 1 5", options).unwrap());
        assert_debug_snapshot!(parse_crontab_with("0 0 1 1 5 ", options).unwrap());

        let options = ParseOptions {
            fallback_timezone_option: FallbackTimezoneOption::System,
            ..Default::default()
        };
        insta::with_settings!({
            filters => vec![(r"TZif\(\n.*\n.*\)", "[SYSTEM]")]
        }, {
            assert_debug_snapshot!(parse_crontab_with("0 0 1 1 5", options).unwrap());
            assert_debug_snapshot!(parse_crontab_with("0 0 1 1 5 ", options).unwrap());
        });

        // hashed value
        let options = ParseOptions {
            hashed_value: Some(42),
            ..Default::default()
        };
        assert_debug_snapshot!(parse_crontab_with("H * * * * America/Denver", options).unwrap());
        assert_debug_snapshot!(parse_crontab_with("H H H H H America/Denver", options).unwrap());

        assert_debug_snapshot!(parse_crontab("0 0 1 1 5 +08:00").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 1 5 +00:00").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 1 5 -08:00").unwrap());
    }

    #[test]
    fn test_parse_crontab_failed() {
        // snapshot files are ordered; for new cases, please add to the end
        assert_snapshot!(parse_crontab("invalid 4 * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("* * * * * Unknown/Timezone").unwrap_err());
        assert_snapshot!(parse_crontab("* 5-4 * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("10086 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("* 0-24 * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("* * * 25 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("32-300 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("129-300 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("29- * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("29 ** * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("29--30 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("1,2,10,100 1 1 1 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("104,2,10,100 1 1 1 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("1,2,10 * * 104,2,10,100 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("1-10,2,10,50 1 * 1 TTT Asia/Shanghai").unwrap_err());

        // check incomplete and edge right; all input are first normalized so no need for extra
        // spaces
        assert_snapshot!(parse_crontab("0").unwrap_err());
        assert_snapshot!(parse_crontab("0 0").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 1").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 1 5").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 1 5 ").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 1 5 Z").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 1 5 Z Z").unwrap_err());
        assert_snapshot!(parse_crontab("").unwrap_err());

        // hashed value
        assert_snapshot!(parse_crontab("H * * * * UTC").unwrap_err());

        assert_snapshot!(parse_crontab("0 0 1 1 5 +26:00").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 1 5 +Ch:Ch").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 1 5 -08:75").unwrap_err());
    }

    #[test]
    fn test_crontab_guru_examples() {
        // crontab.guru examples: https://crontab.guru/examples.html
        assert_debug_snapshot!(parse_crontab("* * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/2 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("1-59/2 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/3 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/4 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/5 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/6 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/10 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/15 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/20 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/30 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("30 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */2 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */3 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */4 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */6 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */8 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */12 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 9-17 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 1 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 2 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 8 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 9 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 0 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 1 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 2 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 3 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 4 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 5 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 6 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 1-5 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 6,0 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 */2 * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 */3 * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 */6 * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 1 * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 9 * * 1-5 +08:00").unwrap());
        assert_debug_snapshot!(parse_crontab("*/15 9-17 * * * +09:00").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */6 * * * -03:00").unwrap());
    }
}
