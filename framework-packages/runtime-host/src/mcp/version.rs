// Conservative semantic-version range checks for resolved MCP packages.
/// The half-open version range a declared requirement admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VersionBounds {
    lower: [u64; 3],
    upper: [u64; 3],
}

impl VersionBounds {
    /// `false` only when the concrete version is certainly outside the range. A version with a
    /// pre-release tag is admitted rather than judged, because pre-release ordering is exactly
    /// where a hand-written comparison would diverge from the host's real one.
    fn admits(&self, version: &str) -> bool {
        match parse_release_version(version) {
            Some(parsed) => parsed >= self.lower && parsed < self.upper,
            None => true,
        }
    }
}

/// Derive the half-open range for the comparator forms Art manifests actually use: a single `=`,
/// `^`, `~` or bare comparator over one to three numeric components.
///
/// Returns `None` for everything else — conjunctions, inequality comparators, wildcards and
/// pre-release comparators — which means "not checked here", not "satisfied". That is deliberate.
/// The authoritative containment check belongs to the host and already runs with the real `semver`
/// crate in `crates/loom_tool_registry/src/framework_process.rs` before the dependency is
/// resolved; this one exists so that a resolved server the Art never declared cannot slip through
/// a host that skipped, lost or predates that check. It is therefore written to be *sound* rather
/// than complete: it must never reject a version the host would accept, so anything it cannot
/// decide exactly it admits.
///
/// When `semver` can be added to this package's manifest, delete both this and
/// `parse_release_version` and use `VersionReq::parse(requirement)` with `Version::parse(version)`
/// instead. The dependency is absent today only because regenerating
/// `framework-packages/runtime-host/Cargo.lock` is blocked by another lane's uncommitted crate;
/// see F13 and H11 in `docs/progress/phase-78-lane-sync.md`.
fn requirement_bounds(requirement: &str) -> Option<VersionBounds> {
    let requirement = requirement.trim();
    if requirement.contains(',') {
        return None;
    }
    let (operator, rest) = match requirement.chars().next()? {
        '^' => ('^', &requirement[1..]),
        '~' => ('~', &requirement[1..]),
        '=' => ('=', &requirement[1..]),
        '0'..='9' => ('^', requirement),
        _ => return None,
    };
    let components = parse_version_components(rest.trim())?;
    let mut lower = [0_u64; 3];
    for (index, component) in components.iter().enumerate() {
        lower[index] = *component;
    }
    let upper = match operator {
        '=' => match components.len() {
            3 => [lower[0], lower[1], lower[2].saturating_add(1)],
            2 => [lower[0], lower[1].saturating_add(1), 0],
            _ => [lower[0].saturating_add(1), 0, 0],
        },
        '~' => match components.len() {
            1 => [lower[0].saturating_add(1), 0, 0],
            _ => [lower[0], lower[1].saturating_add(1), 0],
        },
        // Caret, including the bare form, bumps the leftmost non-zero component the requirement
        // actually spelled out — so `^0.1` allows `0.1.9` but not `0.2.0`.
        _ => {
            if lower[0] > 0 {
                [lower[0].saturating_add(1), 0, 0]
            } else if components.len() == 1 {
                [1, 0, 0]
            } else if lower[1] > 0 {
                [0, lower[1].saturating_add(1), 0]
            } else if components.len() == 2 {
                [0, 1, 0]
            } else {
                [0, 0, lower[2].saturating_add(1)]
            }
        }
    };
    Some(VersionBounds { lower, upper })
}

/// A concrete version as three numeric components, or `None` when it carries a pre-release tag or
/// anything else this framework will not judge. Build metadata is dropped, because semver ignores
/// it when comparing.
fn parse_release_version(version: &str) -> Option<[u64; 3]> {
    let version = version.trim();
    let version = version.split_once('+').map_or(version, |(head, _)| head);
    let components = parse_version_components(version)?;
    let mut parsed = [0_u64; 3];
    for (index, component) in components.iter().enumerate() {
        parsed[index] = *component;
    }
    Some(parsed)
}

fn parse_version_components(value: &str) -> Option<Vec<u64>> {
    let components = value
        .split('.')
        .map(|component| {
            (!component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| component.parse::<u64>().ok())
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    (1..=3).contains(&components.len()).then_some(components)
}

fn format_version(version: [u64; 3]) -> String {
    format!("{}.{}.{}", version[0], version[1], version[2])
}
