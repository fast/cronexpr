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

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::RangeInclusive;

use jiff::civil::Weekday;
use jiff::fmt::temporal::DateTimeParser;

use crate::Crontab;
use crate::Error;
use crate::ParsedDaysOfMonth;
use crate::ParsedDaysOfWeek;
use crate::PossibleLiterals;
use crate::PossibleValue;
use crate::literal_set::LiteralSet;

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
    min: u8,
    max: u8,
    hashed_value: Option<u64>,
    normalization: LiteralNormalization,
}

impl ParseContext {
    fn range(self) -> RangeInclusive<u8> {
        self.min..=self.max
    }

    fn normalize(self, value: u8) -> u8 {
        self.normalization.apply(value)
    }

    fn insert_range(self, literals: &mut LiteralSet, range: RangeInclusive<u8>) {
        self.normalization.insert_range(literals, range);
    }
}

#[derive(Debug)]
struct ParseFailure {
    offset: usize,
    reason: Option<String>,
}

impl ParseFailure {
    fn malformed(offset: usize) -> Self {
        ParseFailure {
            offset,
            reason: None,
        }
    }

    fn custom(offset: usize, reason: impl Into<String>) -> Self {
        ParseFailure {
            offset,
            reason: Some(reason.into()),
        }
    }
}

type ParseResult<T> = Result<T, ParseFailure>;

#[derive(Debug, Copy, Clone)]
enum LiteralKind {
    Number,
    Month,
    DayOfWeek,
}

#[derive(Debug, Copy, Clone)]
enum LiteralNormalization {
    Identity,
    Sunday,
}

impl LiteralNormalization {
    fn apply(self, value: u8) -> u8 {
        match self {
            LiteralNormalization::Identity => value,
            LiteralNormalization::Sunday if value == 0 => 7,
            LiteralNormalization::Sunday => value,
        }
    }

    fn insert_range(self, literals: &mut LiteralSet, range: RangeInclusive<u8>) {
        let (start, end) = range.into_inner();
        match self {
            LiteralNormalization::Identity => literals.insert_range(start..=end),
            LiteralNormalization::Sunday if start == 0 => {
                literals.insert(7);
                if end > 0 {
                    literals.insert_range(1..=end);
                }
            }
            LiteralNormalization::Sunday => literals.insert_range(start..=end),
        }
    }
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
    match parse_normalized_crontab(input, options) {
        Ok(crontab) => Ok(crontab),
        Err(error) => match normalized_error_input(input) {
            Cow::Borrowed(_) => Err(error),
            Cow::Owned(normalized) => parse_normalized_crontab(&normalized, options),
        },
    }
}

fn parse_normalized_crontab(input: &str, options: ParseOptions) -> Result<Crontab, Error> {
    if input.is_empty() {
        return Err(format_error(input, "", "cannot be empty"));
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
    let minutes_end = input.find(' ').unwrap_or(input.len());
    let minutes = parse_minutes(&input[..minutes_end], options)
        .map_err(|err| format_parse_error(input, minutes_start, err))?;

    let hours_start = minutes_end + 1;
    let hours_end = find_next_part(input, hours_start, "hours")?;
    let hours = parse_hours(&input[hours_start..hours_end], options)
        .map_err(|err| format_parse_error(input, hours_start, err))?;

    let days_of_month_start = hours_end + 1;
    let days_of_month_end = find_next_part(input, days_of_month_start, "days of month")?;
    let days_of_month =
        parse_days_of_month(&input[days_of_month_start..days_of_month_end], options)
            .map_err(|err| format_parse_error(input, days_of_month_start, err))?;

    let months_start = days_of_month_end + 1;
    let months_end = find_next_part(input, months_start, "months")?;
    let months = parse_months(&input[months_start..months_end], options)
        .map_err(|err| format_parse_error(input, months_start, err))?;

    let days_of_week_start = months_end + 1;
    let days_of_week_end = find_next_part(input, days_of_week_start, "days of week")?;
    let days_of_week = parse_days_of_week(&input[days_of_week_start..days_of_week_end], options)
        .map_err(|err| format_parse_error(input, days_of_week_start, err))?;

    let timezone_start = days_of_week_end + 1;
    let timezone = if timezone_start < input.len() {
        parse_timezone(&input[timezone_start..])
            .map_err(|err| format_parse_error(input, timezone_start, err))?
    } else {
        match options.fallback_timezone_option {
            FallbackTimezoneOption::System => jiff::tz::TimeZone::system(),
            FallbackTimezoneOption::UTC => jiff::tz::TimeZone::UTC,
            FallbackTimezoneOption::None => {
                return Err(format_incomplete_error(input, "timezone"));
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

fn format_parse_error(input: &str, start: usize, parse_error: ParseFailure) -> Error {
    let offset = start + parse_error.offset;
    let indent = " ".repeat(offset);

    let error = parse_error
        .reason
        .as_deref()
        .unwrap_or("malformed expression");

    format_error(input, &indent, error)
}

fn normalized_error_input(input: &str) -> Cow<'_, str> {
    let mut previous_was_space = true;
    for byte in input.bytes() {
        if byte.is_ascii_whitespace() {
            if byte != b' ' || previous_was_space {
                return Cow::Owned(normalize_crontab(input));
            }
            previous_was_space = true;
        } else {
            previous_was_space = false;
        }
    }

    if previous_was_space {
        Cow::Owned(normalize_crontab(input))
    } else {
        Cow::Borrowed(input)
    }
}

fn parse_minutes(input: &str, options: ParseOptions) -> ParseResult<PossibleLiterals> {
    let context = ParseContext {
        min: 0,
        max: 59,
        hashed_value: options.hashed_value,
        normalization: LiteralNormalization::Identity,
    };
    parse_literal_field(input, context, LiteralKind::Number)
}

fn parse_hours(input: &str, options: ParseOptions) -> ParseResult<PossibleLiterals> {
    let context = ParseContext {
        min: 0,
        max: 23,
        hashed_value: options.hashed_value,
        normalization: LiteralNormalization::Identity,
    };
    parse_literal_field(input, context, LiteralKind::Number)
}

fn parse_months(input: &str, options: ParseOptions) -> ParseResult<PossibleLiterals> {
    let context = ParseContext {
        min: 1,
        max: 12,
        hashed_value: options.hashed_value,
        normalization: LiteralNormalization::Identity,
    };
    parse_literal_field(input, context, LiteralKind::Month)
}

fn parse_days_of_week(input: &str, options: ParseOptions) -> ParseResult<ParsedDaysOfWeek> {
    let context = ParseContext {
        min: 0,
        max: 7,
        hashed_value: options.hashed_value,
        normalization: LiteralNormalization::Sunday,
    };
    let start_with_asterisk = input.starts_with('*');
    let mut literals = LiteralSet::default();
    let mut last_days_of_week = HashSet::new();
    let mut nth_days_of_week = HashSet::new();
    parse_list(input, |item, offset| {
        parse_day_of_week_item(
            item,
            offset,
            context,
            &mut literals,
            &mut |value| match value {
                PossibleValue::LastDayOfWeek(weekday) => {
                    last_days_of_week.insert(weekday);
                }
                PossibleValue::NthDayOfWeek(nth, weekday) => {
                    nth_days_of_week.insert((nth, weekday));
                }
                _ => unreachable!("unexpected value: {value:?}"),
            },
        )
    })?;
    Ok(ParsedDaysOfWeek {
        literals,
        last_days_of_week,
        nth_days_of_week,
        start_with_asterisk,
    })
}

fn parse_days_of_month(input: &str, options: ParseOptions) -> ParseResult<ParsedDaysOfMonth> {
    let context = ParseContext {
        min: 1,
        max: 31,
        hashed_value: options.hashed_value,
        normalization: LiteralNormalization::Identity,
    };
    let start_with_asterisk = input.starts_with('*');
    let mut literals = LiteralSet::default();
    let mut last_day_of_month = false;
    let mut nearest_weekdays = LiteralSet::default();
    parse_list(input, |item, offset| {
        parse_day_of_month_item(
            item,
            offset,
            context,
            &mut literals,
            &mut |value| match value {
                PossibleValue::LastDayOfMonth => last_day_of_month = true,
                PossibleValue::NearestWeekday(day) => nearest_weekdays.insert(day),
                _ => unreachable!("unexpected value: {value:?}"),
            },
        )
    })?;
    Ok(ParsedDaysOfMonth {
        literals,
        last_day_of_month,
        nearest_weekdays,
        start_with_asterisk,
    })
}

fn parse_timezone(timezone: &str) -> ParseResult<jiff::tz::TimeZone> {
    static PARSER: DateTimeParser = DateTimeParser::new();
    PARSER.parse_time_zone(timezone).map_err(|_| {
        ParseFailure::custom(
            0,
            format!(
                "failed to find timezone {timezone}; \
                for a list of time zones, see the list of tz database time zones on Wikipedia: \
                https://en.wikipedia.org/wiki/List_of_tz_database_time_zones#List"
            ),
        )
    })
}

fn parse_literal_field(
    input: &str,
    context: ParseContext,
    kind: LiteralKind,
) -> ParseResult<PossibleLiterals> {
    let mut literals = LiteralSet::default();
    parse_list(input, |item, offset| {
        parse_literal_item(item, offset, context, kind, &mut literals)
    })?;
    Ok(PossibleLiterals { values: literals })
}

fn parse_list<F>(input: &str, mut parse_item: F) -> ParseResult<()>
where
    F: FnMut(&str, usize) -> ParseResult<()>,
{
    if input.is_empty() {
        return Err(ParseFailure::malformed(0));
    }
    if !input.contains(',') {
        return parse_item(input, 0);
    }

    let mut item_start = 0;
    loop {
        let item_end = input[item_start..]
            .find(',')
            .map(|offset| item_start + offset)
            .unwrap_or(input.len());
        if item_start == item_end {
            return Err(ParseFailure::malformed(item_start.saturating_sub(1)));
        }

        match parse_item(&input[item_start..item_end], item_start) {
            Ok(()) => {}
            Err(error)
                if item_start > 0 && error.offset == item_start && error.reason.is_none() =>
            {
                return Err(ParseFailure::malformed(item_start - 1));
            }
            Err(error) => return Err(error),
        }
        if item_end == input.len() {
            break;
        }
        if item_end + 1 == input.len() {
            return Err(ParseFailure::malformed(item_end));
        }
        item_start = item_end + 1;
    }
    Ok(())
}

fn parse_literal_item(
    input: &str,
    offset: usize,
    context: ParseContext,
    kind: LiteralKind,
    literals: &mut LiteralSet,
) -> ParseResult<()> {
    match input.as_bytes().first() {
        Some(b'*') => return parse_asterisk_item(input, offset, context, literals),
        Some(b'H') => return parse_hashed_item(input, offset, context, literals),
        _ => {}
    }

    let (lo, end) = parse_literal(input, offset, context, kind)?;
    if end == input.len() {
        literals.insert(context.normalize(lo));
        return Ok(());
    }

    match input.as_bytes()[end] {
        b'-' => parse_range_item(input, offset, context, kind, (lo, end), literals),
        b'/' => parse_step_item(input, offset, context, end, lo..=context.max, literals),
        _ => Err(ParseFailure::malformed(offset + end)),
    }
}

fn parse_day_of_month_item(
    input: &str,
    offset: usize,
    context: ParseContext,
    literals: &mut LiteralSet,
    emit: &mut impl FnMut(PossibleValue),
) -> ParseResult<()> {
    match input.as_bytes().first() {
        Some(b'*') => {
            return parse_asterisk_item(input, offset, context, literals);
        }
        Some(b'H') => {
            return parse_hashed_item(input, offset, context, literals);
        }
        Some(b'L') => {
            return if input.len() == 1 {
                emit(PossibleValue::LastDayOfMonth);
                Ok(())
            } else {
                Err(ParseFailure::malformed(offset + 1))
            };
        }
        _ => {}
    }

    let (day, end) = parse_literal(input, offset, context, LiteralKind::Number)?;
    if end == input.len() {
        literals.insert(day);
        return Ok(());
    }

    match input.as_bytes()[end] {
        b'-' => parse_range_item(
            input,
            offset,
            context,
            LiteralKind::Number,
            (day, end),
            literals,
        ),
        b'/' => parse_step_item(input, offset, context, end, day..=context.max, literals),
        b'W' if end + 1 == input.len() => {
            emit(PossibleValue::NearestWeekday(day));
            Ok(())
        }
        b'W' => Err(ParseFailure::malformed(offset + end + 1)),
        _ => Err(ParseFailure::malformed(offset + end)),
    }
}

fn parse_day_of_week_item(
    input: &str,
    offset: usize,
    context: ParseContext,
    literals: &mut LiteralSet,
    emit: &mut impl FnMut(PossibleValue),
) -> ParseResult<()> {
    match input.as_bytes().first() {
        Some(b'*') => {
            return parse_asterisk_item(input, offset, context, literals);
        }
        Some(b'H') => {
            return parse_hashed_item(input, offset, context, literals);
        }
        _ => {}
    }

    let (day, end) = parse_literal(input, offset, context, LiteralKind::DayOfWeek)?;
    if end == input.len() {
        literals.insert(context.normalize(day));
        return Ok(());
    }

    match input.as_bytes()[end] {
        b'-' => parse_range_item(
            input,
            offset,
            context,
            LiteralKind::DayOfWeek,
            (day, end),
            literals,
        ),
        b'/' => parse_step_item(input, offset, context, end, day..=context.max, literals),
        b'L' if end + 1 == input.len() => {
            emit(PossibleValue::LastDayOfWeek(make_weekday(day)));
            Ok(())
        }
        b'L' => Err(ParseFailure::malformed(offset + end + 1)),
        b'#' => {
            let nth_context = ParseContext {
                min: 1,
                max: 5,
                hashed_value: None,
                normalization: LiteralNormalization::Identity,
            };
            let nth_start = end + 1;
            let (nth, nth_len) = match parse_literal(
                &input[nth_start..],
                offset + nth_start,
                nth_context,
                LiteralKind::Number,
            ) {
                Ok(value) => value,
                Err(error) if error.reason.is_none() => {
                    return Err(ParseFailure::malformed(offset + end));
                }
                Err(error) => return Err(error),
            };
            let nth_end = nth_start + nth_len;
            if nth_end != input.len() {
                return Err(ParseFailure::malformed(offset + nth_end));
            }
            emit(PossibleValue::NthDayOfWeek(nth, make_weekday(day)));
            Ok(())
        }
        _ => Err(ParseFailure::malformed(offset + end)),
    }
}

fn parse_asterisk_item(
    input: &str,
    offset: usize,
    context: ParseContext,
    literals: &mut LiteralSet,
) -> ParseResult<()> {
    if input.len() == 1 {
        context.insert_range(literals, context.range());
        return Ok(());
    }
    if input.as_bytes()[1] == b'/' {
        return parse_step_item(input, offset, context, 1, context.range(), literals);
    }
    Err(ParseFailure::malformed(offset + 1))
}

fn parse_hashed_item(
    input: &str,
    offset: usize,
    context: ParseContext,
    literals: &mut LiteralSet,
) -> ParseResult<()> {
    let Some(hashed_value) = context.hashed_value else {
        return Err(ParseFailure::malformed(offset));
    };
    if input.len() != 1 {
        return Err(ParseFailure::malformed(offset + 1));
    }
    let value = map_hash_into_range(hashed_value, context.range());
    literals.insert(context.normalize(value));
    Ok(())
}

fn parse_range_item(
    input: &str,
    offset: usize,
    context: ParseContext,
    kind: LiteralKind,
    start: (u8, usize),
    literals: &mut LiteralSet,
) -> ParseResult<()> {
    let (lo, dash) = start;
    let (hi, hi_len) = match parse_literal(&input[dash + 1..], offset + dash + 1, context, kind) {
        Ok(value) => value,
        Err(error) if error.reason.is_none() => {
            return Err(ParseFailure::malformed(offset + dash));
        }
        Err(error) => return Err(error),
    };

    if lo > hi {
        return Err(ParseFailure::custom(
            offset,
            format!("range must be in ascending order; found {lo}-{hi}"),
        ));
    }

    let end = dash + 1 + hi_len;
    if end == input.len() {
        context.insert_range(literals, lo..=hi);
        return Ok(());
    }
    if input.as_bytes()[end] == b'/' {
        return parse_step_item(input, offset, context, end, lo..=hi, literals);
    }
    Err(ParseFailure::malformed(offset + end))
}

fn parse_step_item(
    input: &str,
    offset: usize,
    context: ParseContext,
    slash: usize,
    candidates: RangeInclusive<u8>,
    literals: &mut LiteralSet,
) -> ParseResult<()> {
    let step_start = slash + 1;
    let (step, step_len) = match parse_decimal(&input[step_start..], offset + step_start) {
        Ok(value) => value,
        Err(_) => return Err(ParseFailure::malformed(offset + slash)),
    };

    if step == 0 {
        return Err(ParseFailure::custom(offset, "step must be greater than 0"));
    }
    if step > u8::MAX as u64 || !context.range().contains(&(step as u8)) {
        return Err(ParseFailure::custom(
            offset,
            format!("step must be in range {:?}; found {step}", context.range()),
        ));
    }

    let end = step_start + step_len;
    if end != input.len() {
        return Err(ParseFailure::malformed(offset + end));
    }
    insert_literals(literals, candidates.step_by(step as usize), context);
    Ok(())
}

fn parse_literal(
    input: &str,
    offset: usize,
    context: ParseContext,
    kind: LiteralKind,
) -> ParseResult<(u8, usize)> {
    let named = match kind {
        LiteralKind::Number => None,
        LiteralKind::Month => parse_named_literal(input, &MONTH_NAMES),
        LiteralKind::DayOfWeek => parse_named_literal(input, &WEEKDAY_NAMES),
    };
    if let Some((value, len)) = named {
        return Ok((value, len));
    }

    let (value, len) = parse_decimal(input, offset)?;
    if value > u8::MAX as u64 || !context.range().contains(&(value as u8)) {
        return Err(ParseFailure::custom(
            offset,
            format!(
                "value must be in range {:?}; found {value}",
                context.range()
            ),
        ));
    }
    Ok((value as u8, len))
}

fn parse_decimal(input: &str, offset: usize) -> ParseResult<(u64, usize)> {
    let len = input
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if len == 0 {
        return Err(ParseFailure::malformed(offset));
    }

    let mut value = 0_u64;
    for byte in &input.as_bytes()[..len] {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(byte - b'0')))
            .ok_or_else(|| ParseFailure::malformed(offset))?;
    }
    Ok((value, len))
}

fn parse_named_literal(input: &str, names: &[(&[u8; 3], u8)]) -> Option<(u8, usize)> {
    let prefix = input.as_bytes().get(..3)?;
    names
        .iter()
        .find(|(name, _)| prefix.eq_ignore_ascii_case(name.as_slice()))
        .map(|(_, value)| (*value, 3))
}

fn insert_literals(
    literals: &mut LiteralSet,
    values: impl IntoIterator<Item = u8>,
    context: ParseContext,
) {
    for value in values {
        literals.insert(context.normalize(value));
    }
}

fn make_weekday(value: u8) -> Weekday {
    let weekday = if value == 0 { 7 } else { value } as i8;
    Weekday::from_monday_one_offset(weekday)
        .unwrap_or_else(|err| panic!("{weekday} must be in range 1..=7: {err:?}"))
}

const MONTH_NAMES: [(&[u8; 3], u8); 12] = [
    (b"JAN", 1),
    (b"FEB", 2),
    (b"MAR", 3),
    (b"APR", 4),
    (b"MAY", 5),
    (b"JUN", 6),
    (b"JUL", 7),
    (b"AUG", 8),
    (b"SEP", 9),
    (b"OCT", 10),
    (b"NOV", 11),
    (b"DEC", 12),
];

const WEEKDAY_NAMES: [(&[u8; 3], u8); 7] = [
    (b"SUN", 0),
    (b"MON", 1),
    (b"TUE", 2),
    (b"WED", 3),
    (b"THU", 4),
    (b"FRI", 5),
    (b"SAT", 6),
];

fn map_hash_into_range(hashed_value: u64, range: RangeInclusive<u8>) -> u8 {
    let modulo = range.end() - range.start() + 1;
    let hashed_value = hashed_value % modulo as u64;
    (range.start() + hashed_value as u8).min(*range.end())
}

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
        assert_debug_snapshot!(parse_crontab("0 0 L,15W JAN-MAR/2 MON-FRI UTC").unwrap());

        // Keep numeric and named literals aligned with common cron syntax and the crate docs.
        assert_debug_snapshot!(parse_crontab("00 04 01 Jan-Mar/02 Mon-Fri UTC").unwrap());
    }

    #[test]
    fn test_parse_crontab_irregular_whitespace() {
        let canonical = parse_crontab("2 4 * * 0-6 Asia/Shanghai").unwrap();
        let irregular = parse_crontab("\t2  4 *\n* 0-6  Asia/Shanghai ").unwrap();
        assert_eq!(format!("{canonical:?}"), format!("{irregular:?}"));

        let canonical = parse_crontab("invalid 4 * * * Asia/Shanghai").unwrap_err();
        let irregular = parse_crontab("\tinvalid  4 *\n* *  Asia/Shanghai ").unwrap_err();
        assert_eq!(canonical.to_string(), irregular.to_string());
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

        // parser boundary behavior
        assert_snapshot!(parse_crontab("*/0 * * * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("*/60 * * * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0,,1 * * * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0, * * * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 32W * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 * * 5#6 UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 L/2 * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 * JAN-FOO * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 * * MON# UTC").unwrap_err());
        assert_snapshot!(parse_crontab("18446744073709551616 * * * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0,1- * * * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0,H * * * * UTC").unwrap_err());

        // Diagnostics after consuming syntax accepted by the intentional compatibility fixes.
        assert_snapshot!(parse_crontab("060 * * * * UTC").unwrap_err());
        assert_snapshot!(parse_crontab("0 0 1 JanX * UTC").unwrap_err());
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
