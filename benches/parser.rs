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

use cronexpr::FallbackTimezoneOption;
use cronexpr::ParseOptions;
use cronexpr::parse_crontab;
use cronexpr::parse_crontab_with;
use divan::Bencher;
use divan::black_box;
use divan::counter::BytesCount;

fn main() {
    divan::main();
}

#[derive(Debug, Copy, Clone)]
enum SuccessCase {
    Scalar,
    Wildcard,
    Complex,
}

impl SuccessCase {
    fn expression(self, timezone: bool) -> &'static str {
        match (self, timezone) {
            (SuccessCase::Scalar, false) => "0 0 1 1 1",
            (SuccessCase::Scalar, true) => "0 0 1 1 1 UTC",
            (SuccessCase::Wildcard, false) => "* * * * *",
            (SuccessCase::Wildcard, true) => "* * * * * UTC",
            (SuccessCase::Complex, false) => {
                "0,5,10,15,20,25,30,35,40,45,50,55 0-23/2 L,1W,15W JAN-DEC/2 MON-FRI,0L,5#3"
            }
            (SuccessCase::Complex, true) => {
                "0,5,10,15,20,25,30,35,40,45,50,55 0-23/2 L,1W,15W JAN-DEC/2 MON-FRI,0L,5#3 America/New_York"
            }
        }
    }
}

const SUCCESS_CASES: &[SuccessCase] = &[
    SuccessCase::Scalar,
    SuccessCase::Wildcard,
    SuccessCase::Complex,
];

#[derive(Debug, Copy, Clone)]
enum FailureCase {
    EarlyMalformed,
    LateOutOfRange,
    UnknownTimezone,
}

impl FailureCase {
    fn expression(self) -> &'static str {
        match self {
            FailureCase::EarlyMalformed => "invalid 0 1 1 1 UTC",
            FailureCase::LateOutOfRange => {
                "0,5,10,15,20,25,30,35,40,45,50,55 0-23/2 L,1W,15W JAN-DEC/2 MON-FRI,0L,5#6 America/New_York"
            }
            FailureCase::UnknownTimezone => {
                "0,5,10,15,20,25,30,35,40,45,50,55 0-23/2 L,1W,15W JAN-DEC/2 MON-FRI,0L,5#3 Unknown/Timezone"
            }
        }
    }
}

const FAILURE_CASES: &[FailureCase] = &[
    FailureCase::EarlyMalformed,
    FailureCase::LateOutOfRange,
    FailureCase::UnknownTimezone,
];

#[divan::bench(args = SUCCESS_CASES)]
fn parse_fields(bencher: Bencher, case: SuccessCase) {
    let expression = case.expression(false);
    let mut options = ParseOptions::default();
    options.fallback_timezone_option = FallbackTimezoneOption::UTC;
    assert!(parse_crontab_with(expression, options).is_ok());

    bencher.counter(BytesCount::of_str(expression)).bench(|| {
        let parsed = parse_crontab_with(black_box(expression), options).unwrap();
        black_box(parsed);
    });
}

#[divan::bench(args = SUCCESS_CASES)]
fn parse_with_timezone(bencher: Bencher, case: SuccessCase) {
    let expression = case.expression(true);
    assert!(parse_crontab(expression).is_ok());

    bencher.counter(BytesCount::of_str(expression)).bench(|| {
        let parsed = parse_crontab(black_box(expression)).unwrap();
        black_box(parsed);
    });
}

#[divan::bench]
fn parse_hashed(bencher: Bencher) {
    const EXPRESSION: &str = "H H H H H UTC";
    let mut options = ParseOptions::default();
    options.hashed_value = Some(42);
    assert!(parse_crontab_with(EXPRESSION, options).is_ok());

    bencher.counter(BytesCount::of_str(EXPRESSION)).bench(|| {
        let parsed = parse_crontab_with(black_box(EXPRESSION), options).unwrap();
        black_box(parsed);
    });
}

#[divan::bench(args = FAILURE_CASES)]
fn parse_failure(bencher: Bencher, case: FailureCase) {
    let expression = case.expression();
    assert!(parse_crontab(expression).is_err());

    bencher.counter(BytesCount::of_str(expression)).bench(|| {
        let error = parse_crontab(black_box(expression)).unwrap_err();
        black_box(error);
    });
}
