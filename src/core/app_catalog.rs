use std::collections::HashSet;

use crate::core::config::ApplicationTarget;
use crate::core::search::Query;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub target: ApplicationTarget,
    pub names: Vec<String>,
}

pub fn matching_apps(
    apps: &[InstalledApp],
    query: &str,
    window_apps: &HashSet<String>,
    excluded_names: &[String],
    alias_app: Option<&str>,
) -> Vec<usize> {
    let Some(query) = Query::new(query).filter(|query| !query.is_empty()) else {
        return Vec::new();
    };
    let excluded: HashSet<_> = excluded_names
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect();
    let mut matches: Vec<_> = apps
        .iter()
        .enumerate()
        .filter_map(|(index, app)| {
            if window_apps.contains(&app.target.bundle_id)
                || app
                    .names
                    .iter()
                    .chain(std::iter::once(&app.target.name))
                    .any(|name| excluded.contains(&name.to_lowercase()))
            {
                return None;
            }
            let alias_match = alias_app == Some(app.target.bundle_id.as_str());
            let score = if alias_match {
                12_000
            } else {
                query.score(
                    app.names
                        .iter()
                        .chain(std::iter::once(&app.target.name))
                        .map(String::as_str),
                )?
            };
            Some((index, alias_match, score))
        })
        .collect();
    matches.sort_by(|(a, alias_a, sa), (b, alias_b, sb)| {
        alias_b
            .cmp(alias_a)
            .then_with(|| sb.cmp(sa))
            .then_with(|| {
                apps[*a]
                    .target
                    .name
                    .to_lowercase()
                    .cmp(&apps[*b].target.name.to_lowercase())
            })
            .then_with(|| apps[*a].target.bundle_id.cmp(&apps[*b].target.bundle_id))
    });
    matches.into_iter().map(|(index, _, _)| index).collect()
}
