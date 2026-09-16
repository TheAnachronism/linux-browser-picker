use std::ffi::OsStr;

use crate::configuration::{
    self, BrowserDestination, Configuration, DestinationLaunch, FallbackAction, Inspected,
    LaunchMode, MigrationPreview, PathComparison, RoutingAction, RoutingRule, UrlCondition,
};
use crate::discovery;
use crate::launcher;
use crate::open_target::{self, OpenTarget, WebTarget};

pub enum Error {
    InvalidTarget(open_target::Error),
    Configuration(configuration::Error),
    Launch(launcher::Error),
    NoGraphicalSession,
}

#[derive(Clone, Debug)]
pub struct Preselection {
    pub destination_id: String,
    pub mode: LaunchMode,
}

pub enum Outcome {
    Dispatched,
    Pick {
        target: OpenTarget,
        destinations: Vec<BrowserDestination>,
        preselection: Option<Preselection>,
    },
    Setup {
        target: OpenTarget,
    },
    Recover {
        target: OpenTarget,
        error: configuration::Error,
        destinations: Vec<BrowserDestination>,
    },
    Migrate {
        target: OpenTarget,
        preview: MigrationPreview,
    },
}

#[derive(Clone, Debug)]
pub struct ConditionEvaluation {
    pub description: String,
    pub matched: bool,
}

#[derive(Clone, Debug)]
pub struct GroupEvaluation {
    pub conditions: Vec<ConditionEvaluation>,
    pub matched: bool,
}

#[derive(Clone, Debug)]
pub struct RuleEvaluation {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub groups: Vec<GroupEvaluation>,
    pub matched: bool,
    pub won: bool,
}

#[derive(Clone, Debug)]
pub struct RoutingEvaluation {
    pub matching_url: String,
    pub rules: Vec<RuleEvaluation>,
    pub winner: Option<String>,
    pub action: String,
}

pub fn route(argument: &OsStr) -> Result<Outcome, Error> {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(Error::NoGraphicalSession);
    }

    let target = OpenTarget::parse(argument).map_err(Error::InvalidTarget)?;
    let interactive = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
    match configuration::inspect().map_err(Error::Configuration)? {
        Inspected::Missing => Ok(Outcome::Setup { target }),
        Inspected::Ready(configuration) => route_configured(configuration, target),
        Inspected::Invalid(error) if interactive => Ok(Outcome::Recover {
            target,
            error,
            destinations: discovered_destinations(),
        }),
        Inspected::Invalid(error) => Err(Error::Configuration(error)),
        Inspected::Migratable { preview, .. } if interactive => {
            Ok(Outcome::Migrate { target, preview })
        }
        Inspected::Migratable { preview, .. } => Err(Error::Configuration(
            configuration::Error::MigrationRequired {
                from: preview.from,
                to: preview.to,
            },
        )),
    }
}

fn route_configured(configuration: Configuration, target: OpenTarget) -> Result<Outcome, Error> {
    if matches!(target, OpenTarget::File(_)) {
        return Ok(Outcome::Pick {
            target,
            destinations: configuration.destinations,
            preselection: None,
        });
    }
    if let OpenTarget::Web(web) = &target
        && let Some(rule) = first_matching_rule(&configuration.rules, web)
    {
        return apply_rule(rule, configuration.destinations, target);
    }
    apply_fallback(configuration.fallback, configuration.destinations, target)
}

pub fn discovered_destinations() -> Vec<BrowserDestination> {
    let mut used = std::collections::HashSet::new();
    discovery::discover()
        .ordinary
        .into_iter()
        .map(|candidate| {
            let id = discovery::unique_slug(
                &discovery::suggested_slug(&candidate.desktop_id),
                &mut used,
            );
            BrowserDestination {
                id,
                label: candidate.name.clone(),
                application_label: candidate.name,
                profile_label: None,
                icon_name: None,
                launch: DestinationLaunch::Discovered {
                    desktop_id: candidate.desktop_id,
                },
                unavailable_reason: None,
            }
        })
        .collect()
}

pub fn evaluate(configuration: &Configuration, target: &WebTarget) -> RoutingEvaluation {
    let winner = first_matching_rule(&configuration.rules, target).map(|rule| rule.id.clone());
    let rules = configuration
        .rules
        .iter()
        .map(|rule| {
            let groups = rule
                .groups
                .iter()
                .map(|group| {
                    let conditions: Vec<_> = group
                        .conditions
                        .iter()
                        .map(|condition| ConditionEvaluation {
                            description: describe_condition(condition),
                            matched: condition_matches(condition, target),
                        })
                        .collect();
                    let matched = conditions.iter().all(|condition| condition.matched);
                    GroupEvaluation {
                        conditions,
                        matched,
                    }
                })
                .collect::<Vec<_>>();
            let matched = rule.enabled && groups.iter().any(|group| group.matched);
            RuleEvaluation {
                id: rule.id.clone(),
                name: rule.name.clone(),
                enabled: rule.enabled,
                groups,
                matched,
                won: winner.as_deref() == Some(&rule.id),
            }
        })
        .collect();
    let action = winner
        .as_deref()
        .and_then(|id| configuration.rules.iter().find(|rule| rule.id == id))
        .map(|rule| describe_action(&rule.action))
        .unwrap_or_else(|| match &configuration.fallback {
            FallbackAction::Open { destination, mode } => {
                let mode = match mode {
                    LaunchMode::Normal => "normal",
                    LaunchMode::Private => "private",
                };
                format!("Fallback: open {destination} in {mode} Launch Mode")
            }
            FallbackAction::ShowPicker => "Fallback: show Picker".to_owned(),
        });
    RoutingEvaluation {
        matching_url: target.matching_url().to_owned(),
        rules,
        winner,
        action,
    }
}

fn first_matching_rule<'a>(
    rules: &'a [RoutingRule],
    target: &WebTarget,
) -> Option<&'a RoutingRule> {
    rules.iter().find(|rule| {
        rule.enabled
            && rule.groups.iter().any(|group| {
                group
                    .conditions
                    .iter()
                    .all(|condition| condition_matches(condition, target))
            })
    })
}

fn apply_rule(
    rule: &RoutingRule,
    destinations: Vec<BrowserDestination>,
    target: OpenTarget,
) -> Result<Outcome, Error> {
    let (destination_id, mode, automatic) = match &rule.action {
        RoutingAction::Open { destination, mode } => (destination, *mode, true),
        RoutingAction::Preselect { destination, mode } => (destination, *mode, false),
    };
    if automatic {
        let destination = destination(&destinations, destination_id);
        launcher::dispatch(destination, &target, mode == LaunchMode::Private)
            .map_err(Error::Launch)?;
        Ok(Outcome::Dispatched)
    } else {
        Ok(Outcome::Pick {
            target,
            destinations,
            preselection: Some(Preselection {
                destination_id: destination_id.clone(),
                mode,
            }),
        })
    }
}

fn apply_fallback(
    fallback: FallbackAction,
    destinations: Vec<BrowserDestination>,
    target: OpenTarget,
) -> Result<Outcome, Error> {
    match fallback {
        FallbackAction::Open {
            destination: destination_id,
            mode,
        } => {
            launcher::dispatch(
                destination(&destinations, &destination_id),
                &target,
                mode == LaunchMode::Private,
            )
            .map_err(Error::Launch)?;
            Ok(Outcome::Dispatched)
        }
        FallbackAction::ShowPicker => Ok(Outcome::Pick {
            target,
            destinations,
            preselection: None,
        }),
    }
}

fn destination<'a>(
    destinations: &'a [BrowserDestination],
    destination_id: &str,
) -> &'a BrowserDestination {
    destinations
        .iter()
        .find(|destination| destination.id == destination_id)
        .expect("validated routing destination should exist")
}

fn condition_matches(condition: &UrlCondition, target: &WebTarget) -> bool {
    let (matched, negate) = match condition {
        UrlCondition::Scheme { value, negate } => {
            (target.scheme().eq_ignore_ascii_case(value), *negate)
        }
        UrlCondition::Host {
            value,
            include_subdomains,
            negate,
        } => {
            let host = target.ascii_host();
            let matched = host.eq_ignore_ascii_case(value)
                || (*include_subdomains
                    && host.len() > value.len()
                    && host.as_bytes()[host.len() - value.len() - 1] == b'.'
                    && host[host.len() - value.len()..].eq_ignore_ascii_case(value));
            (matched, *negate)
        }
        UrlCondition::Port { value, negate } => (target.port() == Some(*value), *negate),
        UrlCondition::Path {
            value,
            comparison,
            case_insensitive,
            negate,
        } => (
            text_matches(target.path(), value, *comparison, *case_insensitive),
            *negate,
        ),
        UrlCondition::QueryKey {
            key,
            case_insensitive,
            negate,
        } => (
            query_pairs(target).any(|(candidate, _)| text_equal(candidate, key, *case_insensitive)),
            *negate,
        ),
        UrlCondition::QueryValue {
            key,
            value,
            case_insensitive,
            negate,
        } => (
            query_pairs(target).any(|(candidate_key, candidate_value)| {
                text_equal(candidate_key, key, *case_insensitive)
                    && text_equal(candidate_value, value, *case_insensitive)
            }),
            *negate,
        ),
        UrlCondition::Glob {
            value,
            case_insensitive,
            negate,
        } => (
            crate::url_pattern::glob_matches(value, target.matching_url(), *case_insensitive),
            *negate,
        ),
        UrlCondition::Regex {
            value,
            case_insensitive,
            negate,
        } => (
            crate::url_pattern::regex_matches(value, target.matching_url(), *case_insensitive),
            *negate,
        ),
    };
    matched != negate
}

fn query_pairs(target: &WebTarget) -> impl Iterator<Item = (&str, &str)> {
    target.query().into_iter().flat_map(|query| {
        query.split('&').map(|pair| {
            pair.split_once('=')
                .map_or((pair, ""), |(key, value)| (key, value))
        })
    })
}

fn text_matches(
    candidate: &str,
    expected: &str,
    comparison: PathComparison,
    case_insensitive: bool,
) -> bool {
    match comparison {
        PathComparison::Exact => text_equal(candidate, expected, case_insensitive),
        PathComparison::Prefix if case_insensitive => candidate
            .get(..expected.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected)),
        PathComparison::Prefix => candidate.starts_with(expected),
    }
}

fn text_equal(candidate: &str, expected: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        candidate.eq_ignore_ascii_case(expected)
    } else {
        candidate == expected
    }
}

fn describe_condition(condition: &UrlCondition) -> String {
    match condition {
        UrlCondition::Scheme { value, negate } => described(*negate, format!("scheme is {value}")),
        UrlCondition::Host {
            value,
            include_subdomains,
            negate,
        } => described(
            *negate,
            if *include_subdomains {
                format!("host is {value} or a label-boundary subdomain")
            } else {
                format!("host is exactly {value}")
            },
        ),
        UrlCondition::Port { value, negate } => {
            described(*negate, format!("explicit non-default port is {value}"))
        }
        UrlCondition::Path {
            value,
            comparison,
            case_insensitive,
            negate,
        } => described(
            *negate,
            format!(
                "path {} {value:?} ({})",
                match comparison {
                    PathComparison::Exact => "equals",
                    PathComparison::Prefix => "starts with",
                },
                sensitivity(*case_insensitive)
            ),
        ),
        UrlCondition::QueryKey {
            key,
            case_insensitive,
            negate,
        } => described(
            *negate,
            format!(
                "query contains key {key:?} ({})",
                sensitivity(*case_insensitive)
            ),
        ),
        UrlCondition::QueryValue {
            key,
            value,
            case_insensitive,
            negate,
        } => described(
            *negate,
            format!(
                "query contains {key:?}={value:?} ({})",
                sensitivity(*case_insensitive)
            ),
        ),
        UrlCondition::Glob {
            value,
            case_insensitive,
            negate,
        } => described(
            *negate,
            format!(
                "glob {value:?} matches the Matching URL ({})",
                sensitivity(*case_insensitive)
            ),
        ),
        UrlCondition::Regex {
            value,
            case_insensitive,
            negate,
        } => described(
            *negate,
            format!(
                "regular expression {value:?} matches the Matching URL ({})",
                sensitivity(*case_insensitive)
            ),
        ),
    }
}

fn described(negate: bool, description: String) -> String {
    if negate {
        format!("NOT ({description})")
    } else {
        description
    }
}

fn sensitivity(case_insensitive: bool) -> &'static str {
    if case_insensitive {
        "case-insensitive"
    } else {
        "case-sensitive"
    }
}

fn describe_action(action: &RoutingAction) -> String {
    let (kind, destination, mode) = match action {
        RoutingAction::Open { destination, mode } => ("open", destination, mode),
        RoutingAction::Preselect { destination, mode } => ("preselect", destination, mode),
    };
    format!("{kind} {destination} in {mode:?} Launch Mode")
}
