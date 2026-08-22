//! Performance budgets for Loom.
//!
//! Loom had no performance gate of any kind, so every optimisation the codebase already relies on
//! could regress without a single test noticing. This crate is the smallest thing that fixes that:
//! a named upper limit, an environment override for the machine that has to run it, and one failure
//! message shaped so the person reading a red build knows what was measured, what the limit was, and
//! how to change the limit if the new number is legitimate.
//!
//! A budget is deliberately a plain integer rather than a statistical distribution. Loom's first
//! three budgets measure bytes on the wire, peak resident bytes, and wall-clock milliseconds for a
//! single operation, and for all three a generous ceiling that never moves is worth more than a
//! precise number that has to be re-baselined every week. Set the default well above the observed
//! value: the budget exists to catch an order-of-magnitude regression, not to police noise.
//!
//! ```no_run
//! # fn measure() -> u64 { 1 }
//! let measured = measure();
//! loom_perf::assert_within("surface_action_response_bytes", "bytes", measured, 8_192);
//! ```

use std::env;
use std::fmt::Write as _;

/// Prefix of every budget override variable. A budget named `surface_action_response_bytes` is
/// overridden by `LOOM_PERF_MAX_SURFACE_ACTION_RESPONSE_BYTES`.
pub const BUDGET_ENV_PREFIX: &str = "LOOM_PERF_MAX_";

/// Name of the environment variable that overrides `metric`.
pub fn budget_env_var(metric: &str) -> String {
    let mut name = String::with_capacity(BUDGET_ENV_PREFIX.len() + metric.len());
    name.push_str(BUDGET_ENV_PREFIX);
    for character in metric.chars() {
        match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' => name.extend(character.to_uppercase()),
            _ => name.push('_'),
        }
    }
    name
}

/// The budget in force for `metric`: the environment override when one is set to a non-empty value,
/// otherwise `default`.
///
/// An override that is not a non-negative integer is an error rather than a silent fall back to the
/// default. A typo in a CI variable would otherwise look exactly like a passing gate.
pub fn budget(metric: &str, default: u64) -> Result<u64, String> {
    let variable = budget_env_var(metric);
    let Ok(raw) = env::var(&variable) else {
        return Ok(default);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    trimmed.parse::<u64>().map_err(|error| {
        format!("{variable} is set to `{raw}`, which is not a non-negative integer: {error}")
    })
}

/// One line describing a measurement against its budget, including how much headroom is left.
/// Printed by [`assert_within`] on success so a passing gate still records the number it saw.
pub fn report(metric: &str, unit: &str, measured: u64, budget: u64) -> String {
    let mut line = String::new();
    let _ = write!(
        line,
        "perf budget {metric}: {measured} {unit} of {budget} {unit}"
    );
    if budget > 0 {
        let used = (measured as f64 / budget as f64) * 100.0;
        let _ = write!(line, " ({used:.0}% of budget)");
    }
    line
}

/// `Ok` when `measured` is within the budget for `metric`, otherwise a message naming the override
/// variable, so a legitimate new number can be adopted without reading this crate's source.
pub fn check(metric: &str, unit: &str, measured: u64, default_budget: u64) -> Result<u64, String> {
    let budget = budget(metric, default_budget)?;
    if measured <= budget {
        return Ok(budget);
    }
    Err(format!(
        "{}; over budget by {} {unit}. If the new cost is intended, raise the default in the test \
         and say why in the commit; to measure a one-off run, set {}.",
        report(metric, unit, measured, budget),
        measured - budget,
        budget_env_var(metric)
    ))
}

/// [`check`], panicking on failure and printing the measurement on success. This is the form a test
/// wants: a red build carries the whole explanation, and a green one still leaves the number in the
/// log with `--nocapture`.
pub fn assert_within(metric: &str, unit: &str, measured: u64, default_budget: u64) {
    match check(metric, unit, measured, default_budget) {
        Ok(budget) => println!("{}", report(metric, unit, measured, budget)),
        Err(message) => panic!("{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_metric_name_becomes_an_uppercase_override_variable() {
        assert_eq!(
            budget_env_var("surface_action_response_bytes"),
            "LOOM_PERF_MAX_SURFACE_ACTION_RESPONSE_BYTES"
        );
        assert_eq!(
            budget_env_var("art-execution.wall-time-ms"),
            "LOOM_PERF_MAX_ART_EXECUTION_WALL_TIME_MS"
        );
    }

    #[test]
    fn an_unset_override_leaves_the_default_in_force() {
        // This metric name is used by no gate, so no other test can be setting it.
        assert_eq!(budget("unset_metric_for_test", 42), Ok(42));
    }

    #[test]
    fn a_measurement_at_the_budget_is_within_it() {
        assert_eq!(check("boundary_metric_for_test", "bytes", 42, 42), Ok(42));
    }

    #[test]
    fn a_measurement_over_the_budget_names_the_override_variable() {
        let error = check("over_metric_for_test", "bytes", 43, 42).expect_err("over budget");
        assert!(error.contains("over budget by 1 bytes"), "{error}");
        assert!(
            error.contains("LOOM_PERF_MAX_OVER_METRIC_FOR_TEST"),
            "{error}"
        );
    }

    #[test]
    fn a_report_states_the_share_of_the_budget_used() {
        let line = report("share_metric_for_test", "ms", 25, 100);
        assert!(line.contains("25 ms of 100 ms"), "{line}");
        assert!(line.contains("25% of budget"), "{line}");
    }

    #[test]
    fn a_malformed_override_fails_instead_of_falling_back() {
        // The variable is process-global, so this test owns a metric name no other test reads.
        let variable = budget_env_var("malformed_metric_for_test");
        // SAFETY: single-threaded scope of one uniquely named variable; no other test observes it.
        std::env::set_var(&variable, "lots");
        let error = budget("malformed_metric_for_test", 42).expect_err("malformed override");
        std::env::remove_var(&variable);
        assert!(error.contains(&variable), "{error}");
        assert!(error.contains("not a non-negative integer"), "{error}");
    }
}
