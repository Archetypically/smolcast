//! Launcher ranking: fuzzy score plus integer usage bonuses, with a total
//! deterministic order.
//!
//! Windows-free and host-testable (see `docs/systemPatterns.md` §6.3).
//!
//! ```text
//! frequency_bonus = min(launch_count, 100) * 4
//! recency_bonus   = 20 if age <= 1 day, 10 if <= 7 days, 5 if <= 30 days, else 0
//! combined_score  = fuzzy_score * 100 + frequency_bonus + recency_bonus
//! ```
//!
//! Sort key, in order: combined desc, fuzzy desc, launch_count desc,
//! last_launch desc, sort_name asc, AppId asc. Nothing depends on input,
//! filesystem, hash-map, or channel order.

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::core::app_index::IndexedApp;
use crate::core::usage::{Usage, UsageStats};

pub const FREQUENCY_CAP: u64 = 100;
pub const FREQUENCY_WEIGHT: i64 = 4;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// One ranked result: an index into the slice passed to [`rank`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ranked {
    pub index: usize,
    pub fuzzy_score: i64,
    pub combined_score: i64,
}

/// `min(launch_count, 100) * 4`.
pub fn frequency_bonus(launch_count: u64) -> i64 {
    // The cap keeps the product far below overflow.
    launch_count.min(FREQUENCY_CAP) as i64 * FREQUENCY_WEIGHT
}

/// Recency tier for a launch at `last_ms` observed at `now_ms`. A future
/// timestamp earns nothing.
pub fn recency_bonus(last_ms: i64, now_ms: i64) -> i64 {
    let Some(age) = now_ms.checked_sub(last_ms) else {
        return 0;
    };
    match age {
        a if a < 0 => 0,
        a if a <= DAY_MS => 20,
        a if a <= 7 * DAY_MS => 10,
        a if a <= 30 * DAY_MS => 5,
        _ => 0,
    }
}

/// Rank `apps` for `query` (trimmed). A non-empty query drops apps whose name
/// and aliases all fail to match; an empty query scores every app fuzzy 0.
pub fn rank(query: &str, apps: &[IndexedApp], usage: &Usage, now_ms: i64) -> Vec<Ranked> {
    let query = query.trim();
    let matcher = SkimMatcherV2::default();

    let mut ranked: Vec<(Ranked, UsageStats)> = apps
        .iter()
        .enumerate()
        .filter_map(|(index, app)| {
            let fuzzy_score = if query.is_empty() {
                0
            } else {
                std::iter::once(&app.display_name)
                    .chain(&app.aliases)
                    .filter_map(|name| matcher.fuzzy_match(name, query))
                    .max()?
            };
            let stats = usage.get(&app.id).unwrap_or_default();
            let combined_score = fuzzy_score
                .saturating_mul(100)
                .saturating_add(frequency_bonus(stats.launch_count))
                .saturating_add(recency_bonus(stats.last_launch_unix_ms, now_ms));
            Some((
                Ranked {
                    index,
                    fuzzy_score,
                    combined_score,
                },
                stats,
            ))
        })
        .collect();

    ranked.sort_by(|(a, a_stats), (b, b_stats)| {
        let (a_app, b_app) = (&apps[a.index], &apps[b.index]);
        b.combined_score
            .cmp(&a.combined_score)
            .then_with(|| b.fuzzy_score.cmp(&a.fuzzy_score))
            .then_with(|| b_stats.launch_count.cmp(&a_stats.launch_count))
            .then_with(|| {
                b_stats
                    .last_launch_unix_ms
                    .cmp(&a_stats.last_launch_unix_ms)
            })
            .then_with(|| a_app.sort_name.cmp(&b_app.sort_name))
            .then_with(|| a_app.id.cmp(&b_app.id))
    });
    ranked.into_iter().map(|(r, _)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::app_index::{AppId, AppSource, LaunchSpec};

    const NOW: i64 = 1_800_000_000_000;

    fn app(name: &str, key: &str) -> IndexedApp {
        IndexedApp {
            id: AppId::Packaged {
                aumid_key: key.into(),
            },
            display_name: name.into(),
            sort_name: name.to_lowercase(),
            aliases: Vec::new(),
            source: AppSource::AppsFolder,
            launch: LaunchSpec::Packaged { aumid: key.into() },
            icon_key: key.into(),
        }
    }

    fn launches(usage: &mut Usage, app: &IndexedApp, count: u64, at: i64) {
        for _ in 0..count {
            usage.record_launch(&app.id, at);
        }
    }

    fn names(apps: &[IndexedApp], ranked: &[Ranked]) -> Vec<String> {
        ranked
            .iter()
            .map(|r| apps[r.index].display_name.clone())
            .collect()
    }

    #[test]
    fn frequency_bonus_is_capped() {
        assert_eq!(frequency_bonus(0), 0);
        assert_eq!(frequency_bonus(25), 100);
        assert_eq!(frequency_bonus(100), 400);
        assert_eq!(frequency_bonus(u64::MAX), 400);
    }

    #[test]
    fn recency_tiers_and_invalid_timestamps() {
        assert_eq!(recency_bonus(NOW, NOW), 20);
        assert_eq!(recency_bonus(NOW - DAY_MS, NOW), 20);
        assert_eq!(recency_bonus(NOW - DAY_MS - 1, NOW), 10);
        assert_eq!(recency_bonus(NOW - 7 * DAY_MS, NOW), 10);
        assert_eq!(recency_bonus(NOW - 30 * DAY_MS, NOW), 5);
        assert_eq!(recency_bonus(NOW - 30 * DAY_MS - 1, NOW), 0);
        // Future and overflowing ages earn nothing.
        assert_eq!(recency_bonus(NOW + 1, NOW), 0);
        assert_eq!(recency_bonus(i64::MIN, NOW), 0);
        assert_eq!(recency_bonus(i64::MAX, i64::MIN), 0);
    }

    #[test]
    fn non_empty_query_excludes_non_matches_and_uses_best_alias() {
        let mut calc = app("Calculator", "calc");
        let mut term = app("Terminal", "term");
        term.aliases = vec!["Windows Terminal".into()];
        calc.aliases = vec!["Rechner".into()];
        let apps = vec![calc, term];
        let usage = Usage::default();

        let ranked = rank("wterm", &apps, &usage, NOW);
        assert_eq!(names(&apps, &ranked), vec!["Terminal"]);
        assert!(ranked[0].fuzzy_score > 0);

        let ranked = rank("rechner", &apps, &usage, NOW);
        assert_eq!(names(&apps, &ranked), vec!["Calculator"]);

        assert!(rank("zzzz", &apps, &usage, NOW).is_empty());
    }

    #[test]
    fn empty_query_orders_by_usage_then_name() {
        let apps = vec![
            app("Zeta", "z"),
            app("alpha", "a"),
            app("Beta", "b"),
            app("Gamma", "g"),
        ];
        let mut usage = Usage::default();
        launches(&mut usage, &apps[3], 2, NOW - 40 * DAY_MS); // freq 8, no recency
        launches(&mut usage, &apps[2], 1, NOW - DAY_MS / 2); // freq 4 + recency 20
        for query in ["", "   "] {
            let ranked = rank(query, &apps, &usage, NOW);
            assert_eq!(
                names(&apps, &ranked),
                vec!["Beta", "Gamma", "alpha", "Zeta"]
            );
            assert!(ranked.iter().all(|r| r.fuzzy_score == 0));
            assert_eq!(ranked[0].combined_score, 24);
        }
    }

    #[test]
    fn full_tie_tuple_is_deterministic() {
        // With an empty query every fuzzy score is 0, so each tie-break key is
        // exercised in turn:
        //   A: 5 launches, 40 days ago -> 20 (frequency only)
        //   D: 5 launches, 45 days ago -> 20, same count as A but older
        //   C: 0 launches, now          -> 20 (recency only), fewer launches
        //   B: no usage                 -> 0, sort_name "b"
        //   F-App (id "e") and f-app (id "f"): no usage, equal sort_name;
        //   AppId decides.
        let apps = vec![
            app("f-app", "f"),
            app("B", "b"),
            app("C", "c"),
            app("D", "d"),
            app("A", "a"),
            app("F-App", "e"),
        ];
        let usage = Usage::decode(
            br#"{"schema_version": 1, "entries": [
                {"id": {"kind": "packaged", "aumid_key": "a"}, "launch_count": 5, "last_launch_unix_ms": 1796544000000},
                {"id": {"kind": "packaged", "aumid_key": "d"}, "launch_count": 5, "last_launch_unix_ms": 1796112000000},
                {"id": {"kind": "packaged", "aumid_key": "c"}, "launch_count": 0, "last_launch_unix_ms": 1800000000000}
            ]}"#,
        )
        .unwrap();

        let expected = vec!["A", "D", "C", "B", "F-App", "f-app"];
        let ranked = rank("", &apps, &usage, NOW);
        assert_eq!(names(&apps, &ranked), expected);
        assert_eq!(
            ranked.iter().map(|r| r.combined_score).collect::<Vec<_>>(),
            vec![20, 20, 20, 0, 0, 0]
        );

        // Input order never matters.
        let mut reversed = apps.clone();
        reversed.reverse();
        let ranked = rank("", &reversed, &usage, NOW);
        assert_eq!(names(&reversed, &ranked), expected);
    }

    #[test]
    fn fuzzy_score_outweighs_maximal_usage() {
        // "pa" matches both; Paint's prefix match must beat Notepad's capped
        // 400 + 20 usage bonus because fuzzy scores are weighted by 100.
        let apps = vec![app("Notepad", "notepad"), app("Paint", "paint")];
        let mut usage = Usage::default();
        launches(&mut usage, &apps[0], 1000, NOW);
        let ranked = rank("pa", &apps, &usage, NOW);
        assert_eq!(names(&apps, &ranked), vec!["Paint", "Notepad"]);
        assert!(ranked[0].fuzzy_score > ranked[1].fuzzy_score);
    }
}
