use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use crate::application::{self, PickerSession};
use crate::associations;
use crate::configuration::{
    self, BrowserDestination, Configuration, ConfigurationStore, DestinationLaunch, FallbackAction,
    LaunchMode, RoutingRule, SaveConflictPolicy,
};
use crate::discovery::{self, BrowserCandidate};
use crate::i18n;
use crate::list_order;
use crate::open_target::OpenTarget;
use crate::profiles::{self, ProfileCapability, ProfileIdentity};
use crate::reorder_ui;
use crate::routing;
use crate::routing_editor::{self, RoutingRuleEditor};

#[derive(Clone)]
struct ManualEditor {
    root: gtk::Box,
    application_label: gtk::Entry,
    executable: gtk::Entry,
    arguments: gtk::TextView,
    private_arguments: gtk::TextView,
}

#[derive(Clone)]
struct EditorItem {
    launch: DestinationLaunch,
    application_label: String,
    manual: Option<ManualEditor>,
    profile_label: Option<String>,
    enabled: gtk::CheckButton,
    enable_allowed: bool,
    id: gtk::Entry,
    label: gtk::Entry,
    icon: gtk::Entry,
    drag_handle: gtk::Widget,
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

    let mut retry_button = None;
    match &store.status {
        configuration::StoreStatus::Invalid(error) => {
            let recovery = gtk::Label::builder()
                .label(error.message())
                .xalign(0.0)
                .wrap(true)
                .selectable(true)
                .build();
            recovery.add_css_class("error");
            recovery.update_property(&[gtk::accessible::Property::Description(&i18n::text(
                "Configuration recovery error",
            ))]);
            content.append(&recovery);
            let hint = gtk::Label::builder()
                .label(i18n::text(
                    "The invalid or unusable configuration path was left unchanged. Retry after repairing ownership, permissions, or the symlink, or enable destinations and save to replace a readable invalid file. Independently discovered browsers are listed below.",
                ))
                .xalign(0.0)
                .wrap(true)
                .build();
            content.append(&hint);
            let retry = gtk::Button::with_mnemonic(&i18n::text("_Retry"));
            retry.update_property(&[
                gtk::accessible::Property::Label(&i18n::text("Retry")),
                gtk::accessible::Property::KeyShortcuts("<Alt>r"),
            ]);
            content.append(&retry);
            retry_button = Some(retry);
        }
        configuration::StoreStatus::Migratable(preview) => {
            let recovery = gtk::Label::builder()
                .label(preview.message())
                .xalign(0.0)
                .wrap(true)
                .build();
            recovery.update_property(&[gtk::accessible::Property::Description(&i18n::text(
                "Configuration migration preview",
            ))]);
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
        host.update_property(&[gtk::accessible::Property::Description(&i18n::text(
            "Waiting Open Target",
        ))]);
        content.append(&host);
    }

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Browser Candidates",
    ))]);
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
        gtk::accessible::Property::Label(&i18n::text("Move destination up")),
        gtk::accessible::Property::KeyShortcuts("<Alt><Shift>Up"),
    ]);
    let move_down = gtk::Button::from_icon_name("go-down-symbolic");
    move_down.update_property(&[
        gtk::accessible::Property::Label(&i18n::text("Move destination down")),
        gtk::accessible::Property::KeyShortcuts("<Alt><Shift>Down"),
    ]);
    order.append(&move_up);
    order.append(&move_down);
    let add_manual = gtk::Button::with_label(&i18n::text("Add Manual Browser Application"));
    add_manual.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Add Manual Browser Application",
    ))]);
    order.append(&add_manual);
    let refresh = gtk::Button::with_label(&i18n::text("Refresh Browser Profiles"));
    refresh.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Refresh Browser Profiles",
    ))]);
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
    query_warning.update_property(&[gtk::accessible::Property::Description(&i18n::text(
        "Exact query values are stored as plain text",
    ))]);
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
    fallback.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Fallback Action",
    ))]);
    fallback.set_focusable(true);
    content.append(&fallback);
    let fallback_mode_label = gtk::Label::builder()
        .label(i18n::text("Fallback Launch Mode"))
        .xalign(0.0)
        .build();
    content.append(&fallback_mode_label);
    let fallback_mode =
        gtk::DropDown::from_strings(&[&i18n::text("Normal"), &i18n::text("Private")]);
    fallback_mode.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Fallback Launch Mode",
    ))]);
    content.append(&fallback_mode);

    if !store.warnings.is_empty() {
        let warning = gtk::Label::builder()
            .label(store.warnings.join("\n"))
            .xalign(0.0)
            .wrap(true)
            .build();
        warning.add_css_class("warning");
        warning.update_property(&[gtk::accessible::Property::Description(&i18n::text(
            "Configuration permission warning",
        ))]);
        content.append(&warning);
    }

    append_desktop_defaults(&content);

    let error = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    error.add_css_class("error");
    error.set_focusable(true);
    error.update_property(&[gtk::accessible::Property::Description(&i18n::text(
        "Browser Picker configuration error",
    ))]);
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

    let page = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(i18n::text("Browser Picker"))
        .default_width(720)
        .default_height(720)
        .content(&toolbar)
        .build();
    application::apply_window_state(&window, "setup");
    window.set_widget_name("destination-setup");
    let store = Rc::new(RefCell::new(store));
    let allow_close = Rc::new(Cell::new(false));
    if let Some(retry) = retry_button {
        retry.connect_clicked(glib::clone!(
            #[weak]
            application,
            #[weak]
            window,
            #[strong]
            session,
            #[strong]
            allow_close,
            move |_| {
                allow_close.set(true);
                window.set_widget_name("destination-setup-closing");
                window.close();
                crate::application::present_setup(&application, &session);
            }
        ));
    }
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
    let restoring_fallback = Rc::new(Cell::new(false));
    let refresh_references = glib::clone!(
        #[weak]
        fallback,
        #[weak]
        fallback_mode,
        #[weak]
        error,
        #[strong]
        items,
        #[strong]
        preferred_fallback,
        #[strong]
        rule_editor,
        move || {
            update_remove_blocks(
                &items.borrow(),
                &preferred_fallback,
                &rule_editor.referenced_destination_ids(),
            );
            fallback_mode.set_sensitive(fallback.selected() != 0);
            show_draft_status(
                &error,
                &items.borrow(),
                &rule_editor.rules(),
                &fallback,
                &fallback_mode,
            );
        }
    );
    let refresh_fallback = glib::clone!(
        #[weak]
        fallback,
        #[strong]
        items,
        #[strong]
        preferred_fallback,
        #[strong]
        rule_editor,
        #[strong]
        refresh_references,
        #[strong]
        restoring_fallback,
        move || {
            restoring_fallback.set(true);
            restore_fallback(
                &fallback,
                &items.borrow(),
                &preferred_fallback,
                &rule_editor.referenced_destination_ids(),
            );
            restoring_fallback.set(false);
            refresh_references();
        }
    );
    let on_drop: Rc<dyn Fn(usize, usize)> = Rc::new(glib::clone!(
        #[weak]
        list,
        #[strong]
        items,
        #[strong]
        refresh_fallback,
        move |from, to| {
            if reorder_ui::move_row_to(
                &list,
                &mut items.borrow_mut(),
                |item| item.row.clone(),
                from,
                to,
            ) {
                refresh_fallback();
            }
        }
    ));
    for item in items.borrow().iter() {
        reorder_ui::attach_row_drag(&item.row, &item.drag_handle, Rc::clone(&on_drop));
    }
    refresh_fallback();
    fallback.connect_selected_notify(glib::clone!(
        #[strong]
        items,
        #[strong]
        preferred_fallback,
        #[strong]
        refresh_references,
        #[strong]
        restoring_fallback,
        move |fallback| {
            if restoring_fallback.get() {
                return;
            }
            update_preferred_fallback(fallback, &items.borrow(), &preferred_fallback);
            refresh_references();
        }
    ));
    rule_editor.connect_changed(glib::clone!(
        #[strong]
        refresh_references,
        move || refresh_references()
    ));

    for item in items.borrow().iter() {
        bind_destination_item(
            item,
            &rule_editor,
            &preferred_fallback,
            refresh_fallback.clone(),
        );
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
        refresh_fallback,
        #[strong]
        on_drop,
        move |_| {
            let ordinary = discovery::discover().ordinary;
            refresh_profiles(&list, &items, &ordinary, &on_drop);
            refresh_fallback();
        }
    ));
    add_manual.connect_clicked(glib::clone!(
        #[weak]
        list,
        #[strong]
        items,
        #[strong]
        rule_editor,
        #[strong]
        preferred_fallback,
        #[strong]
        refresh_fallback,
        #[strong]
        on_drop,
        move |_| {
            let mut used_ids = items
                .borrow()
                .iter()
                .map(|item| item.id.text().to_string())
                .collect();
            let slug = discovery::unique_slug("manual-browser", &mut used_ids);
            let item = item_from_manual(slug);
            bind_destination_item(
                &item,
                &rule_editor,
                &preferred_fallback,
                refresh_fallback.clone(),
            );
            reorder_ui::attach_row_drag(&item.row, &item.drag_handle, Rc::clone(&on_drop));
            list.append(&item.row);
            list.select_row(Some(&item.row));
            item.id.grab_focus();
            items.borrow_mut().push(item);
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
    if let Some(first) = items.borrow().first() {
        let enable = first.enabled.clone();
        let id = first.id.clone();
        glib::idle_add_local_once(move || {
            if first_run {
                enable.grab_focus();
            } else {
                id.grab_focus();
            }
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

fn append_desktop_defaults(content: &gtk::Box) {
    let heading = gtk::Label::builder()
        .label(i18n::text("Desktop defaults"))
        .xalign(0.0)
        .build();
    heading.add_css_class("heading");
    heading.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Desktop defaults",
    ))]);
    content.append(&heading);

    let report = associations::report();
    for (kind, line) in ["http", "https", "html", "xhtml"]
        .into_iter()
        .zip(report.lines())
    {
        let status = gtk::Label::builder()
            .label(&line)
            .xalign(0.0)
            .wrap(true)
            .build();
        status.set_widget_name(&format!("desktop-default-{kind}"));
        status.update_property(&[gtk::accessible::Property::Label(line.as_str())]);
        content.append(&status);
    }

    let instructions = gtk::Label::builder()
        .label(associations::instructions())
        .xalign(0.0)
        .wrap(true)
        .build();
    instructions.add_css_class("dim-label");
    instructions.update_property(&[gtk::accessible::Property::Description(&i18n::text(
        "Desktop default association instructions",
    ))]);
    content.append(&instructions);
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
    on_drop: &Rc<dyn Fn(usize, usize)>,
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
        reorder_ui::attach_row_drag(&item.row, &item.drag_handle, Rc::clone(on_drop));
        list.append(&item.row);
    }
}

fn append_profile_candidates(
    candidate: &BrowserCandidate,
    items: &mut Vec<EditorItem>,
    used_ids: &mut HashSet<String>,
    configured_profiles: &HashSet<(String, ProfileIdentity)>,
) {
    let Some(application) = discovery::application(&candidate.desktop_id) else {
        return;
    };
    let Some(assumptions) = profiles::classify(
        &candidate.desktop_id,
        &application.executable,
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

fn item_from_manual(slug: String) -> EditorItem {
    build_item(
        DestinationLaunch::Manual {
            executable: String::new(),
            arguments: vec!["{target}".to_owned()],
            private_arguments: None,
        },
        i18n::text("Manual Browser Application"),
        None,
        true,
        &slug,
        &i18n::text("Manual Browser"),
        "",
        &i18n::text("Manual Browser"),
        None,
        None,
        true,
    )
}

fn argument_editor(label: &str, arguments: &[String]) -> (gtk::Box, gtk::TextView) {
    let heading = gtk::Label::builder().label(label).xalign(0.0).build();
    let editor = gtk::TextView::new();
    editor.set_monospace(true);
    editor.set_wrap_mode(gtk::WrapMode::None);
    editor.set_accepts_tab(false);
    editor.buffer().set_text(&arguments.join("\n"));
    editor.update_property(&[gtk::accessible::Property::Label(label)]);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&editor)
        .min_content_height(72)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    let group = gtk::Box::new(gtk::Orientation::Vertical, 3);
    group.append(&heading);
    group.append(&scroller);
    (group, editor)
}

fn manual_editor(launch: &DestinationLaunch, application_label: &str) -> Option<ManualEditor> {
    let DestinationLaunch::Manual {
        executable,
        arguments,
        private_arguments,
    } = launch
    else {
        return None;
    };
    let application_label_name = i18n::text("Browser Application label");
    let application_label_entry = gtk::Entry::builder()
        .text(application_label)
        .placeholder_text(&application_label_name)
        .build();
    application_label_entry
        .update_property(&[gtk::accessible::Property::Label(&application_label_name)]);
    let executable_name = i18n::text("Manual executable");
    let executable_entry = gtk::Entry::builder()
        .text(executable)
        .placeholder_text(i18n::text("Absolute executable or PATH name"))
        .build();
    executable_entry.update_property(&[gtk::accessible::Property::Label(&executable_name)]);
    let normal_arguments_label = i18n::text("Normal literal arguments");
    let (arguments_group, arguments) =
        argument_editor(&normal_arguments_label, arguments.as_slice());
    let private_arguments_label = i18n::text("Private literal arguments");
    let (private_arguments_group, private_arguments) = argument_editor(
        &private_arguments_label,
        private_arguments.as_deref().unwrap_or_default(),
    );
    let guidance = gtk::Label::builder()
        .label(i18n::text(
            "Enter one literal argument per line; blank lines are ignored. Exactly one line must be {target}. Shell syntax is never interpreted. Leave private arguments empty to disable private Launch Mode.",
        ))
        .xalign(0.0)
        .wrap(true)
        .build();
    guidance.add_css_class("dim-label");
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.append(&application_label_entry);
    root.append(&executable_entry);
    root.append(&arguments_group);
    root.append(&private_arguments_group);
    root.append(&guidance);
    Some(ManualEditor {
        root,
        application_label: application_label_entry,
        executable: executable_entry,
        arguments,
        private_arguments,
    })
}

#[allow(clippy::too_many_arguments)]
fn set_enable_name(enable: &gtk::CheckButton, name: &str) {
    enable.set_label(Some(name));
    enable.update_property(&[gtk::accessible::Property::Label(&i18n::text_with(
        "Enable Browser Candidate {name}",
        &[("{name}", name)],
    ))]);
}

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
    let enable = gtk::CheckButton::new();
    enable.set_active(enabled);
    set_enable_name(&enable, name);
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
    id_entry.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Browser Destination ID",
    ))]);
    let label_entry = gtk::Entry::builder()
        .text(label)
        .placeholder_text(i18n::text("Browser Destination display label"))
        .build();
    label_entry.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Browser Destination display label",
    ))]);
    let icon_entry = gtk::Entry::builder()
        .text(icon)
        .placeholder_text(i18n::text("Icon override"))
        .build();
    icon_entry.update_property(&[gtk::accessible::Property::Label(&i18n::text(
        "Icon override",
    ))]);

    let fields = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    fields.append(&id_entry);
    fields.append(&label_entry);
    fields.append(&icon_entry);
    let manual = manual_editor(&launch, &application_label);
    if manual.is_some() {
        label_entry.connect_changed(glib::clone!(
            #[weak]
            enable,
            move |label| set_enable_name(&enable, label.text().as_str())
        ));
    }

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
    let drag_handle = reorder_ui::prepend_handle(
        &header,
        &i18n::text_with("Reorder {name}", &[("{name}", name)]),
    );
    header.append(&icon_image);
    header.append(&enable);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.set_margin_top(9);
    body.set_margin_bottom(9);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.append(&header);
    body.append(&fields);
    if let Some(manual) = &manual {
        body.append(&manual.root);
    }
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
        manual,
        profile_label,
        enabled: enable,
        enable_allowed,
        id: id_entry,
        label: label_entry,
        icon: icon_entry,
        drag_handle,
        row,
    }
}

fn restore_fallback(
    fallback: &gtk::DropDown,
    items: &[EditorItem],
    preferred: &RefCell<Option<String>>,
    referenced_rules: &HashSet<String>,
) {
    let wanted = preferred.borrow().clone();
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

    let selected = wanted
        .as_ref()
        .and_then(|destination| ids.iter().position(|id| id == destination))
        .map(|index| index + 1)
        .unwrap_or(0);
    fallback.set_selected(selected as u32);
    *preferred.borrow_mut() = wanted;
    update_remove_blocks(items, preferred, referenced_rules);
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

fn update_remove_blocks(
    items: &[EditorItem],
    preferred: &RefCell<Option<String>>,
    referenced_rules: &HashSet<String>,
) {
    let referenced = preferred.borrow();
    for item in items {
        let id = item.id.text();
        let id = id.trim();
        let blocked =
            referenced.as_deref().map(str::trim) == Some(id) || referenced_rules.contains(id);
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
                "Cannot remove a destination referenced by a Routing Rule or the Fallback Action",
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

fn bind_destination_item(
    item: &EditorItem,
    rule_editor: &Rc<RoutingRuleEditor>,
    preferred_fallback: &Rc<RefCell<Option<String>>>,
    refresh_fallback: impl Fn() + Clone + 'static,
) {
    let previous_id = Rc::new(RefCell::new(item.id.text().to_string()));
    item.enabled.connect_toggled(glib::clone!(
        #[strong]
        refresh_fallback,
        move |_| refresh_fallback()
    ));
    item.id.connect_changed(glib::clone!(
        #[strong]
        previous_id,
        #[strong]
        rule_editor,
        #[strong]
        preferred_fallback,
        #[strong]
        refresh_fallback,
        move |entry| {
            let next = entry.text().to_string();
            let mut previous = previous_id.borrow_mut();
            if *previous != next {
                rule_editor.rewrite_destination_id(&previous, &next);
                if preferred_fallback.borrow().as_deref() == Some(previous.as_str()) {
                    *preferred_fallback.borrow_mut() = Some(next.clone());
                }
                *previous = next;
            }
            refresh_fallback();
        }
    ));
    item.label.connect_changed(glib::clone!(
        #[strong]
        refresh_fallback,
        move |_| refresh_fallback()
    ));
}

fn show_draft_status(
    error: &gtk::Label,
    items: &[EditorItem],
    rules: &[RoutingRule],
    fallback: &gtk::DropDown,
    fallback_mode: &gtk::DropDown,
) {
    match collect_configuration(items, rules, fallback, fallback_mode) {
        Ok(_) => error.set_visible(false),
        Err(failure) => {
            error.set_label(&failure.message());
            error.set_visible(true);
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
    let Some((from, to)) = list_order::neighbor_insertion(items.len(), index, direction) else {
        return;
    };
    reorder_ui::move_row_to(list, &mut items, |item| item.row.clone(), from, to);
}

fn editor_arguments(editor: &gtk::TextView) -> Vec<String> {
    let buffer = editor.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n')
            .filter(|argument| !argument.is_empty())
            .map(str::to_owned)
            .collect()
    }
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
        let (application_label, launch) = match &item.manual {
            Some(manual) => {
                let private_arguments = editor_arguments(&manual.private_arguments);
                (
                    manual.application_label.text().to_string(),
                    DestinationLaunch::Manual {
                        executable: manual.executable.text().to_string(),
                        arguments: editor_arguments(&manual.arguments),
                        private_arguments: if private_arguments.is_empty() {
                            None
                        } else {
                            Some(private_arguments)
                        },
                    },
                )
            }
            None => (item.application_label.clone(), item.launch.clone()),
        };
        let destination = BrowserDestination {
            id: item.id.text().trim().to_string(),
            label: item.label.text().to_string(),
            application_label,
            profile_label: item.profile_label.clone(),
            icon_name: if icon.is_empty() {
                None
            } else {
                Some(icon.to_string())
            },
            launch,
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
    let executable = discovery::application(desktop_id).map(|application| application.executable);
    let decision = profiles::decide(
        desktop_id,
        executable.as_deref(),
        identity,
        &profiles::DiscoveryPaths::from_env(),
    );
    let enable_allowed = decision.capability == ProfileCapability::Verified;
    let Some(assumptions) = decision.assumptions else {
        let message = match decision.capability {
            ProfileCapability::MissingApplication => i18n::text(
                "Browser Application is not installed. This profile cannot be enabled as a verified destination.",
            ),
            _ => i18n::text(
                "Unknown packaging or unsupported capability. This remains a generic Browser Application rather than a verified profile destination.",
            ),
        };
        return (Some(message), false);
    };
    let locator = match &decision.identity {
        ProfileIdentity::Firefox { name, path } => format!("{} ({name})", path.display()),
        ProfileIdentity::Chromium {
            user_data_dir,
            profile_directory,
        } => format!("{} / {profile_directory}", user_data_dir.display()),
    };
    let executable = decision
        .executable
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let text = i18n::text_with(
        "{product} family\nExecutable: {executable}\nProfile: {profile}\nPrivate flag: {flag}\nLaunch reuses the existing browser process for this profile. Browser Picker will not create, rename, delete, clone, or repair it.",
        &[
            ("{product}", &assumptions.product),
            ("{executable}", &executable),
            ("{profile}", &locator),
            ("{flag}", assumptions.private_flag),
        ],
    );
    (Some(text), enable_allowed)
}
