use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::configuration::{
    ConditionGroup, LaunchMode, PathComparison, RoutingAction, RoutingRule, UrlCondition,
};
use crate::i18n;
use crate::open_target::WebTarget;
use crate::routing::RoutingEvaluation;

#[derive(Clone)]
pub struct RoutingRuleEditor {
    pub root: gtk::Box,
    pub test: gtk::Button,
    pub explanation: gtk::Label,
    list: gtk::ListBox,
    rules: Rc<RefCell<Vec<RuleWidgets>>>,
}

#[derive(Clone)]
struct RuleWidgets {
    enabled: gtk::CheckButton,
    id: gtk::Entry,
    name: gtk::Entry,
    action: gtk::DropDown,
    destination: gtk::Entry,
    mode: gtk::DropDown,
    groups: Rc<RefCell<Vec<GroupWidgets>>>,
    row: gtk::ListBoxRow,
}

#[derive(Clone)]
struct GroupWidgets {
    conditions: Rc<RefCell<Vec<ConditionWidgets>>>,
}

#[derive(Clone)]
struct ConditionWidgets {
    kind: gtk::DropDown,
    key: gtk::Entry,
    value: gtk::Entry,
    option: gtk::CheckButton,
    insensitive: gtk::CheckButton,
    negate: gtk::CheckButton,
}

impl RoutingRuleEditor {
    pub fn new(
        existing: &[RoutingRule],
        target: Option<&WebTarget>,
        destination: Option<&str>,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 9);
        let heading = gtk::Label::builder()
            .label(i18n::text("Routing Rules"))
            .xalign(0.0)
            .build();
        heading.add_css_class("title-2");
        root.append(&heading);

        let help = gtk::Label::builder()
            .label(i18n::text("Enabled rules run from top to bottom. Groups are OR alternatives; every condition in a group must match. Matching uses the normalized Matching URL shown by Test Rules."))
            .xalign(0.0)
            .wrap(true)
            .build();
        help.add_css_class("dim-label");
        root.append(&help);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("boxed-list");
        list.update_property(&[gtk::accessible::Property::Label("Ordered Routing Rules")]);
        let rules = Rc::new(RefCell::new(Vec::new()));
        for rule in existing {
            let widgets = build_rule(rule.clone());
            list.append(&widgets.row);
            rules.borrow_mut().push(widgets);
        }
        if let Some(first) = rules.borrow().first() {
            list.select_row(Some(&first.row));
        }
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .max_content_height(420)
            .propagate_natural_height(true)
            .build();
        root.append(&scroller);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let add = gtk::Button::with_label(&i18n::text("Add Routing Rule"));
        add.update_property(&[gtk::accessible::Property::Label("Add Routing Rule")]);
        let up = gtk::Button::from_icon_name("go-up-symbolic");
        up.update_property(&[
            gtk::accessible::Property::Label("Move Routing Rule up"),
            gtk::accessible::Property::KeyShortcuts("<Alt>Up"),
        ]);
        let down = gtk::Button::from_icon_name("go-down-symbolic");
        down.update_property(&[
            gtk::accessible::Property::Label("Move Routing Rule down"),
            gtk::accessible::Property::KeyShortcuts("<Alt>Down"),
        ]);
        controls.append(&add);
        controls.append(&up);
        controls.append(&down);
        root.append(&controls);

        let matching = target.map(|target| target.matching_url().to_owned());
        let target_host = target.map(|target| target.ascii_host().to_owned());
        let initial_destination = destination.unwrap_or_default().to_owned();
        add.connect_clicked(glib::clone!(
            #[weak]
            list,
            #[strong]
            rules,
            move |_| {
                let number = rules.borrow().len() + 1;
                let condition = UrlCondition::Host {
                    value: target_host.clone().unwrap_or_default(),
                    include_subdomains: false,
                    negate: false,
                };
                let rule = RoutingRule {
                    id: format!("rule-{number}"),
                    name: target_host
                        .clone()
                        .map_or_else(|| format!("Rule {number}"), |host| format!("Route {host}")),
                    enabled: true,
                    groups: vec![ConditionGroup {
                        conditions: vec![condition],
                    }],
                    action: RoutingAction::Preselect {
                        destination: initial_destination.clone(),
                        mode: LaunchMode::Normal,
                    },
                };
                let widgets = build_rule(rule);
                list.append(&widgets.row);
                list.select_row(Some(&widgets.row));
                widgets.id.grab_focus();
                rules.borrow_mut().push(widgets);
            }
        ));
        up.connect_clicked(glib::clone!(
            #[weak]
            list,
            #[strong]
            rules,
            move |_| reorder(&list, &rules, -1)
        ));
        down.connect_clicked(glib::clone!(
            #[weak]
            list,
            #[strong]
            rules,
            move |_| reorder(&list, &rules, 1)
        ));

        let test = gtk::Button::with_label(&i18n::text("Test Rules"));
        test.update_property(&[gtk::accessible::Property::Label("Test Routing Rules")]);
        root.append(&test);
        let explanation = gtk::Label::builder()
            .label(matching.map_or_else(
                || i18n::text("Open an URL to test Routing Rules."),
                |url| i18n::text_with("Matching URL: {url}", &[("{url}", &url)]),
            ))
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        explanation
            .update_property(&[gtk::accessible::Property::Label("Routing Rule explanation")]);
        root.append(&explanation);

        Self {
            root,
            test,
            explanation,
            list,
            rules,
        }
    }

    pub fn rules(&self) -> Vec<RoutingRule> {
        self.rules.borrow().iter().map(collect_rule).collect()
    }

    pub fn connect_order_shortcuts(&self, window: &adw::ApplicationWindow) {
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let list = self.list.clone();
        let rules = Rc::clone(&self.rules);
        controller.connect_key_pressed(move |_, key, _, modifiers| {
            if modifiers.contains(gtk::gdk::ModifierType::ALT_MASK) {
                if key == gtk::gdk::Key::Up {
                    reorder(&list, &rules, -1);
                    return glib::Propagation::Stop;
                }
                if key == gtk::gdk::Key::Down {
                    reorder(&list, &rules, 1);
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        window.add_controller(controller);
    }
}

pub fn explanation_text(evaluation: &RoutingEvaluation) -> String {
    let mut lines = vec![format!("Matching URL: {}", evaluation.matching_url)];
    for rule in &evaluation.rules {
        let state = if !rule.enabled {
            "disabled"
        } else if rule.won {
            "FIRST WINNER"
        } else if rule.matched {
            "matched after winner"
        } else {
            "did not match"
        };
        lines.push(format!("{} ({}): {state}", rule.name, rule.id));
        for (group_index, group) in rule.groups.iter().enumerate() {
            lines.push(format!(
                "  OR group {}: {}",
                group_index + 1,
                if group.matched {
                    "matched"
                } else {
                    "did not match"
                }
            ));
            for condition in &group.conditions {
                lines.push(format!(
                    "    {} — {}",
                    if condition.matched { "PASS" } else { "FAIL" },
                    condition.description
                ));
            }
        }
    }
    lines.push(match evaluation.winner.as_deref() {
        Some(winner) => format!("First winner: {winner}"),
        None => "First winner: none".to_owned(),
    });
    lines.push(format!("Resulting action: {}", evaluation.action));
    lines.join("\n")
}

fn build_rule(rule: RoutingRule) -> RuleWidgets {
    let enabled = gtk::CheckButton::with_label(&i18n::text("Enabled"));
    enabled.set_active(rule.enabled);
    let id = entry(&rule.id, "Routing Rule ID");
    let name = entry(&rule.name, "Routing Rule name");
    let action = dropdown(
        &["Open automatically", "Preselect in Picker"],
        "Routing Rule action",
    );
    let (action_index, destination_id, launch_mode) = match rule.action {
        RoutingAction::Open { destination, mode } => (0, destination, mode),
        RoutingAction::Preselect { destination, mode } => (1, destination, mode),
    };
    action.set_selected(action_index);
    let destination = entry(&destination_id, "Action Browser Destination ID");
    let mode = dropdown(&["Normal", "Private"], "Action Launch Mode");
    mode.set_selected(if launch_mode == LaunchMode::Private {
        1
    } else {
        0
    });

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.append(&enabled);
    header.append(&id);
    header.append(&name);
    header.append(&action);
    header.append(&destination);
    header.append(&mode);

    let groups_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let groups = Rc::new(RefCell::new(Vec::new()));
    for group in rule.groups {
        let (widgets, row) = build_group(group);
        groups_box.append(&row);
        groups.borrow_mut().push(widgets);
    }
    let add_group = gtk::Button::with_label(&i18n::text("Add OR group"));
    add_group.update_property(&[gtk::accessible::Property::Label("Add OR condition group")]);
    add_group.connect_clicked(glib::clone!(
        #[weak]
        groups_box,
        #[strong]
        groups,
        move |_| {
            let (widgets, row) = build_group(ConditionGroup {
                conditions: vec![UrlCondition::Host {
                    value: String::new(),
                    include_subdomains: false,
                    negate: false,
                }],
            });
            groups_box.append(&row);
            groups.borrow_mut().push(widgets);
        }
    ));

    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.set_margin_top(9);
    body.set_margin_bottom(9);
    body.set_margin_start(9);
    body.set_margin_end(9);
    body.append(&header);
    body.append(&groups_box);
    body.append(&add_group);
    let row = gtk::ListBoxRow::builder()
        .child(&body)
        .selectable(true)
        .activatable(false)
        .build();
    row.update_property(&[gtk::accessible::Property::Label(&rule.name)]);
    RuleWidgets {
        enabled,
        id,
        name,
        action,
        destination,
        mode,
        groups,
        row,
    }
}

fn build_group(group: ConditionGroup) -> (GroupWidgets, gtk::Box) {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let heading = gtk::Label::builder()
        .label(i18n::text("All conditions below must match (AND)"))
        .xalign(0.0)
        .build();
    row.append(&heading);
    let conditions_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let conditions = Rc::new(RefCell::new(Vec::new()));
    for condition in group.conditions {
        let (widgets, condition_row) = build_condition(condition);
        conditions_box.append(&condition_row);
        conditions.borrow_mut().push(widgets);
    }
    row.append(&conditions_box);
    let add = gtk::Button::with_label(&i18n::text("Add AND condition"));
    add.connect_clicked(glib::clone!(
        #[weak]
        conditions_box,
        #[strong]
        conditions,
        move |_| {
            let (widgets, condition_row) = build_condition(UrlCondition::Host {
                value: String::new(),
                include_subdomains: false,
                negate: false,
            });
            conditions_box.append(&condition_row);
            conditions.borrow_mut().push(widgets);
        }
    ));
    row.append(&add);
    (GroupWidgets { conditions }, row)
}

fn build_condition(condition: UrlCondition) -> (ConditionWidgets, gtk::Box) {
    let kind = dropdown(
        &[
            "Scheme",
            "Host",
            "Port",
            "Exact path",
            "Path prefix",
            "Query key",
            "Query value",
        ],
        "URL Condition type",
    );
    let (index, key_text, value_text, option_active, insensitive_active, negate_active) =
        match condition {
            UrlCondition::Scheme { value, negate } => {
                (0, "".to_owned(), value, false, false, negate)
            }
            UrlCondition::Host {
                value,
                include_subdomains,
                negate,
            } => (1, "".to_owned(), value, include_subdomains, false, negate),
            UrlCondition::Port { value, negate } => {
                (2, "".to_owned(), value.to_string(), false, false, negate)
            }
            UrlCondition::Path {
                value,
                comparison,
                case_insensitive,
                negate,
            } => (
                if matches!(comparison, PathComparison::Exact) {
                    3
                } else {
                    4
                },
                "".to_owned(),
                value,
                false,
                case_insensitive,
                negate,
            ),
            UrlCondition::QueryKey {
                key,
                case_insensitive,
                negate,
            } => (5, key, "".to_owned(), false, case_insensitive, negate),
            UrlCondition::QueryValue {
                key,
                value,
                case_insensitive,
                negate,
            } => (6, key, value, false, case_insensitive, negate),
        };
    kind.set_selected(index);
    let key = entry(&key_text, "URL Condition query key");
    let value = entry(&value_text, "URL Condition value");
    let option = gtk::CheckButton::with_label(&i18n::text("Include subdomains"));
    option.set_active(option_active);
    let insensitive = gtk::CheckButton::with_label(&i18n::text("Ignore case"));
    insensitive.set_active(insensitive_active);
    let negate = gtk::CheckButton::with_label(&i18n::text("Not"));
    negate.set_active(negate_active);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.append(&kind);
    row.append(&key);
    row.append(&value);
    row.append(&option);
    row.append(&insensitive);
    row.append(&negate);
    (
        ConditionWidgets {
            kind,
            key,
            value,
            option,
            insensitive,
            negate,
        },
        row,
    )
}

fn collect_rule(rule: &RuleWidgets) -> RoutingRule {
    let destination = rule.destination.text().to_string();
    let mode = if rule.mode.selected() == 1 {
        LaunchMode::Private
    } else {
        LaunchMode::Normal
    };
    let action = if rule.action.selected() == 0 {
        RoutingAction::Open { destination, mode }
    } else {
        RoutingAction::Preselect { destination, mode }
    };
    RoutingRule {
        id: rule.id.text().to_string(),
        name: rule.name.text().to_string(),
        enabled: rule.enabled.is_active(),
        groups: rule.groups.borrow().iter().map(collect_group).collect(),
        action,
    }
}

fn collect_group(group: &GroupWidgets) -> ConditionGroup {
    ConditionGroup {
        conditions: group
            .conditions
            .borrow()
            .iter()
            .map(collect_condition)
            .collect(),
    }
}

fn collect_condition(condition: &ConditionWidgets) -> UrlCondition {
    let key = condition.key.text().to_string();
    let value = condition.value.text().to_string();
    let negate = condition.negate.is_active();
    let case_insensitive = condition.insensitive.is_active();
    match condition.kind.selected() {
        0 => UrlCondition::Scheme { value, negate },
        1 => UrlCondition::Host {
            value,
            include_subdomains: condition.option.is_active(),
            negate,
        },
        2 => UrlCondition::Port {
            value: value.parse().unwrap_or(0),
            negate,
        },
        3 | 4 => UrlCondition::Path {
            value,
            comparison: if condition.kind.selected() == 3 {
                PathComparison::Exact
            } else {
                PathComparison::Prefix
            },
            case_insensitive,
            negate,
        },
        5 => UrlCondition::QueryKey {
            key,
            case_insensitive,
            negate,
        },
        _ => UrlCondition::QueryValue {
            key,
            value,
            case_insensitive,
            negate,
        },
    }
}

fn reorder(list: &gtk::ListBox, rules: &Rc<RefCell<Vec<RuleWidgets>>>, direction: isize) {
    let Some(selected) = list.selected_row() else {
        return;
    };
    let mut rules = rules.borrow_mut();
    let Some(index) = rules.iter().position(|rule| rule.row == selected) else {
        return;
    };
    let next = index as isize + direction;
    if next < 0 || next >= rules.len() as isize {
        return;
    }
    let next = next as usize;
    rules.swap(index, next);
    let row = rules[next].row.clone();
    list.remove(&row);
    list.insert(&row, next as i32);
    list.select_row(Some(&row));
    row.grab_focus();
}

fn entry(text: &str, label: &str) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .text(text)
        .placeholder_text(i18n::text(label))
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(label)]);
    entry
}

fn dropdown(values: &[&str], label: &str) -> gtk::DropDown {
    let localized: Vec<_> = values.iter().map(|value| i18n::text(value)).collect();
    let localized: Vec<_> = localized.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&localized);
    dropdown.update_property(&[gtk::accessible::Property::Label(label)]);
    dropdown
}
