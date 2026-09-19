use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::configuration::{
    ConditionGroup, LaunchMode, PathComparison, RoutingAction, RoutingRule, UrlCondition,
};
use crate::i18n;
use crate::list_order;
use crate::open_target::OpenTarget;
use crate::reorder_ui;
use crate::routing::RoutingEvaluation;

type ChangeCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Clone, Copy)]
#[repr(u32)]
enum ConditionKind {
    Scheme,
    Host,
    Port,
    ExactPath,
    PathPrefix,
    QueryKey,
    QueryValue,
    Glob,
    Regex,
}

impl ConditionKind {
    const ALL: [Self; 9] = [
        Self::Scheme,
        Self::Host,
        Self::Port,
        Self::ExactPath,
        Self::PathPrefix,
        Self::QueryKey,
        Self::QueryValue,
        Self::Glob,
        Self::Regex,
    ];

    fn label(self) -> String {
        match self {
            Self::Scheme => i18n::text("Scheme"),
            Self::Host => i18n::text("Host"),
            Self::Port => i18n::text("Port"),
            Self::ExactPath => i18n::text("Exact path"),
            Self::PathPrefix => i18n::text("Path prefix"),
            Self::QueryKey => i18n::text("Query key"),
            Self::QueryValue => i18n::text("Query value"),
            Self::Glob => i18n::text("Glob"),
            Self::Regex => i18n::text("Regular expression"),
        }
    }

    fn labels() -> [String; 9] {
        Self::ALL.map(Self::label)
    }

    fn from_index(index: u32) -> Self {
        Self::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(Self::QueryValue)
    }

    fn from_condition(condition: &UrlCondition) -> Self {
        match condition {
            UrlCondition::Scheme { .. } => Self::Scheme,
            UrlCondition::Host { .. } => Self::Host,
            UrlCondition::Port { .. } => Self::Port,
            UrlCondition::Path {
                comparison: PathComparison::Exact,
                ..
            } => Self::ExactPath,
            UrlCondition::Path {
                comparison: PathComparison::Prefix,
                ..
            } => Self::PathPrefix,
            UrlCondition::QueryKey { .. } => Self::QueryKey,
            UrlCondition::QueryValue { .. } => Self::QueryValue,
            UrlCondition::Glob { .. } => Self::Glob,
            UrlCondition::Regex { .. } => Self::Regex,
        }
    }
}

#[derive(Clone)]
pub struct RoutingRuleEditor {
    pub root: gtk::Box,
    pub sample: gtk::Entry,
    pub test: gtk::Button,
    pub explanation: gtk::Label,
    list: gtk::ListBox,
    rules: Rc<RefCell<Vec<RuleWidgets>>>,
    on_change: ChangeCallback,
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
    drag_handle: gtk::Widget,
    row: gtk::ListBoxRow,
}

#[derive(Clone)]
struct GroupWidgets {
    conditions: Rc<RefCell<Vec<ConditionWidgets>>>,
    row: gtk::Box,
}

/// Everything `build_group` needs to create OR groups and wire their "Add"/"Remove" controls.
/// Bundled because these four values always travel together (the initial build and the "Add
/// OR group" handler both need the full set).
#[derive(Clone)]
struct GroupEditorContext {
    on_change: ChangeCallback,
    groups: Rc<RefCell<Vec<GroupWidgets>>>,
    groups_box: gtk::Box,
    add_group: gtk::Button,
}

/// Everything `build_condition` needs to create AND conditions and wire their "Add"/"Remove"
/// controls. See `GroupEditorContext` for why these are bundled.
#[derive(Clone)]
struct ConditionEditorContext {
    on_change: ChangeCallback,
    conditions: Rc<RefCell<Vec<ConditionWidgets>>>,
    conditions_box: gtk::Box,
    add: gtk::Button,
}

#[derive(Clone)]
struct ConditionWidgets {
    kind: gtk::DropDown,
    key: gtk::Entry,
    value: gtk::Entry,
    option: gtk::CheckButton,
    insensitive: gtk::CheckButton,
    negate: gtk::CheckButton,
    row: gtk::Box,
}

impl RoutingRuleEditor {
    pub fn new(
        existing: &[RoutingRule],
        target: Option<&OpenTarget>,
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

        let on_change: ChangeCallback = Rc::new(RefCell::new(None));
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("boxed-list");
        list.update_property(&[gtk::accessible::Property::Label(&i18n::text(
            "Ordered Routing Rules",
        ))]);
        let rules = Rc::new(RefCell::new(Vec::new()));
        for rule in existing {
            let widgets = build_rule(rule.clone(), &on_change);
            list.append(&widgets.row);
            rules.borrow_mut().push(widgets);
        }
        let on_drop: Rc<dyn Fn(usize, usize)> = Rc::new(glib::clone!(
            #[weak]
            list,
            #[strong]
            rules,
            move |from, to| {
                if reorder_ui::move_row_to(
                    &list,
                    &mut rules.borrow_mut(),
                    |rule| rule.row.clone(),
                    from,
                    to,
                ) {
                    if let Some(row) = list.selected_row() {
                        row.grab_focus();
                    }
                }
            }
        ));
        for rule in rules.borrow().iter() {
            reorder_ui::attach_row_drag(&rule.row, &rule.drag_handle, Rc::clone(&on_drop));
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
        add.update_property(&[gtk::accessible::Property::Label(&i18n::text(
            "Add Routing Rule",
        ))]);
        let up = gtk::Button::from_icon_name("go-up-symbolic");
        up.update_property(&[
            gtk::accessible::Property::Label(&i18n::text("Move Routing Rule up")),
            gtk::accessible::Property::KeyShortcuts("<Alt>Up"),
        ]);
        let down = gtk::Button::from_icon_name("go-down-symbolic");
        down.update_property(&[
            gtk::accessible::Property::Label(&i18n::text("Move Routing Rule down")),
            gtk::accessible::Property::KeyShortcuts("<Alt>Down"),
        ]);
        let remove = gtk::Button::with_label(&i18n::text("Remove Routing Rule"));
        remove.update_property(&[gtk::accessible::Property::Label(&i18n::text(
            "Remove Routing Rule",
        ))]);
        controls.append(&add);
        controls.append(&up);
        controls.append(&down);
        controls.append(&remove);
        root.append(&controls);

        let matching = target
            .and_then(OpenTarget::as_web)
            .map(|target| target.matching_url().to_owned());
        let target_host = target
            .and_then(OpenTarget::as_web)
            .map(|target| target.ascii_host().to_owned());
        let initial_destination = destination.unwrap_or_default().to_owned();
        add.connect_clicked(glib::clone!(
            #[weak]
            list,
            #[strong]
            rules,
            #[strong]
            on_change,
            #[strong]
            on_drop,
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
                let widgets = build_rule(rule, &on_change);
                reorder_ui::attach_row_drag(
                    &widgets.row,
                    &widgets.drag_handle,
                    Rc::clone(&on_drop),
                );
                list.append(&widgets.row);
                list.select_row(Some(&widgets.row));
                widgets.id.grab_focus();
                rules.borrow_mut().push(widgets);
                notify_change(&on_change);
            }
        ));
        remove.connect_clicked(glib::clone!(
            #[weak]
            list,
            #[strong]
            rules,
            #[strong]
            on_change,
            move |_| {
                let Some(selected) = list.selected_row() else {
                    return;
                };
                remove_and_refocus(
                    &on_change,
                    &rules,
                    |rule| rule.row == selected,
                    |removed| list.remove(&removed.row),
                    |next| {
                        list.select_row(Some(&next.row));
                        next.destination.grab_focus();
                    },
                    || {},
                );
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

        let sample = gtk::Entry::builder()
            .text(target.map(OpenTarget::as_str).unwrap_or_default())
            .placeholder_text(i18n::text("Matching URL to test"))
            .build();
        sample.update_property(&[gtk::accessible::Property::Label(&i18n::text(
            "Matching URL to test",
        ))]);
        root.append(&sample);
        let test = gtk::Button::with_label(&i18n::text("Test Rules"));
        root.append(&test);
        let explanation = gtk::Label::builder()
            .label(match target {
                Some(OpenTarget::File(_)) => file_explanation(),
                Some(_) => matching.map_or_else(
                    || i18n::text("Open an URL to test Routing Rules."),
                    |url| i18n::text_with("Matching URL: {url}", &[("{url}", &url)]),
                ),
                None => i18n::text("Open an URL to test Routing Rules."),
            })
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        explanation.update_property(&[gtk::accessible::Property::Label(&i18n::text(
            "Routing Rule explanation",
        ))]);
        root.append(&explanation);

        Self {
            root,
            sample,
            test,
            explanation,
            list,
            rules,
            on_change,
        }
    }

    pub fn rules(&self) -> Vec<RoutingRule> {
        self.rules.borrow().iter().map(collect_rule).collect()
    }

    pub fn connect_changed(&self, callback: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn rewrite_destination_id(&self, from: &str, to: &str) {
        if from == to {
            return;
        }
        for rule in self.rules.borrow().iter() {
            if rule.destination.text().as_str() == from {
                rule.destination.set_text(to);
            }
        }
    }

    pub fn referenced_destination_ids(&self) -> HashSet<String> {
        self.rules
            .borrow()
            .iter()
            .map(|rule| rule.destination.text().trim().to_string())
            .filter(|id| !id.is_empty())
            .collect()
    }

    pub fn connect_order_shortcuts(&self, window: &adw::ApplicationWindow) {
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let list = self.list.clone();
        let rules = Rc::clone(&self.rules);
        controller.connect_key_pressed(move |_, key, _, modifiers| {
            if modifiers.contains(gtk::gdk::ModifierType::ALT_MASK)
                && !modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK)
                && !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            {
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

pub fn file_explanation() -> String {
    i18n::text(
        "Local files always require a Picker choice. Routing Rules and Fallback Action do not apply.",
    )
}

pub fn explanation_text(evaluation: &RoutingEvaluation) -> String {
    let mut lines = vec![i18n::text_with(
        "Matching URL: {url}",
        &[("{url}", &evaluation.matching_url)],
    )];
    for rule in &evaluation.rules {
        let state = if !rule.enabled {
            i18n::text("disabled")
        } else if rule.won {
            i18n::text("FIRST WINNER")
        } else if rule.matched {
            i18n::text("matched after winner")
        } else {
            i18n::text("did not match")
        };
        lines.push(i18n::text_with(
            "{name} ({id}): {state}",
            &[
                ("{name}", &rule.name),
                ("{id}", &rule.id),
                ("{state}", &state),
            ],
        ));
        for (group_index, group) in rule.groups.iter().enumerate() {
            let group_state = if group.matched {
                i18n::text("matched")
            } else {
                i18n::text("did not match")
            };
            lines.push(i18n::text_with(
                "  OR group {number}: {state}",
                &[
                    ("{number}", &(group_index + 1).to_string()),
                    ("{state}", &group_state),
                ],
            ));
            for condition in &group.conditions {
                let result = if condition.matched {
                    i18n::text("PASS")
                } else {
                    i18n::text("FAIL")
                };
                lines.push(i18n::text_with(
                    "    {result} — {description}",
                    &[
                        ("{result}", &result),
                        ("{description}", &condition.description),
                    ],
                ));
            }
        }
    }
    lines.push(match evaluation.winner.as_deref() {
        Some(winner) => i18n::text_with("First winner: {winner}", &[("{winner}", winner)]),
        None => i18n::text("First winner: none"),
    });
    lines.push(i18n::text_with(
        "Resulting action: {action}",
        &[("{action}", &evaluation.action)],
    ));
    lines.join("\n")
}

fn build_rule(rule: RoutingRule, on_change: &ChangeCallback) -> RuleWidgets {
    let enabled = gtk::CheckButton::with_label(&i18n::text("Enabled"));
    enabled.set_active(rule.enabled);
    bind_toggle(&enabled, on_change);
    let id = entry(&rule.id, &i18n::text("Routing Rule ID"), on_change);
    let name = entry(&rule.name, &i18n::text("Routing Rule name"), on_change);
    let open_automatically = i18n::text("Open automatically");
    let preselect_in_picker = i18n::text("Preselect in Picker");
    let action = dropdown(
        &[&open_automatically, &preselect_in_picker],
        &i18n::text("Routing Rule action"),
        on_change,
    );
    let (action_index, destination_id, launch_mode) = match rule.action {
        RoutingAction::Open { destination, mode } => (0, destination, mode),
        RoutingAction::Preselect { destination, mode } => (1, destination, mode),
    };
    action.set_selected(action_index);
    let destination = entry(
        &destination_id,
        &i18n::text("Action Browser Destination ID"),
        on_change,
    );
    let normal = i18n::text("Normal");
    let private = i18n::text("Private");
    let mode = dropdown(
        &[&normal, &private],
        &i18n::text("Action Launch Mode"),
        on_change,
    );
    mode.set_selected(if launch_mode == LaunchMode::Private {
        1
    } else {
        0
    });

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let drag_handle = reorder_ui::prepend_handle(
        &header,
        &i18n::text_with("Reorder {name}", &[("{name}", &rule.name)]),
    );
    header.append(&enabled);
    header.append(&id);
    header.append(&name);
    header.append(&action);
    header.append(&destination);
    header.append(&mode);

    let groups_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let groups = Rc::new(RefCell::new(Vec::new()));
    let add_group = gtk::Button::with_label(&i18n::text("Add OR group"));
    add_group.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Add OR condition group",
    ))]);
    let group_ctx = GroupEditorContext {
        on_change: on_change.clone(),
        groups: groups.clone(),
        groups_box: groups_box.clone(),
        add_group: add_group.clone(),
    };
    for group in rule.groups {
        let widgets = build_group(group, &group_ctx);
        groups_box.append(&widgets.row);
        groups.borrow_mut().push(widgets);
    }
    add_group.connect_clicked(glib::clone!(
        #[strong]
        group_ctx,
        move |_| {
            let widgets = build_group(
                ConditionGroup {
                    conditions: vec![UrlCondition::Host {
                        value: String::new(),
                        include_subdomains: false,
                        negate: false,
                    }],
                },
                &group_ctx,
            );
            group_ctx.groups_box.append(&widgets.row);
            group_ctx.groups.borrow_mut().push(widgets);
            notify_change(&group_ctx.on_change);
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
        drag_handle,
        row,
    }
}

fn build_group(group: ConditionGroup, ctx: &GroupEditorContext) -> GroupWidgets {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let heading = gtk::Label::builder()
        .label(i18n::text("All conditions below must match (AND)"))
        .xalign(0.0)
        .build();
    row.append(&heading);
    let conditions_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let conditions = Rc::new(RefCell::new(Vec::new()));
    let add = gtk::Button::with_label(&i18n::text("Add AND condition"));
    let condition_ctx = ConditionEditorContext {
        on_change: ctx.on_change.clone(),
        conditions: conditions.clone(),
        conditions_box: conditions_box.clone(),
        add: add.clone(),
    };
    for condition in group.conditions {
        let widgets = build_condition(condition, &condition_ctx);
        conditions_box.append(&widgets.row);
        conditions.borrow_mut().push(widgets);
    }
    row.append(&conditions_box);
    add.connect_clicked(glib::clone!(
        #[strong]
        condition_ctx,
        move |_| {
            let widgets = build_condition(
                UrlCondition::Host {
                    value: String::new(),
                    include_subdomains: false,
                    negate: false,
                },
                &condition_ctx,
            );
            condition_ctx.conditions_box.append(&widgets.row);
            condition_ctx.conditions.borrow_mut().push(widgets);
            notify_change(&condition_ctx.on_change);
        }
    ));
    let remove = gtk::Button::with_label(&i18n::text("Remove OR group"));
    remove.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Remove OR group",
    ))]);
    let ctx = ctx.clone();
    remove.connect_clicked(glib::clone!(
        #[weak]
        row,
        #[strong]
        ctx,
        move |_| {
            remove_and_refocus(
                &ctx.on_change,
                &ctx.groups,
                |group| group.row == row,
                |removed| ctx.groups_box.remove(&removed.row),
                focus_group,
                || {
                    ctx.add_group.grab_focus();
                },
            );
        }
    ));
    row.append(&add);
    row.append(&remove);
    GroupWidgets { conditions, row }
}

fn build_condition(condition: UrlCondition, ctx: &ConditionEditorContext) -> ConditionWidgets {
    let on_change = &ctx.on_change;
    let labels = ConditionKind::labels();
    let labels: Vec<_> = labels.iter().map(String::as_str).collect();
    let kind = dropdown(&labels, &i18n::text("URL Condition type"), on_change);
    let (key_text, value_text, option_active, insensitive_active, negate_active) = match &condition
    {
        UrlCondition::Scheme { value, negate } => {
            ("".to_owned(), value.clone(), false, false, *negate)
        }
        UrlCondition::Host {
            value,
            include_subdomains,
            negate,
        } => (
            "".to_owned(),
            value.clone(),
            *include_subdomains,
            false,
            *negate,
        ),
        UrlCondition::Port { value, negate } => {
            ("".to_owned(), value.to_string(), false, false, *negate)
        }
        UrlCondition::Path {
            value,
            case_insensitive,
            negate,
            ..
        } => (
            "".to_owned(),
            value.clone(),
            false,
            *case_insensitive,
            *negate,
        ),
        UrlCondition::QueryKey {
            key,
            case_insensitive,
            negate,
        } => (
            key.clone(),
            "".to_owned(),
            false,
            *case_insensitive,
            *negate,
        ),
        UrlCondition::QueryValue {
            key,
            value,
            case_insensitive,
            negate,
        } => (
            key.clone(),
            value.clone(),
            false,
            *case_insensitive,
            *negate,
        ),
        UrlCondition::Glob {
            value,
            case_insensitive,
            negate,
        }
        | UrlCondition::Regex {
            value,
            case_insensitive,
            negate,
        } => (
            "".to_owned(),
            value.clone(),
            false,
            *case_insensitive,
            *negate,
        ),
    };
    kind.set_selected(ConditionKind::from_condition(&condition) as u32);
    let key = entry(&key_text, &i18n::text("URL Condition query key"), on_change);
    let value = entry(&value_text, &i18n::text("URL Condition value"), on_change);
    let option = gtk::CheckButton::with_label(&i18n::text("Include subdomains"));
    option.set_active(option_active);
    let insensitive = gtk::CheckButton::with_label(&i18n::text("Ignore case"));
    insensitive.set_active(insensitive_active);
    let negate = gtk::CheckButton::with_label(&i18n::text("Not"));
    negate.set_active(negate_active);
    bind_toggle(&option, on_change);
    bind_toggle(&insensitive, on_change);
    bind_toggle(&negate, on_change);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.append(&kind);
    row.append(&key);
    row.append(&value);
    row.append(&option);
    row.append(&insensitive);
    row.append(&negate);
    let remove = gtk::Button::with_label(&i18n::text("Remove AND condition"));
    remove.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Remove AND condition",
    ))]);
    let ctx = ctx.clone();
    remove.connect_clicked(glib::clone!(
        #[weak]
        row,
        #[strong]
        ctx,
        move |_| {
            remove_and_refocus(
                &ctx.on_change,
                &ctx.conditions,
                |condition| condition.row == row,
                |removed| ctx.conditions_box.remove(&removed.row),
                |next| {
                    next.value.grab_focus();
                },
                || {
                    ctx.add.grab_focus();
                },
            );
        }
    ));
    row.append(&remove);
    ConditionWidgets {
        kind,
        key,
        value,
        option,
        insensitive,
        negate,
        row,
    }
}

fn collect_rule(rule: &RuleWidgets) -> RoutingRule {
    let destination = rule.destination.text().trim().to_string();
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
    match ConditionKind::from_index(condition.kind.selected()) {
        ConditionKind::Scheme => UrlCondition::Scheme { value, negate },
        ConditionKind::Host => UrlCondition::Host {
            value,
            include_subdomains: condition.option.is_active(),
            negate,
        },
        ConditionKind::Port => UrlCondition::Port {
            value: value.parse().unwrap_or(0),
            negate,
        },
        ConditionKind::ExactPath => UrlCondition::Path {
            value,
            comparison: PathComparison::Exact,
            case_insensitive,
            negate,
        },
        ConditionKind::PathPrefix => UrlCondition::Path {
            value,
            comparison: PathComparison::Prefix,
            case_insensitive,
            negate,
        },
        ConditionKind::QueryKey => UrlCondition::QueryKey {
            key,
            case_insensitive,
            negate,
        },
        ConditionKind::QueryValue => UrlCondition::QueryValue {
            key,
            value,
            case_insensitive,
            negate,
        },
        ConditionKind::Glob => UrlCondition::Glob {
            value,
            case_insensitive,
            negate,
        },
        ConditionKind::Regex => UrlCondition::Regex {
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
    let Some((from, to)) = list_order::neighbor_insertion(rules.len(), index, direction) else {
        return;
    };
    if reorder_ui::move_row_to(list, &mut rules, |rule| rule.row.clone(), from, to) {
        if let Some(row) = list.selected_row() {
            row.grab_focus();
        }
    }
}

fn focus_group(group: &GroupWidgets) {
    if let Some(condition) = group.conditions.borrow().first() {
        condition.value.grab_focus();
    }
}

/// Removes the item matching `matches` from `items`, removes its widget from the container via
/// `remove_widget`, and restores keyboard focus: to the item that slid into the removed slot (or
/// the new last item) via `focus_next`, or to `focus_fallback` when the list is now empty. Shared
/// by the Routing Rule, OR group, and AND condition "Remove" handlers, which all follow this same
/// find-remove-refocus shape.
fn remove_and_refocus<T>(
    on_change: &ChangeCallback,
    items: &Rc<RefCell<Vec<T>>>,
    matches: impl Fn(&T) -> bool,
    remove_widget: impl FnOnce(&T),
    focus_next: impl FnOnce(&T),
    focus_fallback: impl FnOnce(),
) {
    let removed_index = {
        let mut items_ref = items.borrow_mut();
        let Some(index) = items_ref.iter().position(matches) else {
            return;
        };
        remove_widget(&items_ref.remove(index));
        index
    };
    let items_ref = items.borrow();
    match items_ref.get(removed_index).or_else(|| items_ref.last()) {
        Some(next) => focus_next(next),
        None => {
            drop(items_ref);
            focus_fallback();
        }
    }
    notify_change(on_change);
}

fn notify_change(on_change: &ChangeCallback) {
    if let Some(callback) = on_change.borrow().clone() {
        callback();
    }
}

fn bind_toggle(button: &gtk::CheckButton, on_change: &ChangeCallback) {
    button.connect_toggled(glib::clone!(
        #[strong]
        on_change,
        move |_| notify_change(&on_change)
    ));
}

fn entry(text: &str, label: &str, on_change: &ChangeCallback) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .text(text)
        .placeholder_text(label)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(label)]);
    entry.connect_changed(glib::clone!(
        #[strong]
        on_change,
        move |_| notify_change(&on_change)
    ));
    entry
}

fn dropdown(values: &[&str], label: &str, on_change: &ChangeCallback) -> gtk::DropDown {
    let dropdown = gtk::DropDown::from_strings(values);
    dropdown.update_property(&[gtk::accessible::Property::Label(label)]);
    dropdown.connect_selected_notify(glib::clone!(
        #[strong]
        on_change,
        move |_| notify_change(&on_change)
    ));
    dropdown
}
