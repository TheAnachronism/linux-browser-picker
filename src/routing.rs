use std::ffi::OsStr;

use crate::configuration::{
    self, BrowserDestination, Configuration, DestinationLaunch, FallbackAction, Inspected,
    LaunchMode, MigrationPreview, PathComparison, RoutingAction, RoutingRule, UrlCondition,
};
use crate::discovery;
use crate::i18n;
use crate::launcher;
use crate::open_target::{self, OpenTarget, WebTarget};

pub enum Error {
    InvalidTarget(open_target::Error),
    Configuration(configuration::Error),
    Launch(launcher::Error),
    NoGraphicalSession,
}

struct Diagnostic {
    kind: &'static str,
    result: &'static str,
    rule_id: Option<String>,
    destination_id: Option<String>,
    mode: Option<LaunchMode>,
    private: Option<&'static str>,
    availability: Option<&'static str>,
}

impl Diagnostic {
    fn render(&self) -> String {
        let mut lines = vec![
            format!("kind={}", self.kind),
            format!("result={}", self.result),
        ];
        if let Some(rule_id) = &self.rule_id {
            lines.push(format!("rule={rule_id}"));
        }
        if let Some(destination_id) = &self.destination_id {
            lines.push(format!("destination={destination_id}"));
        }
        if let Some(mode) = self.mode {
            let mode = match mode {
                LaunchMode::Normal => "normal",
                LaunchMode::Private => "private",
            };
            lines.push(format!("mode={mode}"));
        }
        if let Some(private) = self.private {
            lines.push(format!("private={private}"));
        }
        if let Some(availability) = self.availability {
            lines.push(format!("availability={availability}"));
        }
        lines.push(String::new());
        lines.join("\n")
    }
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
    RecoverLaunch {
        target: OpenTarget,
        error: launcher::Error,
        destinations: Vec<BrowserDestination>,
        preselection: Preselection,
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
    route_with(argument, AutomaticAction::Dispatch)
}

pub fn preview(argument: &OsStr) -> Result<Outcome, Error> {
    route_with(argument, AutomaticAction::Preselect)
}

#[derive(Clone, Copy)]
enum AutomaticAction {
    Dispatch,
    RecoverFailure,
    Preselect,
}

fn route_with(argument: &OsStr, automatic: AutomaticAction) -> Result<Outcome, Error> {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(Error::NoGraphicalSession);
    }

    let target = OpenTarget::parse(argument).map_err(Error::InvalidTarget)?;
    let interactive = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
    let automatic = if interactive && matches!(automatic, AutomaticAction::Dispatch) {
        AutomaticAction::RecoverFailure
    } else {
        automatic
    };
    match configuration::inspect().map_err(Error::Configuration)? {
        Inspected::Missing => Ok(Outcome::Setup { target }),
        Inspected::Ready(configuration) => route_configured(configuration, target, automatic),
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

pub fn diagnose(argument: &OsStr) -> Result<String, Error> {
    let target = OpenTarget::parse(argument).map_err(Error::InvalidTarget)?;
    match configuration::inspect().map_err(Error::Configuration)? {
        Inspected::Missing => Ok(diagnostic(kind_of(&target), "setup", None, None, None)),
        Inspected::Invalid(error) => Err(Error::Configuration(error)),
        Inspected::Migratable { .. } => {
            Ok(diagnostic(kind_of(&target), "migrate", None, None, None))
        }
        Inspected::Ready(configuration) => Ok(diagnose_configured(configuration, target).render()),
    }
}

fn diagnostic(
    kind: &'static str,
    result: &'static str,
    rule_id: Option<String>,
    destination_id: Option<String>,
    mode: Option<LaunchMode>,
) -> String {
    Diagnostic {
        kind,
        result,
        rule_id,
        destination_id,
        mode,
        private: None,
        availability: None,
    }
    .render()
}

fn kind_of(target: &OpenTarget) -> &'static str {
    match target {
        OpenTarget::Web(_) => "web",
        OpenTarget::File(_) => "file",
    }
}

fn destination_capability(
    destinations: &[BrowserDestination],
    destination_id: &str,
) -> (Option<&'static str>, Option<&'static str>) {
    let Some(destination) = destinations
        .iter()
        .find(|destination| destination.id == destination_id)
    else {
        return (None, None);
    };
    (
        Some(if destination.supports_private() {
            "available"
        } else {
            "unavailable"
        }),
        Some(if destination.is_available() {
            "available"
        } else {
            "unavailable"
        }),
    )
}

fn diagnose_configured(configuration: Configuration, target: OpenTarget) -> Diagnostic {
    if matches!(target, OpenTarget::File(_)) {
        return Diagnostic {
            kind: "file",
            result: "picker",
            rule_id: None,
            destination_id: None,
            mode: None,
            private: None,
            availability: None,
        };
    }
    let OpenTarget::Web(web) = &target else {
        unreachable!("file Open Targets already returned");
    };
    if let Some(rule) = first_matching_rule(&configuration.rules, web) {
        let (destination_id, mode, automatic) = match &rule.action {
            RoutingAction::Open { destination, mode } => (destination.clone(), *mode, true),
            RoutingAction::Preselect { destination, mode } => (destination.clone(), *mode, false),
        };
        let (private, availability) =
            destination_capability(&configuration.destinations, &destination_id);
        return Diagnostic {
            kind: "web",
            result: if automatic {
                "dispatched"
            } else {
                "preselection"
            },
            rule_id: Some(rule.id.clone()),
            destination_id: Some(destination_id),
            mode: Some(mode),
            private,
            availability,
        };
    }
    match configuration.fallback {
        FallbackAction::Open {
            destination: destination_id,
            mode,
        } => {
            let (private, availability) =
                destination_capability(&configuration.destinations, &destination_id);
            Diagnostic {
                kind: "web",
                result: "dispatched",
                rule_id: None,
                destination_id: Some(destination_id),
                mode: Some(mode),
                private,
                availability,
            }
        }
        FallbackAction::ShowPicker => Diagnostic {
            kind: "web",
            result: "picker",
            rule_id: None,
            destination_id: None,
            mode: None,
            private: None,
            availability: None,
        },
    }
}

fn route_configured(
    configuration: Configuration,
    target: OpenTarget,
    automatic: AutomaticAction,
) -> Result<Outcome, Error> {
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
        return apply_rule(rule, configuration.destinations, target, automatic);
    }
    apply_fallback(
        configuration.fallback,
        configuration.destinations,
        target,
        automatic,
    )
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
                    LaunchMode::Normal => i18n::text("normal"),
                    LaunchMode::Private => i18n::text("private"),
                };
                i18n::text_with(
                    "Fallback: open {destination} in {mode} Launch Mode",
                    &[("{destination}", destination), ("{mode}", &mode)],
                )
            }
            FallbackAction::ShowPicker => i18n::text("Fallback: show Picker"),
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
    automatic: AutomaticAction,
) -> Result<Outcome, Error> {
    let (destination_id, mode, opens_automatically) = match &rule.action {
        RoutingAction::Open { destination, mode } => (destination, *mode, true),
        RoutingAction::Preselect { destination, mode } => (destination, *mode, false),
    };
    if opens_automatically {
        apply_automatic(destination_id, mode, destinations, target, automatic)
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
    automatic: AutomaticAction,
) -> Result<Outcome, Error> {
    match fallback {
        FallbackAction::Open {
            destination: destination_id,
            mode,
        } => apply_automatic(&destination_id, mode, destinations, target, automatic),
        FallbackAction::ShowPicker => Ok(Outcome::Pick {
            target,
            destinations,
            preselection: None,
        }),
    }
}

fn apply_automatic(
    destination_id: &str,
    mode: LaunchMode,
    destinations: Vec<BrowserDestination>,
    target: OpenTarget,
    automatic: AutomaticAction,
) -> Result<Outcome, Error> {
    let preselection = || Preselection {
        destination_id: destination_id.to_owned(),
        mode,
    };
    if matches!(automatic, AutomaticAction::Preselect) {
        return Ok(Outcome::Pick {
            target,
            destinations,
            preselection: Some(preselection()),
        });
    }
    match launcher::dispatch(
        destination(&destinations, destination_id),
        &target,
        mode == LaunchMode::Private,
    ) {
        Ok(()) => Ok(Outcome::Dispatched),
        Err(error) if matches!(automatic, AutomaticAction::RecoverFailure) => {
            Ok(Outcome::RecoverLaunch {
                target,
                error,
                destinations,
                preselection: preselection(),
            })
        }
        Err(error) => Err(Error::Launch(error)),
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
            .to_lowercase()
            .starts_with(&expected.to_lowercase()),
        PathComparison::Prefix => candidate.starts_with(expected),
    }
}

fn text_equal(candidate: &str, expected: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        candidate.to_lowercase() == expected.to_lowercase()
    } else {
        candidate == expected
    }
}

fn describe_condition(condition: &UrlCondition) -> String {
    match condition {
        UrlCondition::Scheme { value, negate } => described(
            *negate,
            i18n::text_with("scheme is {value}", &[("{value}", value)]),
        ),
        UrlCondition::Host {
            value,
            include_subdomains,
            negate,
        } => described(
            *negate,
            if *include_subdomains {
                i18n::text_with(
                    "host is {value} or a label-boundary subdomain",
                    &[("{value}", value)],
                )
            } else {
                i18n::text_with("host is exactly {value}", &[("{value}", value)])
            },
        ),
        UrlCondition::Port { value, negate } => described(
            *negate,
            i18n::text_with(
                "explicit non-default port is {value}",
                &[("{value}", &value.to_string())],
            ),
        ),
        UrlCondition::Path {
            value,
            comparison,
            case_insensitive,
            negate,
        } => described(
            *negate,
            i18n::text_with(
                "path {comparison} {value} ({sensitivity})",
                &[
                    (
                        "{comparison}",
                        &match comparison {
                            PathComparison::Exact => i18n::text("equals"),
                            PathComparison::Prefix => i18n::text("starts with"),
                        },
                    ),
                    ("{value}", &format!("{value:?}")),
                    ("{sensitivity}", &sensitivity(*case_insensitive)),
                ],
            ),
        ),
        UrlCondition::QueryKey {
            key,
            case_insensitive,
            negate,
        } => described(
            *negate,
            i18n::text_with(
                "query contains key {key} ({sensitivity})",
                &[
                    ("{key}", &format!("{key:?}")),
                    ("{sensitivity}", &sensitivity(*case_insensitive)),
                ],
            ),
        ),
        UrlCondition::QueryValue {
            key,
            value,
            case_insensitive,
            negate,
        } => described(
            *negate,
            i18n::text_with(
                "query contains {key}={value} ({sensitivity})",
                &[
                    ("{key}", &format!("{key:?}")),
                    ("{value}", &format!("{value:?}")),
                    ("{sensitivity}", &sensitivity(*case_insensitive)),
                ],
            ),
        ),
        UrlCondition::Glob {
            value,
            case_insensitive,
            negate,
        } => described(
            *negate,
            i18n::text_with(
                "glob {value} matches the Matching URL ({sensitivity})",
                &[
                    ("{value}", &format!("{value:?}")),
                    ("{sensitivity}", &sensitivity(*case_insensitive)),
                ],
            ),
        ),
        UrlCondition::Regex {
            value,
            case_insensitive,
            negate,
        } => described(
            *negate,
            i18n::text_with(
                "regular expression {value} matches the Matching URL ({sensitivity})",
                &[
                    ("{value}", &format!("{value:?}")),
                    ("{sensitivity}", &sensitivity(*case_insensitive)),
                ],
            ),
        ),
    }
}

fn described(negate: bool, description: String) -> String {
    if negate {
        i18n::text_with("NOT ({description})", &[("{description}", &description)])
    } else {
        description
    }
}

fn sensitivity(case_insensitive: bool) -> String {
    if case_insensitive {
        i18n::text("case-insensitive")
    } else {
        i18n::text("case-sensitive")
    }
}

fn describe_action(action: &RoutingAction) -> String {
    let (kind, destination, mode) = match action {
        RoutingAction::Open { destination, mode } => (i18n::text("open"), destination, mode),
        RoutingAction::Preselect { destination, mode } => {
            (i18n::text("preselect"), destination, mode)
        }
    };
    let mode = match mode {
        LaunchMode::Normal => i18n::text("Normal"),
        LaunchMode::Private => i18n::text("Private"),
    };
    i18n::text_with(
        "{kind} {destination} in {mode} Launch Mode",
        &[
            ("{kind}", &kind),
            ("{destination}", destination),
            ("{mode}", &mode),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    use crate::configuration::{
        ConditionGroup, Configuration, FallbackAction, LaunchMode, PathComparison, RoutingAction,
        RoutingRule, UrlCondition,
    };

    fn parse_web(url: &str) -> WebTarget {
        match OpenTarget::parse(OsStr::new(url)) {
            Ok(OpenTarget::Web(target)) => target,
            Ok(OpenTarget::File(_)) => panic!("expected a web Open Target"),
            Err(error) => panic!("{}", error.message()),
        }
    }

    fn configuration_with(condition: UrlCondition) -> Configuration {
        Configuration {
            destinations: Vec::new(),
            rules: vec![RoutingRule {
                id: "match".to_owned(),
                name: "Match".to_owned(),
                enabled: true,
                groups: vec![ConditionGroup {
                    conditions: vec![condition],
                }],
                action: RoutingAction::Open {
                    destination: "controlled".to_owned(),
                    mode: LaunchMode::Normal,
                },
            }],
            fallback: FallbackAction::ShowPicker,
        }
    }

    fn wins(url: &str, condition: UrlCondition) -> bool {
        evaluate(&configuration_with(condition), &parse_web(url))
            .winner
            .as_deref()
            == Some("match")
    }

    #[test]
    fn unicode_case_insensitive_path_matches_without_byte_slicing() {
        assert!(wins(
            "https://example.com/Café/Page",
            UrlCondition::Path {
                value: "/CAFÉ".to_owned(),
                comparison: PathComparison::Prefix,
                case_insensitive: true,
                negate: false,
            },
        ));
        assert!(wins(
            "https://example.com/İstanbul/docs",
            UrlCondition::Path {
                value: "/i".to_owned(),
                comparison: PathComparison::Prefix,
                case_insensitive: true,
                negate: false,
            },
        ));
        assert!(!wins(
            "https://example.com/Café/Page",
            UrlCondition::Path {
                value: "/CAFÉ".to_owned(),
                comparison: PathComparison::Prefix,
                case_insensitive: false,
                negate: false,
            },
        ));
        assert!(!wins(
            "https://example.com/Caf%C3%A9/Page",
            UrlCondition::Path {
                value: "/CAFÉ".to_owned(),
                comparison: PathComparison::Prefix,
                case_insensitive: true,
                negate: false,
            },
        ));
    }

    #[test]
    fn unicode_case_insensitive_query_matches_encoded_spelling() {
        assert!(wins(
            "https://example.com/path?Q=Café",
            UrlCondition::QueryValue {
                key: "q".to_owned(),
                value: "CAFÉ".to_owned(),
                case_insensitive: true,
                negate: false,
            },
        ));
        assert!(!wins(
            "https://example.com/path?Q=Caf%C3%A9",
            UrlCondition::QueryValue {
                key: "q".to_owned(),
                value: "CAFÉ".to_owned(),
                case_insensitive: true,
                negate: false,
            },
        ));
    }
}
