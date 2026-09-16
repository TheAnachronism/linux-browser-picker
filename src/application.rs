use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::glib;

use crate::configuration::BrowserDestination;
use crate::i18n;
use crate::launcher;
use crate::open_target::WebTarget;

pub const ID: &str = "io.github.TheAnachronism.BrowserPicker";

pub fn run() -> glib::ExitCode {
    let application = adw::Application::builder().application_id(ID).build();
    application.connect_activate(show_configuration);
    application.run()
}

pub fn run_picker(target: WebTarget, destinations: Vec<BrowserDestination>) -> glib::ExitCode {
    let application = adw::Application::builder().application_id(ID).build();
    let target = Rc::new(target);
    let destinations = Rc::new(destinations);
    application.connect_activate(move |application| {
        if let Some(window) = application.active_window() {
            window.present();
            return;
        }
        show_picker(application, Rc::clone(&target), Rc::clone(&destinations));
    });
    application.run_with_args(&["browser-picker"])
}

fn show_configuration(application: &adw::Application) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }
    show_configuration_window(application);
}

fn show_configuration_window(application: &adw::Application) {
    let page = adw::StatusPage::builder()
        .title(i18n::text("Browser Picker"))
        .description(i18n::text(
            "Configure browser destinations and routing rules.",
        ))
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(i18n::text("Browser Picker"))
        .default_width(720)
        .default_height(480)
        .content(&toolbar)
        .build();
    window.present();
}

fn show_picker(
    application: &adw::Application,
    target: Rc<WebTarget>,
    destinations: Rc<Vec<BrowserDestination>>,
) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let host = gtk::Label::builder()
        .label(target.unicode_host())
        .xalign(0.0)
        .selectable(true)
        .build();
    host.add_css_class("title-1");
    host.update_property(&[gtk::accessible::Property::Label("Open Target host")]);
    content.append(&host);

    let host_forms = if target.unicode_host() == target.ascii_host() {
        i18n::text_with("Host: {host}", &[("{host}", target.ascii_host())])
    } else {
        i18n::text_with(
            "Unicode host: {unicode}\nASCII host: {ascii}",
            &[
                ("{unicode}", target.unicode_host()),
                ("{ascii}", target.ascii_host()),
            ],
        )
    };
    let host_details = gtk::Label::builder()
        .label(&host_forms)
        .xalign(0.0)
        .selectable(true)
        .build();
    content.append(&host_details);

    let reveal = gtk::CheckButton::with_label(&i18n::text("Reveal full URL details"));
    reveal.update_property(&[gtk::accessible::Property::Description(
        "Reveals credentials, path, query, and fragment",
    )]);
    content.append(&reveal);
    let full_target = gtk::Label::builder()
        .label(target.as_str())
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .visible(false)
        .build();
    full_target.update_property(&[gtk::accessible::Property::Label("Full Open Target details")]);
    content.append(&full_target);
    reveal.connect_toggled(glib::clone!(
        #[weak]
        full_target,
        move |button| full_target.set_visible(button.is_active())
    ));

    let search = gtk::SearchEntry::builder()
        .placeholder_text(i18n::text("Filter destinations"))
        .build();
    search.update_property(&[gtk::accessible::Property::Label(
        "Filter Browser Destinations",
    )]);
    content.append(&search);

    let error = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    error.set_focusable(true);
    error.add_css_class("error");
    error.update_property(&[gtk::accessible::Property::Label("Launch error")]);
    content.append(&error);

    let list = gtk::ListBox::new();
    list.set_activate_on_single_click(false);
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.update_property(&[gtk::accessible::Property::Label("Browser Destinations")]);
    let mut rows = Vec::with_capacity(destinations.len());
    for (index, destination) in destinations.iter().enumerate() {
        let labels = destination_search_text(destination);
        let row_content = gtk::Box::new(gtk::Orientation::Vertical, 3);
        row_content.set_margin_top(9);
        row_content.set_margin_bottom(9);
        row_content.set_margin_start(12);
        row_content.set_margin_end(12);
        let title = gtk::Label::builder()
            .label(&destination.label)
            .xalign(0.0)
            .build();
        title.add_css_class("heading");
        row_content.append(&title);
        let description = gtk::Label::builder().label(&labels).xalign(0.0).build();
        description.add_css_class("dim-label");
        row_content.append(&description);
        let row = gtk::ListBoxRow::builder()
            .child(&row_content)
            .activatable(true)
            .selectable(true)
            .build();
        let shortcut = if index < 9 {
            format!("Alt+{}", index + 1)
        } else {
            String::new()
        };
        row.update_property(&[
            gtk::accessible::Property::Label(&destination.label),
            gtk::accessible::Property::Description(&labels),
            gtk::accessible::Property::KeyShortcuts(&shortcut),
        ]);
        list.append(&row);
        rows.push(row);
    }
    let rows = Rc::new(rows);
    if let Some(first) = rows.first() {
        list.select_row(Some(first));
    }

    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    content.append(&scroller);

    let private_mode =
        gtk::CheckButton::with_label(&i18n::text("Private Launch Mode (Ctrl+Shift+P)"));
    private_mode.update_property(&[
        gtk::accessible::Property::Label("Private Launch Mode"),
        gtk::accessible::Property::KeyShortcuts("Ctrl+Shift+P"),
    ]);
    content.append(&private_mode);

    let no_results = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    let no_results_label = gtk::Label::new(Some(&i18n::text(
        "No Browser Destinations match this filter.",
    )));
    let clear_filter = gtk::Button::with_label(&i18n::text("Clear filter"));
    clear_filter.update_property(&[gtk::accessible::Property::Label("Clear destination filter")]);
    let configure = gtk::Button::with_label(&i18n::text("Configure Browser Picker"));
    no_results.append(&no_results_label);
    no_results.append(&clear_filter);
    no_results.append(&configure);
    no_results.set_visible(destinations.is_empty());
    content.append(&no_results);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&content));
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(i18n::text("Browser Picker"))
        .default_width(720)
        .default_height(640)
        .content(&toolbar)
        .build();
    search.set_key_capture_widget(Some(&window));

    update_private_mode(&list, &private_mode, &destinations);
    list.connect_selected_rows_changed(glib::clone!(
        #[weak]
        private_mode,
        #[strong]
        destinations,
        move |list| update_private_mode(list, &private_mode, &destinations)
    ));
    list.connect_row_activated(glib::clone!(
        #[weak]
        window,
        #[weak]
        error,
        #[weak]
        private_mode,
        #[strong]
        target,
        #[strong]
        destinations,
        move |_, row| {
            let index = row.index() as usize;
            launch_destination(
                &window,
                &error,
                &destinations[index],
                &target,
                private_mode.is_active(),
            );
        }
    ));

    search.connect_search_changed(glib::clone!(
        #[weak]
        list,
        #[weak]
        no_results,
        #[strong]
        rows,
        #[strong]
        destinations,
        move |search| {
            let query = search.text().to_lowercase();
            let mut first_match = None;
            for (index, row) in rows.iter().enumerate() {
                let visible = destination_search_text(&destinations[index])
                    .to_lowercase()
                    .contains(&query);
                row.set_visible(visible);
                if visible && first_match.is_none() {
                    first_match = Some(row);
                }
            }
            no_results.set_visible(first_match.is_none());
            list.select_row(first_match);
        }
    ));
    clear_filter.connect_clicked(glib::clone!(
        #[weak]
        search,
        move |_| search.set_text("")
    ));
    configure.connect_clicked(glib::clone!(
        #[weak]
        application,
        move |_| show_configuration_window(&application)
    ));

    search.connect_activate(glib::clone!(
        #[weak]
        window,
        #[weak]
        list,
        #[weak]
        error,
        #[weak]
        private_mode,
        #[strong]
        target,
        #[strong]
        destinations,
        move |_| {
            if let Some(row) = list.selected_row() {
                let index = row.index() as usize;
                launch_destination(
                    &window,
                    &error,
                    &destinations[index],
                    &target,
                    private_mode.is_active(),
                );
            }
        }
    ));

    let toggle_private = gtk::gio::SimpleAction::new("toggle-private", None);
    toggle_private.connect_activate(glib::clone!(
        #[weak]
        list,
        #[weak]
        private_mode,
        #[strong]
        destinations,
        move |_, _| {
            let supported = list
                .selected_row()
                .and_then(|row| destinations.get(row.index() as usize))
                .is_some_and(|destination| destination.private_arguments.is_some());
            if supported {
                private_mode.set_sensitive(true);
                private_mode.set_active(!private_mode.is_active());
            }
        }
    ));
    window.add_action(&toggle_private);
    application.set_accels_for_action("win.toggle-private", &["<Ctrl><Shift>p"]);

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let window_weak = window.downgrade();
    let list_weak = list.downgrade();
    let error_weak = error.downgrade();
    let private_mode_weak = private_mode.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let (Some(window), Some(list), Some(error), Some(private_mode)) = (
            window_weak.upgrade(),
            list_weak.upgrade(),
            error_weak.upgrade(),
            private_mode_weak.upgrade(),
        ) else {
            return glib::Propagation::Proceed;
        };
        if modifiers.contains(gdk::ModifierType::ALT_MASK)
            && let Some(digit) = key.to_unicode().and_then(|value| value.to_digit(10))
            && (1..=9).contains(&digit)
        {
            let index = (digit - 1) as usize;
            if let Some(row) = rows.get(index).filter(|row| row.is_visible()) {
                list.select_row(Some(row));
                launch_destination(
                    &window,
                    &error,
                    &destinations[index],
                    &target,
                    private_mode.is_active(),
                );
            }
            return glib::Propagation::Stop;
        }

        match key {
            gdk::Key::Escape => {
                window.close();
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if let Some(row) = list.selected_row() {
                    let index = row.index() as usize;
                    launch_destination(
                        &window,
                        &error,
                        &destinations[index],
                        &target,
                        private_mode.is_active(),
                    );
                }
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                select_relative(&list, &rows, -1);
                glib::Propagation::Stop
            }
            gdk::Key::Down => {
                select_relative(&list, &rows, 1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    });
    window.add_controller(keys);
    window.present();
    search.grab_focus();
}

fn destination_search_text(destination: &BrowserDestination) -> String {
    match &destination.profile_label {
        Some(profile) => format!(
            "{} — {} — {}",
            destination.label, destination.application_label, profile
        ),
        None => format!("{} — {}", destination.label, destination.application_label),
    }
}

fn update_private_mode(
    list: &gtk::ListBox,
    private_mode: &gtk::CheckButton,
    destinations: &[BrowserDestination],
) {
    let supported = list
        .selected_row()
        .and_then(|row| destinations.get(row.index() as usize))
        .is_some_and(|destination| destination.private_arguments.is_some());
    private_mode.set_sensitive(supported);
    let description = if supported {
        i18n::text("Open using the destination's private mode")
    } else {
        private_mode.set_active(false);
        i18n::text("Selected destination does not support private mode")
    };
    private_mode.set_tooltip_text(Some(&description));
    private_mode.update_property(&[gtk::accessible::Property::Description(&description)]);
}

fn select_relative(list: &gtk::ListBox, rows: &[gtk::ListBoxRow], direction: isize) {
    let selected = list.selected_row().map(|row| row.index() as isize);
    let start = selected.unwrap_or(if direction > 0 {
        -1
    } else {
        rows.len() as isize
    });
    let mut index = start + direction;
    while let Some(row) = rows.get(index as usize) {
        if row.is_visible() {
            list.select_row(Some(row));
            row.grab_focus();
            break;
        }
        index += direction;
        if index < 0 {
            break;
        }
    }
}

fn launch_destination(
    window: &adw::ApplicationWindow,
    error: &gtk::Label,
    destination: &BrowserDestination,
    target: &WebTarget,
    private: bool,
) {
    match launcher::dispatch(destination, target, private) {
        Ok(()) => window.close(),
        Err(failure) => {
            let reason = match failure.reason {
                launcher::FailureReason::NotFound => i18n::text("executable was not found"),
                launcher::FailureReason::PermissionDenied => {
                    i18n::text("executable permission was denied")
                }
                launcher::FailureReason::Other => i18n::text("process could not be started"),
            };
            error.set_label(&i18n::text_with(
                "Browser Destination '{label}' could not accept dispatch: {reason}. Choose it again to retry, or choose another destination.",
                &[("{label}", &destination.label), ("{reason}", &reason)],
            ));
            error.set_visible(true);
            error.grab_focus();
        }
    }
}
