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

//! Parsing throughput on syntax shared by cronexpr, cronp, saffron, and croner.
//!
//! Each benchmark turns the same input text into a complete schedule and drops it in the timed
//! region. Matching and next-occurrence calculation are not included. The cases and comparison
//! method, including cronp's Vixie dialect, are adapted from cronp's benchmark:
//! <https://github.com/al8n/cronp/blob/c6e1c5ec70abc04e98e1cd2f01a9f60f3b0e0b77/benches/parse.rs>.

use cronexpr::FallbackTimezoneOption;
use cronexpr::ParseOptions;
use divan::Bencher;
use divan::black_box;
use divan::black_box_drop;

const SIMPLE: &str = "30 2 * * 1-5";
const DENSE: &str = "0,15,30,45 0-23/2 1-15 JAN-JUN MON-FRI";
const REJECTED: &str = "0 0 * * 99";

fn main() {
    acceptance_preflight();
    divan::main();
}

fn cronexpr_options() -> ParseOptions {
    let mut options = ParseOptions::default();
    options.fallback_timezone_option = FallbackTimezoneOption::UTC;
    options
}

fn acceptance_preflight() {
    for expression in [SIMPLE, DENSE] {
        assert!(
            cronexpr::parse_crontab_with(expression, cronexpr_options()).is_ok(),
            "cronexpr must accept the successful comparison case `{expression}`"
        );
        assert!(
            cronp::Schedule::<cronp::Vixie>::parse(expression).is_ok(),
            "cronp must accept the successful comparison case `{expression}`"
        );
        assert!(
            expression.parse::<saffron::Cron>().is_ok(),
            "saffron must accept the successful comparison case `{expression}`"
        );
        assert!(
            expression.parse::<croner::Cron>().is_ok(),
            "croner must accept the successful comparison case `{expression}`"
        );
    }

    assert!(
        cronexpr::parse_crontab_with(REJECTED, cronexpr_options()).is_err(),
        "cronexpr must reject the invalid comparison case `{REJECTED}`"
    );
    assert!(
        cronp::Schedule::<cronp::Vixie>::parse(REJECTED).is_err(),
        "cronp must reject the invalid comparison case `{REJECTED}`"
    );
    assert!(
        REJECTED.parse::<saffron::Cron>().is_err(),
        "saffron must reject the invalid comparison case `{REJECTED}`"
    );
    assert!(
        REJECTED.parse::<croner::Cron>().is_err(),
        "croner must reject the invalid comparison case `{REJECTED}`"
    );
}

#[divan::bench_group(name = "parse/simple")]
mod parse_simple {
    use super::*;

    #[divan::bench]
    fn cronexpr(bencher: Bencher) {
        let options = cronexpr_options();
        bencher.bench(|| {
            let result = cronexpr::parse_crontab_with(black_box(SIMPLE), options);
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn cronp(bencher: Bencher) {
        bencher.bench(|| {
            let result = cronp::Schedule::<cronp::Vixie>::parse(black_box(SIMPLE));
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn saffron(bencher: Bencher) {
        bencher.bench(|| {
            let result = black_box(SIMPLE).parse::<saffron::Cron>();
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn croner(bencher: Bencher) {
        bencher.bench(|| {
            let result = black_box(SIMPLE).parse::<croner::Cron>();
            black_box_drop(result);
        });
    }
}

#[divan::bench_group(name = "parse/dense")]
mod parse_dense {
    use super::*;

    #[divan::bench]
    fn cronexpr(bencher: Bencher) {
        let options = cronexpr_options();
        bencher.bench(|| {
            let result = cronexpr::parse_crontab_with(black_box(DENSE), options);
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn cronp(bencher: Bencher) {
        bencher.bench(|| {
            let result = cronp::Schedule::<cronp::Vixie>::parse(black_box(DENSE));
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn saffron(bencher: Bencher) {
        bencher.bench(|| {
            let result = black_box(DENSE).parse::<saffron::Cron>();
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn croner(bencher: Bencher) {
        bencher.bench(|| {
            let result = black_box(DENSE).parse::<croner::Cron>();
            black_box_drop(result);
        });
    }
}

// Rejection is deliberately separate from successful parsing: every library stops at the first
// error, so this group compares only their error paths.
#[divan::bench_group(name = "reject/out-of-range")]
mod reject_out_of_range {
    use super::*;

    #[divan::bench]
    fn cronexpr(bencher: Bencher) {
        let options = cronexpr_options();
        bencher.bench(|| {
            let result = cronexpr::parse_crontab_with(black_box(REJECTED), options);
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn cronp(bencher: Bencher) {
        bencher.bench(|| {
            let result = cronp::Schedule::<cronp::Vixie>::parse(black_box(REJECTED));
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn saffron(bencher: Bencher) {
        bencher.bench(|| {
            let result = black_box(REJECTED).parse::<saffron::Cron>();
            black_box_drop(result);
        });
    }

    #[divan::bench]
    fn croner(bencher: Bencher) {
        bencher.bench(|| {
            let result = black_box(REJECTED).parse::<croner::Cron>();
            black_box_drop(result);
        });
    }
}
