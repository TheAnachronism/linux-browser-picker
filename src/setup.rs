use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use crate::application::{self, PickerSession};
use crate::configuration::{
    self, BrowserDestination, Configuration, ConfigurationStore, DestinationLaunch, FallbackAction,
    LaunchMode, RoutingRule, SaveConflictPolicy,
};
use crate::discovery::{self, BrowserCandidate};
use crate::i18n;
use crate::open_target::OpenTarget;
use crate::profiles::{self, ProfileIdentity};
use crate::routing;
use crate::routing_editor::{self, RoutingRuleEditor};

#[derive(Clone)]
struct EditorItem {
    launch: DestinationLaunch,
    application_label: String,
    profile_label: Option<String>,
    enabled: gtk::CheckButton,
    enable_allowed: bool,
    id: gtk::Entry,
    label: gtk::Entry,
    icon: gtk::Entry,
    row: gtk::ListBoxRow,
}

pub fn present(application: &adw::Application, session: PickerSession, store: ConfigurationStore) {
    let pending = Rc::clone(&session.pending);
    let existing = store.configuration.clone();
    session.set_configuration_open(true);
    if let Some(window) = application
        .windows()
        .into_iter()
        .find(|window| window.widget_name() == "destination-setup")
    {
        window.present();
        return;
    }

    let discovery = discovery::discover();
    let ordinary = discovery.ordinary.clone();
    let items = Rc::new(RefCell::new(editor_items(&existing, &ordinary)));
    let current_target = session
        .pending
        .borrow()
        .front()
        .map(|request| request.target.clone());
    let initial_destination = existing
        .as_ref()
        .and_then(|configuration| configuration.destinations.first())
        .map(|destination| destination.id.as_str());
    let rule_editor = Rc::new(RoutingRuleEditor::new(
        existing
            .as_ref()
            .map(|configuration| configuration.rules.as_slice())
            .unwrap_or_default(),
        current_target.as_ref(),
        initial_destination,
    ));
    let first_run = existing.is_none();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let heading = gtk::Label::builder()
        .label(i18n::text("Choose Browser Destinations"))
        .xalign(0.0)
        .build();
    heading.add_css_class("title-2");
    content.append(&heading);

    match &store.status {
        configuration::StoreStatus::Invalid(error) => {
            let recovery = gtk::Label::builder()
                .label(error.message())
                .xalign(0.0)
                .wrap(true)
                .build();
            recovery.add_css_class("error");
            recovery.update_property(&[gtk::accessible::Property::Description(
                "Configuration recovery error",
            )]);
            content.append(&recovery);
            let hint = gtk::Label::builder()
                .label(i18n::text(
                    "The invalid file was left unchanged. Enable destinations and save to replace it. Independently discovered browsers are listed below.",
                ))
                .xalign(0.0)
                .wrap(true)
                .build();
            content.append(&hint);
        }
        configuration::StoreStatus::Migratable(preview) => {
            let recovery = gtk::Label::builder()
                .label(preview.message())
                .xalign(0.0)
                .wrap(true)
                .build();
            recovery.update_property(&[gtk::accessible::Property::Description(
                "Configuration migration preview",
            )]);
            content.append(&recovery);
        }
        _ => {}
    }

    if let Some(target) = pending.borrow().front() {
        let waiting = gtk::Label::builder()
            .label(i18n::text(
                "An Open Target is waiting. After setup it will open in the Picker.",
            ))
            .xalign(0.0)
            .wrap(true)
            .build();
        content.append(&waiting);
        let host = gtk::Label::builder()
            .label(target.target.title())
            .xalign(0.0)
            .selectable(true)
            .build();
        host.add_css_class("heading");
        host.update_property(&[gtk::accessible::Property::Description(
            "Waiting Open Target",
        )]);
        content.append(&host);
    }

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.update_property(&[gtk::accessible::Property::Label("Browser Candidates")]);
    for item in items.borrow().iter() {
        list.append(&item.row);
    }
    if let Some(first) = items.borrow().first() {
        list.select_row(Some(&first.row));
    }
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    content.append(&scroller);

    let order = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    let move_up = gtk::Button::from_icon_name("go-up-symbolic");
    move_up.update_property(&[
        gtk::accessible::Property::Label("Move destination up"),
        gtk::accessible::Property::KeyShortcuts("<Alt><Shift>Up"),
    ]);
    let move_down = gtk::Button::from_icon_name("go-down-symbolic");
    move_down.update_property(&[
        gtk::accessible::Property::Label("Move destination down"),
        gtk::accessible::Property::KeyShortcuts("<Alt><Shift>Down"),
    ]);
    order.append(&move_up);
    order.append(&move_down);
    let refresh = gtk::Button::with_label(&i18n::text("Refresh Browser Profiles"));
    refresh.update_property(&[gtk::accessible::Property::Label("Refresh Browser Profiles")]);
    order.append(&refresh);
    content.append(&order);
    content.append(&rule_editor.root);
    let query_warning = gtk::Label::builder()
        .label(i18n::text(
            "Exact query values are stored as plain text in the configuration file. Browser Picker does not use a secret service.",
        ))
        .xalign(0.0)
        .wrap(true)
        .build();
    query_warning.add_css_class("dim-label");
    query_warning.update_property(&[gtk::accessible::Property::Description(
        "Exact query values are stored as plain text",
    )]);
    content.append(&query_warning);

    if !discovery.partial.is_empty() {
        let expander = gtk::Expander::with_mnemonic(&i18n::text("_Partial handlers"));
        let diagnostics = gtk::Box::new(gtk::Orientation::Vertical, 6);
        for handler in &discovery.partial {
            let schemes = match (handler.http, handler.https) {
                (true, false) => "HTTP",
                (false, true) => "HTTPS",
                _ => "HTTP or HTTPS",
            };
            let label = gtk::Label::builder()
                .label(i18n::text_with(
                    "Partial handler {name} advertises {schemes} only",
                    &[("{name}", &handler.candidate.name), ("{schemes}", schemes)],
                ))
                .xalign(0.0)
                .wrap(true)
                .build();
            diagnostics.append(&label);
        }
        expander.set_child(Some(&diagnostics));
        content.append(&expander);
    }

    let fallback_label = gtk::Label::builder()
        .label(i18n::text("Fallback Action"))
        .xalign(0.0)
        .build();
    content.append(&fallback_label);
    let show_picker_label = i18n::text("Show Picker");
    let fallback = gtk::DropDown::from_strings(&[&show_picker_label]);
    fallback.update_property(&[gtk::accessible::Property::Label("Fallback Action")]);
    content.append(&fallback);
    let fallback_mode_label = gtk::Label::builder()
        .label(i18n::text("Fallback Launch Mode"))
        .xalign(0.0)
        .build();
    content.append(&fallback_mode_label);
    let fallback_mode =
        gtk::DropDown::from_strings(&[&i18n::text("Normal"), &i18n::text("Private")]);
    fallback_mode.update_property(&[gtk::accessible::Property::Label("Fallback Launch Mode")]);
    content.append(&fallback_mode);

    if !store.warnings.is_empty() {
        let warning = gtk::Label::builder()
            .label(store.warnings.join("\n"))
            .xalign(0.0)
            .wrap(true)
            .build();
        warning.add_css_class("warning");
        warning.update_property(&[gtk::accessible::Property::Description(
            "Configuration permission warning",
        )]);
        content.append(&warning);
    }

    let error = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    error.add_css_class("error");
    error.set_focusable(true);
    error.update_property(&[gtk::accessible::Property::Description(
        "Browser Picker configuration error",
    )]);
    content.append(&error);
    let save_label = if current_target.is_some() {
        i18n::text("_Save and Apply")
    } else {
        i18n::text("_Save configuration")
    };
    let save = gtk::Button::with_mnemonic(&save_label);
    save.add_css_class("suggested-action");
    save.update_property(&[
        gtk::accessible::Property::Label(save_label.trim_start_matches('_')),
        gtk::accessible::Property::KeyShortcuts("<Alt>s"),
    ]);
    content.append(&save);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&content));
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(i18n::text("Browser Picker"))
        .default_width(720)
        .default_height(720)
        .content(&toolbar)
        .build();
    window.set_widget_name("destination-setup");
    let store = Rc::new(RefCell::new(store));
    let allow_close = Rc::new(Cell::new(false));
    let preferred_fallback_mode = existing
        .as_ref()
        .map(|configuration| match configuration.fallback.clone() {
            FallbackAction::Open { mode, .. } => mode,
            FallbackAction::ShowPicker => LaunchMode::Normal,
        })
        .unwrap_or(LaunchMode::Normal);
    fallback_mode.set_selected(u32::from(preferred_fallback_mode == LaunchMode::Private));

    let preferred_fallback = Rc::new(RefCell::new(existing.as_ref().and_then(|configuration| {
        match &configuration.fallback {
            FallbackAction::Open { destination, .. } => Some(destination.clone()),
            FallbackAction::ShowPicker => None,
        }
    })));
    let refresh_fallback = glib::clone!(
        #[weak]
        fallback,
        #[strong]
        items,
        #[strong]
        preferred_fallback,
        #[strong]
        fallback_mode,
        move || {
            restore_fallback(&fallback, &items.borrow(), &preferred_fallback);
            fallback_mode.set_sensitive(fallback.selected() != 0);
        }
    );
    refresh_fallback();
    fallback.connect_selected_notify(glib::clone!(
        #[strong]
        items,
        #[strong]
        preferred_fallback,
        #[weak]
        fallback_mode,
        move |fallback| {
            update_preferred_fallback(fallback, &items.borrow(), &preferred_fallback);
            update_remove_blocks(&items.borrow(), &preferred_fallback);
            fallback_mode.set_sensitive(fallback.selected() != 0);
        }
    ));

    for item in items.borrow().iter() {
        item.enabled.connect_toggled(glib::clone!(
            #[strong]
            refresh_fallback,
            move |_| refresh_fallback()
        ));
        item.id.connect_changed(glib::clone!(
            #[strong]
            refresh_fallback,
            move |_| refresh_fallback()
        ));
        item.label.connect_changed(glib::clone!(
            #[strong]
            refresh_fallback,
            move |_| refresh_fallback()
        ));
    }
    let explain = glib::clone!(
        #[weak]
        fallback,
        #[weak]
        fallback_mode,
        #[strong]
        items,
        #[strong]
        rule_editor,
        move || {
            let sample = rule_editor.sample.text();
            if sample.is_empty() {
                rule_editor
                    .explanation
                    .set_label(&i18n::text("Open an URL to test Routing Rules."));
                return;
            }
            let target = match OpenTarget::parse(OsStr::new(sample.as_str())) {
                Ok(OpenTarget::File(_)) => {
                    rule_editor
                        .explanation
                        .set_label(&routing_editor::file_explanation());
                    return;
                }
                Ok(OpenTarget::Web(target)) => target,
                Err(_) => {
                    rule_editor.explanation.set_label(&i18n::text(
                        "Invalid Open Target: expected an existing regular local file or an absolute HTTP or HTTPS URL",
                    ));
                    return;
                }
            };
            match collect_configuration(
                &items.borrow(),
                &rule_editor.rules(),
                &fallback,
                &fallback_mode,
            ) {
                Ok(configuration) => {
                    let evaluation = routing::evaluate(&configuration, &target);
                    rule_editor
                        .explanation
                        .set_label(&routing_editor::explanation_text(&evaluation));
                }
                Err(failure) => rule_editor.explanation.set_label(&failure.message()),
            }
        }
    );
    explain();
    rule_editor.test.connect_clicked(glib::clone!(
        #[strong]
        explain,
        move |_| explain()
    ));

    let move_destination = |direction: isize| {
        glib::clone!(
            #[weak]
            list,
            #[strong]
            items,
            #[strong]
            refresh_fallback,
            move |_: &gio::SimpleAction, _: Option<&glib::Variant>| {
                reorder(&list, &items, direction);
                refresh_fallback();
            }
        )
    };
    let move_destination_up = gio::SimpleAction::new("move-destination-up", None);
    move_destination_up.connect_activate(move_destination(-1));
    window.add_action(&move_destination_up);
    application.set_accels_for_action("win.move-destination-up", &["<Alt><Shift>Up"]);
    let move_destination_down = gio::SimpleAction::new("move-destination-down", None);
    move_destination_down.connect_activate(move_destination(1));
    window.add_action(&move_destination_down);
    application.set_accels_for_action("win.move-destination-down", &["<Alt><Shift>Down"]);
    move_up.connect_clicked(glib::clone!(
        #[strong]
        move_destination_up,
        move |_| {
            move_destination_up.activate(None);
        }
    ));
    move_down.connect_clicked(glib::clone!(
        #[strong]
        move_destination_down,
        move |_| {
            move_destination_down.activate(None);
        }
    ));

    refresh.connect_clicked(glib::clone!(
        #[weak]
        list,
        #[strong]
        items,
        #[strong]
        ordinary,
        #[strong]
        refresh_fallback,
        move |_| {
            refresh_profiles(&list, &items, &ordinary);
            refresh_fallback();
        }
    ));

    save.connect_clicked(glib::clone!(
        #[weak]
        application,
        #[weak]
        window,
        #[weak]
        fallback,
        #[weak]
        fallback_mode,
        #[weak]
        error,
        #[strong]
        items,
        #[strong]
        rule_editor,
        #[strong]
        session,
        #[strong]
        store,
        #[strong]
        allow_close,
        move |_| {
            save_destinations(
                &application,
                &window,
                &fallback,
                &fallback_mode,
                &error,
                &items,
                &rule_editor.rules(),
                &session,
                &store,
                &allow_close,
                SaveConflictPolicy::Abort,
                false,
            );
        }
    ));
    let save_action = gio::SimpleAction::new("save-destinations", None);
    save_action.connect_activate(glib::clone!(
        #[weak]
        application,
        #[weak]
        window,
        #[weak]
        fallback,
        #[weak]
        fallback_mode,
        #[weak]
        error,
        #[strong]
        items,
        #[strong]
        rule_editor,
        #[strong]
        session,
        #[strong]
        store,
        #[strong]
        allow_close,
        move |_, _| {
            save_destinations(
                &application,
                &window,
                &fallback,
                &fallback_mode,
                &error,
                &items,
                &rule_editor.rules(),
                &session,
                &store,
                &allow_close,
                SaveConflictPolicy::Abort,
                false,
            );
        }
    ));
    window.add_action(&save_action);
    application.set_accels_for_action("win.save-destinations", &["<Alt>s"]);
    save.set_receives_default(true);
    window.set_default_widget(Some(&save));
    let close = gio::SimpleAction::new("close", None);
    close.connect_activate(glib::clone!(
        #[weak]
        window,
        move |_, _| window.close()
    ));
    window.add_action(&close);
    application.set_accels_for_action("win.close", &["<Ctrl>w"]);

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let application_weak = application.downgrade();
    let window_weak = window.downgrade();
    let fallback_weak = fallback.downgrade();
    let fallback_mode_weak = fallback_mode.downgrade();
    let error_weak = error.downgrade();
    let key_items = Rc::clone(&items);
    let key_rule_editor = Rc::clone(&rule_editor);
    let key_session = session.clone();
    let key_store = Rc::clone(&store);
    let key_allow_close = Rc::clone(&allow_close);
    rule_editor.connect_order_shortcuts(&window);
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let (Some(application), Some(window), Some(fallback), Some(fallback_mode), Some(error)) = (
            application_weak.upgrade(),
            window_weak.upgrade(),
            fallback_weak.upgrade(),
            fallback_mode_weak.upgrade(),
            error_weak.upgrade(),
        ) else {
            return glib::Propagation::Proceed;
        };
        if modifiers.contains(gdk::ModifierType::ALT_MASK)
            && !modifiers.contains(gdk::ModifierType::CONTROL_MASK)
            && (key == gdk::Key::s || key == gdk::Key::S)
        {
            save_destinations(
                &application,
                &window,
                &fallback,
                &fallback_mode,
                &error,
                &key_items,
                &key_rule_editor.rules(),
                &key_session,
                &key_store,
                &key_allow_close,
                SaveConflictPolicy::Abort,
                false,
            );
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);

    window.connect_close_request(glib::clone!(
        #[weak]
        application,
        #[weak]
        fallback,
        #[weak]
        fallback_mode,
        #[weak]
        error,
        #[strong]
        items,
        #[strong]
        rule_editor,
        #[strong]
        session,
        #[strong]
        store,
        #[strong]
        allow_close,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |window| {
            if allow_close.get() {
                session.resume_picker(&application);
                return glib::Propagation::Proceed;
            }
            let prompt = match collect_configuration(
                &items.borrow(),
                &rule_editor.rules(),
                &fallback,
                &fallback_mode,
            ) {
                Ok(draft) => store
                    .borrow()
                    .configuration
                    .as_ref()
                    .is_none_or(|saved| !configuration::semantically_equal(saved, &draft)),
                Err(_) => true,
            };
            if !prompt {
                session.resume_picker(&application);
                return glib::Propagation::Proceed;
            }
            let dialog = adw::AlertDialog::new(
                Some(&i18n::text("Unsaved changes")),
                Some(&i18n::text("Save the configuration draft before closing?")),
            );
            dialog.add_responses(&[
                ("cancel", &i18n::text("_Cancel")),
                ("discard", &i18n::text("_Discard")),
                ("save", &i18n::text("_Save")),
            ]);
            dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("save"));
            dialog.set_close_response("cancel");
            dialog.connect_response(
                None,
                glib::clone!(
                    #[weak]
                    application,
                    #[weak]
                    window,
                    #[weak]
                    fallback,
                    #[weak]
                    fallback_mode,
                    #[weak]
                    error,
                    #[strong]
                    items,
                    #[strong]
                    rule_editor,
                    #[strong]
                    session,
                    #[strong]
                    store,
                    #[strong]
                    allow_close,
                    move |_, response| {
                        match response {
                            "discard" => {
                                allow_close.set(true);
                                session.set_configuration_open(false);
                                window.close();
                            }
                            "save" => {
                                save_destinations(
                                    &application,
                                    &window,
                                    &fallback,
                                    &fallback_mode,
                                    &error,
                                    &items,
                                    &rule_editor.rules(),
                                    &session,
                                    &store,
                                    &allow_close,
                                    SaveConflictPolicy::Abort,
                                    true,
                                );
                            }
                            _ => {}
                        }
                    }
                ),
            );
            dialog.present(Some(window));
            glib::Propagation::Stop
        }
    ));

    window.present();
    if first_run && let Some(first) = items.borrow().first() {
        let enable = first.enabled.clone();
        glib::idle_add_local_once(move || {
            enable.grab_focus();
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn save_destinations(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    fallback: &gtk::DropDown,
    fallback_mode: &gtk::DropDown,
    error: &gtk::Label,
    items: &Rc<RefCell<Vec<EditorItem>>>,
    rules: &[RoutingRule],
    session: &PickerSession,
    store: &Rc<RefCell<ConfigurationStore>>,
    allow_close: &Rc<Cell<bool>>,
    policy: SaveConflictPolicy,
    close_after: bool,
) {
    match collect_configuration(&items.borrow(), rules, fallback, fallback_mode) {
        Ok(configuration) => match store.borrow_mut().save(&configuration, policy) {
            Ok(result) => {
                let mut messages = Vec::new();
                if let Some(path) = result.backup_path {
                    messages.push(i18n::text_with(
                        "Saved. A backup of the external file is at {path}",
                        &[("{path}", &path.display().to_string())],
                    ));
                }
                messages.extend(result.warnings);
                if messages.is_empty() {
                    error.set_visible(false);
                } else {
                    error.set_label(&messages.join("\n"));
                    error.set_visible(true);
                }
                if session.pending.borrow().front().is_some() {
                    allow_close.set(true);
                    session.set_configuration_open(false);
                    match application::reapply_front(application, session) {
                        Ok(()) => window.close(),
                        Err((message, _)) => {
                            allow_close.set(false);
                            session.set_configuration_open(true);
                            error.set_label(&message);
                            error.set_visible(true);
                            error.grab_focus();
                        }
                    }
                } else if close_after {
                    allow_close.set(true);
                    session.set_configuration_open(false);
                    window.close();
                } else {
                    window.present();
                }
            }
            Err(configuration::Error::Conflict) if policy == SaveConflictPolicy::Abort => {
                prompt_overwrite(
                    application,
                    window,
                    fallback,
                    fallback_mode,
                    error,
                    items,
                    rules,
                    session,
                    store,
                    allow_close,
                    close_after,
                );
            }
            Err(failure) => {
                error.set_label(&failure.message());
                error.set_visible(true);
                error.grab_focus();
            }
        },
        Err(failure) => {
            error.set_label(&failure.message());
            error.set_visible(true);
            error.grab_focus();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn prompt_overwrite(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    fallback: &gtk::DropDown,
    fallback_mode: &gtk::DropDown,
    error: &gtk::Label,
    items: &Rc<RefCell<Vec<EditorItem>>>,
    rules: &[RoutingRule],
    session: &PickerSession,
    store: &Rc<RefCell<ConfigurationStore>>,
    allow_close: &Rc<Cell<bool>>,
    close_after: bool,
) {
    let dialog = adw::AlertDialog::new(
        Some(&i18n::text("Configuration changed on disk")),
        Some(&i18n::text(
            "Another editor replaced this file. Overwrite after creating an owner-only backup?",
        )),
    );
    dialog.add_responses(&[
        ("cancel", &i18n::text("_Cancel")),
        ("overwrite", &i18n::text("_Overwrite")),
    ]);
    dialog.set_response_appearance("overwrite", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let rules = rules.to_vec();
    dialog.connect_response(
        Some("overwrite"),
        glib::clone!(
            #[weak]
            application,
            #[weak]
            window,
            #[weak]
            fallback,
            #[weak]
            fallback_mode,
            #[weak]
            error,
            #[strong]
            items,
            #[strong]
            session,
            #[strong]
            store,
            #[strong]
            allow_close,
            move |_, _| {
                save_destinations(
                    &application,
                    &window,
                    &fallback,
                    &fallback_mode,
                    &error,
                    &items,
                    &rules,
                    &session,
                    &store,
                    &allow_close,
                    SaveConflictPolicy::Overwrite,
                    close_after,
                );
            }
        ),
    );
    dialog.present(Some(window));
}

fn editor_items(
    existing: &Option<Configuration>,
    ordinary: &[BrowserCandidate],
) -> Vec<EditorItem> {
    let mut used_ids = HashSet::new();
    let mut items = Vec::new();
    let mut configured_desktop_ids = HashSet::new();
    let mut configured_profiles = HashSet::new();

    if let Some(configuration) = existing {
        for destination in &configuration.destinations {
            used_ids.insert(destination.id.clone());
            if let DestinationLaunch::Discovered { desktop_id } = &destination.launch {
                configured_desktop_ids.insert(desktop_id.clone());
            }
            if let Some(key) = profile_key(&destination.launch) {
                configured_profiles.insert(key);
            }
            items.push(item_from_destination(destination));
        }
    }

    for candidate in ordinary {
        if !configured_desktop_ids.contains(&candidate.desktop_id) {
            let slug = discovery::unique_slug(
                &discovery::suggested_slug(&candidate.desktop_id),
                &mut used_ids,
            );
            items.push(item_from_candidate(candidate, slug));
        }
        append_profile_candidates(candidate, &mut items, &mut used_ids, &configured_profiles);
    }
    items
}

fn refresh_profiles(
    list: &gtk::ListBox,
    items: &Rc<RefCell<Vec<EditorItem>>>,
    ordinary: &[BrowserCandidate],
) {
    let mut items = items.borrow_mut();
    let mut used_ids: HashSet<String> = items
        .iter()
        .map(|item| item.id.text().to_string())
        .collect();
    let configured_profiles: HashSet<_> = items
        .iter()
        .filter_map(|item| profile_key(&item.launch))
        .collect();
    let start = items.len();
    for candidate in ordinary {
        append_profile_candidates(candidate, &mut items, &mut used_ids, &configured_profiles);
    }
    for item in items.iter().skip(start) {
        list.append(&item.row);
    }
}

fn append_profile_candidates(
    candidate: &BrowserCandidate,
    items: &mut Vec<EditorItem>,
    used_ids: &mut HashSet<String>,
    configured_profiles: &HashSet<(String, ProfileIdentity)>,
) {
    let Some(application) = discovery::app_info(&candidate.desktop_id) else {
        return;
    };
    let executable = gtk::gio::prelude::AppInfoExt::executable(&application);
    let Some(assumptions) = profiles::classify(
        &candidate.desktop_id,
        &executable,
        &profiles::DiscoveryPaths::from_env(),
    ) else {
        return;
    };
    for profile in profiles::discover(&assumptions) {
        let key = identity_key(&candidate.desktop_id, &profile.identity);
        if configured_profiles.contains(&key) {
            continue;
        }
        let slug_base = format!(
            "{}-{}",
            discovery::suggested_slug(&candidate.desktop_id),
            discovery::suggested_slug(&profile.display_name)
        );
        let slug = discovery::unique_slug(&slug_base, used_ids);
        let label = format!("{} — {}", candidate.name, profile.display_name);
        items.push(item_from_profile(
            candidate,
            slug,
            label,
            profile.display_name,
            launch_from_identity(&candidate.desktop_id, profile.identity.clone()),
            &profile.identity,
        ));
    }
}

fn item_from_destination(destination: &BrowserDestination) -> EditorItem {
    let identity = destination.launch.profile_identity();
    let (assumptions, enable_allowed) = match (destination.desktop_id(), identity.as_ref()) {
        (Some(desktop_id), Some(identity)) => assumptions_for(desktop_id, identity),
        _ => (None, true),
    };
    build_item(
        destination.launch.clone(),
        destination.application_label.clone(),
        destination.profile_label.clone(),
        true,
        &destination.id,
        &destination.label,
        destination.icon_name.as_deref().unwrap_or(""),
        &destination.label,
        None,
        assumptions,
        enable_allowed,
    )
}

fn item_from_candidate(candidate: &BrowserCandidate, slug: String) -> EditorItem {
    build_item(
        DestinationLaunch::Discovered {
            desktop_id: candidate.desktop_id.clone(),
        },
        candidate.name.clone(),
        None,
        false,
        &slug,
        &candidate.name,
        "",
        &candidate.name,
        candidate.icon.clone(),
        None,
        true,
    )
}

fn item_from_profile(
    candidate: &BrowserCandidate,
    slug: String,
    label: String,
    profile_label: String,
    launch: DestinationLaunch,
    identity: &ProfileIdentity,
) -> EditorItem {
    let (assumptions, enable_allowed) = assumptions_for(&candidate.desktop_id, identity);
    build_item(
        launch,
        candidate.name.clone(),
        Some(profile_label),
        false,
        &slug,
        &label,
        "",
        &label,
        candidate.icon.clone(),
        assumptions,
        enable_allowed,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_item(
    launch: DestinationLaunch,
    application_label: String,
    profile_label: Option<String>,
    enabled: bool,
    id: &str,
    label: &str,
    icon: &str,
    name: &str,
    candidate_icon: Option<gio::Icon>,
    assumptions: Option<String>,
    enable_allowed: bool,
) -> EditorItem {
    let enable = gtk::CheckButton::with_label(name);
    enable.set_active(enabled);
    enable.update_property(&[gtk::accessible::Property::Label(&i18n::text_with(
        "Enable Browser Candidate {name}",
        &[("{name}", name)],
    ))]);
    if !enable_allowed && !enabled {
        enable.set_sensitive(false);
        enable.set_tooltip_text(Some(&i18n::text(
            "Family assumptions must be valid before a profiled destination can be enabled",
        )));
    }

    let id_entry = gtk::Entry::builder()
        .text(id)
        .placeholder_text(i18n::text("Browser Destination ID"))
        .build();
    id_entry.update_property(&[gtk::accessible::Property::Label("Browser Destination ID")]);
    let label_entry = gtk::Entry::builder()
        .text(label)
        .placeholder_text(i18n::text("Browser Destination display label"))
        .build();
    label_entry.update_property(&[gtk::accessible::Property::Label(
        "Browser Destination display label",
    )]);
    let icon_entry = gtk::Entry::builder()
        .text(icon)
        .placeholder_text(i18n::text("Icon override"))
        .build();
    icon_entry.update_property(&[gtk::accessible::Property::Label("Icon override")]);

    let fields = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    fields.append(&id_entry);
    fields.append(&label_entry);
    fields.append(&icon_entry);

    let icon_image = if let Some(icon) = candidate_icon {
        gtk::Image::from_gicon(&icon)
    } else {
        match &launch {
            DestinationLaunch::Discovered { desktop_id }
            | DestinationLaunch::FirefoxProfile { desktop_id, .. }
            | DestinationLaunch::ChromiumProfile { desktop_id, .. } => discovery::icon(desktop_id)
                .map(|icon| gtk::Image::from_gicon(&icon))
                .unwrap_or_else(|| gtk::Image::from_icon_name("web-browser")),
            DestinationLaunch::Manual { .. } => gtk::Image::from_icon_name("web-browser"),
        }
    };
    icon_image.set_pixel_size(32);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    header.append(&icon_image);
    header.append(&enable);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.set_margin_top(9);
    body.set_margin_bottom(9);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.append(&header);
    body.append(&fields);
    if let Some(assumptions) = assumptions {
        let details = gtk::Label::builder()
            .label(assumptions)
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        let assumptions_description = i18n::text("Family assumptions for this Browser Profile");
        details.update_property(&[gtk::accessible::Property::Description(
            assumptions_description.as_str(),
        )]);
        body.append(&details);
    }

    let row = gtk::ListBoxRow::builder()
        .child(&body)
        .activatable(false)
        .selectable(true)
        .build();
    row.update_property(&[gtk::accessible::Property::Label(name)]);

    EditorItem {
        launch,
        application_label,
        profile_label,
        enabled: enable,
        enable_allowed,
        id: id_entry,
        label: label_entry,
        icon: icon_entry,
        row,
    }
}

fn restore_fallback(
    fallback: &gtk::DropDown,
    items: &[EditorItem],
    preferred: &RefCell<Option<String>>,
) {
    let mut labels = vec![i18n::text("Show Picker")];
    let mut ids = Vec::new();
    for item in items {
        if !item.enabled.is_active() {
            continue;
        }
        let label = item.label.text();
        labels.push(i18n::text_with(
            "Open {label}",
            &[("{label}", label.as_str())],
        ));
        ids.push(item.id.text().to_string());
    }
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    fallback.set_model(Some(&gtk::StringList::new(&label_refs)));

    let selected = preferred
        .borrow()
        .as_ref()
        .and_then(|destination| ids.iter().position(|id| id == destination))
        .map(|index| index + 1)
        .unwrap_or(0);
    fallback.set_selected(selected as u32);
    update_preferred_fallback(fallback, items, preferred);
    update_remove_blocks(items, preferred);
}

fn update_preferred_fallback(
    fallback: &gtk::DropDown,
    items: &[EditorItem],
    preferred: &RefCell<Option<String>>,
) {
    let selected = fallback.selected() as usize;
    *preferred.borrow_mut() = if selected == 0 {
        None
    } else {
        items
            .iter()
            .filter(|item| item.enabled.is_active())
            .nth(selected.saturating_sub(1))
            .map(|item| item.id.text().to_string())
    };
}

fn update_remove_blocks(items: &[EditorItem], preferred: &RefCell<Option<String>>) {
    let referenced = preferred.borrow();
    for item in items {
        let blocked = referenced
            .as_deref()
            .is_some_and(|destination| item.id.text().as_str() == destination);
        let can_toggle = if blocked {
            false
        } else if item.enabled.is_active() {
            true
        } else {
            item.enable_allowed
        };
        item.enabled.set_sensitive(can_toggle);
        if blocked {
            item.enabled.set_tooltip_text(Some(&i18n::text(
                "Cannot remove a destination referenced by the Fallback Action",
            )));
        } else if !item.enable_allowed && !item.enabled.is_active() {
            item.enabled.set_tooltip_text(Some(&i18n::text(
                "Family assumptions must be valid before a profiled destination can be enabled",
            )));
        } else {
            item.enabled.set_tooltip_text(None);
        }
    }
}

fn reorder(list: &gtk::ListBox, items: &Rc<RefCell<Vec<EditorItem>>>, direction: isize) {
    let Some(selected) = list.selected_row() else {
        return;
    };
    let mut items = items.borrow_mut();
    let Some(index) = items.iter().position(|item| item.row == selected) else {
        return;
    };
    let next = index as isize + direction;
    if next < 0 || next >= items.len() as isize {
        return;
    }
    let next = next as usize;
    items.swap(index, next);
    let row = items[next].row.clone();
    list.remove(&row);
    list.insert(&row, next as i32);
    list.select_row(Some(&row));
}

fn collect_configuration(
    items: &[EditorItem],
    rules: &[RoutingRule],
    fallback: &gtk::DropDown,
    fallback_mode: &gtk::DropDown,
) -> Result<Configuration, configuration::Error> {
    let mut destinations = Vec::new();
    let mut enabled_ids = Vec::new();
    for item in items {
        if !item.enabled.is_active() {
            continue;
        }
        let icon = item.icon.text();
        let destination = BrowserDestination {
            id: item.id.text().to_string(),
            label: item.label.text().to_string(),
            application_label: item.application_label.clone(),
            profile_label: item.profile_label.clone(),
            icon_name: if icon.is_empty() {
                None
            } else {
                Some(icon.to_string())
            },
            launch: item.launch.clone(),
            unavailable_reason: None,
        };
        enabled_ids.push(destination.id.clone());
        destinations.push(destination);
    }

    let selected = fallback.selected() as usize;
    let fallback = if selected == 0 {
        FallbackAction::ShowPicker
    } else {
        let Some(id) = enabled_ids.get(selected - 1) else {
            return Err(configuration::Error::UnknownFallback(String::new()));
        };
        let mode = if fallback_mode.selected() == 1 {
            LaunchMode::Private
        } else {
            LaunchMode::Normal
        };
        FallbackAction::Open {
            destination: id.clone(),
            mode,
        }
    };
    configuration::assemble(destinations, rules.to_vec(), fallback)
}

fn profile_key(launch: &DestinationLaunch) -> Option<(String, ProfileIdentity)> {
    let desktop_id = launch.desktop_id()?.to_owned();
    Some((desktop_id, launch.profile_identity()?))
}

fn identity_key(desktop_id: &str, identity: &ProfileIdentity) -> (String, ProfileIdentity) {
    (desktop_id.to_owned(), identity.clone())
}

fn launch_from_identity(desktop_id: &str, identity: ProfileIdentity) -> DestinationLaunch {
    match identity {
        ProfileIdentity::Firefox { name, path } => DestinationLaunch::FirefoxProfile {
            desktop_id: desktop_id.to_owned(),
            name,
            path: path.to_string_lossy().into_owned(),
        },
        ProfileIdentity::Chromium {
            user_data_dir,
            profile_directory,
        } => DestinationLaunch::ChromiumProfile {
            desktop_id: desktop_id.to_owned(),
            user_data_dir: user_data_dir.to_string_lossy().into_owned(),
            profile_directory,
        },
    }
}

fn assumptions_for(desktop_id: &str, identity: &ProfileIdentity) -> (Option<String>, bool) {
    let executable = discovery::app_info(desktop_id)
        .map(|application| gtk::gio::prelude::AppInfoExt::executable(&application));
    let Some(executable) = executable else {
        return (
            Some(i18n::text(
                "Browser Application is not installed. This profile cannot be enabled as a verified destination.",
            )),
            false,
        );
    };
    let Some(assumptions) = profiles::classify(
        desktop_id,
        &executable,
        &profiles::DiscoveryPaths::from_env(),
    ) else {
        return (
            Some(i18n::text(
                "Unknown packaging or unsupported capability. This remains a generic Browser Application rather than a verified profile destination.",
            )),
            false,
        );
    };
    let locator = match identity {
        ProfileIdentity::Firefox { name, path } => format!("{} ({name})", path.display()),
        ProfileIdentity::Chromium {
            user_data_dir,
            profile_directory,
        } => format!("{} / {profile_directory}", user_data_dir.display()),
    };
    let text = i18n::text_with(
        "{product} family\nExecutable: {executable}\nProfile: {profile}\nPrivate flag: {flag}\nLaunch reuses the existing browser process for this profile. Browser Picker will not create, rename, delete, clone, or repair it.",
        &[
            ("{product}", &assumptions.product),
            ("{executable}", &executable.display().to_string()),
            ("{profile}", &locator),
            ("{flag}", assumptions.private_flag),
        ],
    );
    (Some(text), profiles::is_present(identity))
}
