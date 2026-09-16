use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use crate::configuration::{self, BrowserDestination, DestinationLaunch};
use crate::i18n;
use crate::launcher;
use crate::open_target::WebTarget;
use crate::setup;

pub const ID: &str = "io.github.TheAnachronism.BrowserPicker";

#[derive(Clone)]
pub(crate) struct PickerSession {
    pub pending: Rc<RefCell<VecDeque<WebTarget>>>,
    pub destinations: Rc<Vec<BrowserDestination>>,
}

struct PickerSurface<'a> {
    window: &'a adw::ApplicationWindow,
    list: &'a gtk::ListBox,
    error: &'a gtk::Label,
    host: &'a gtk::Label,
    host_details: &'a gtk::Label,
    full_target: &'a gtk::Label,
    reveal: &'a gtk::CheckButton,
    search: &'a gtk::SearchEntry,
    private_mode: &'a gtk::CheckButton,
}

pub fn run() -> glib::ExitCode {
    let application = adw::Application::builder().application_id(ID).build();
    application.connect_activate(|application| {
        if let Some(window) = application.active_window() {
            window.present();
            return;
        }
        match configuration::load_optional() {
            Ok(existing) => setup::present(
                application,
                Rc::new(RefCell::new(VecDeque::new())),
                existing,
            ),
            Err(_) => show_configuration_window(application),
        }
    });
    application.run()
}

pub fn run_setup(target: WebTarget) -> glib::ExitCode {
    let application = adw::Application::builder()
        .application_id(ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let url = target.as_str().to_owned();
    let pending = Rc::new(RefCell::new(VecDeque::new()));
    application.connect_command_line(move |application, command_line| {
        for argument in command_line.arguments().iter().skip(1) {
            if let Ok(open_target) = WebTarget::parse(argument) {
                pending.borrow_mut().push_back(open_target);
            }
        }
        if application.active_window().is_none() {
            setup::present(application, Rc::clone(&pending), None);
        } else if let Some(window) = application.active_window() {
            window.present();
        }
        glib::ExitCode::SUCCESS
    });
    application.run_with_args(&["browser-picker", &url])
}

pub fn run_picker(target: WebTarget, destinations: Vec<BrowserDestination>) -> glib::ExitCode {
    let application = adw::Application::builder()
        .application_id(ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let url = target.as_str().to_owned();
    let session = PickerSession {
        pending: Rc::new(RefCell::new(VecDeque::new())),
        destinations: Rc::new(destinations),
    };
    application.connect_command_line(move |application, command_line| {
        for argument in command_line.arguments().iter().skip(1) {
            if let Ok(open_target) = WebTarget::parse(argument) {
                session.pending.borrow_mut().push_back(open_target);
            }
        }
        if application.active_window().is_none() && !session.pending.borrow().is_empty() {
            show_picker(application, session.clone());
        } else if let Some(window) = application.active_window() {
            window.present();
        }
        glib::ExitCode::SUCCESS
    });
    application.run_with_args(&["browser-picker", &url])
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

pub(crate) fn show_picker(application: &adw::Application, session: PickerSession) {
    let destinations = Rc::clone(&session.destinations);
    let pending = Rc::clone(&session.pending);
    let current = pending
        .borrow()
        .front()
        .cloned()
        .expect("Picker requires a Pending Request");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let host = gtk::Label::builder()
        .label(current.unicode_host())
        .xalign(0.0)
        .selectable(true)
        .build();
    host.add_css_class("title-1");
    host.update_property(&[gtk::accessible::Property::Description("Open Target host")]);
    content.append(&host);

    let host_details = gtk::Label::builder()
        .label(host_forms(&current))
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
        .label(current.as_str())
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .visible(false)
        .build();
    full_target.update_property(&[gtk::accessible::Property::Description(
        "Full Open Target details",
    )]);
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
    error.update_property(&[gtk::accessible::Property::Description("Launch error")]);
    content.append(&error);

    let list = gtk::ListBox::new();
    list.set_activate_on_single_click(false);
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.update_property(&[gtk::accessible::Property::Label("Browser Destinations")]);
    let mut rows = Vec::with_capacity(destinations.len());
    for (index, destination) in destinations.iter().enumerate() {
        let labels = destination_search_text(destination);
        let description_text = match &destination.unavailable_reason {
            Some(reason) => format!("{labels}\n{reason}"),
            None => labels.clone(),
        };
        let row_content = gtk::Box::new(gtk::Orientation::Vertical, 3);
        let title = gtk::Label::builder()
            .label(&destination.label)
            .xalign(0.0)
            .build();
        title.add_css_class("heading");
        row_content.append(&title);
        let description = gtk::Label::builder()
            .label(&description_text)
            .xalign(0.0)
            .wrap(true)
            .build();
        description.add_css_class("dim-label");
        row_content.append(&description);
        row_content.set_hexpand(true);

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row_box.set_margin_top(9);
        row_box.set_margin_bottom(9);
        row_box.set_margin_start(12);
        row_box.set_margin_end(12);
        row_box.append(&destination_icon(destination));
        row_box.append(&row_content);
        if destination.unavailable_reason.is_some() {
            let repair = gtk::Button::with_label(&i18n::text("Repair"));
            repair.update_property(&[
                gtk::accessible::Property::Label("Repair Browser Destination"),
                gtk::accessible::Property::Description(
                    destination
                        .unavailable_reason
                        .as_deref()
                        .unwrap_or_default(),
                ),
            ]);
            repair.connect_clicked(glib::clone!(
                #[weak]
                application,
                move |_| {
                    let existing = configuration::load_optional().ok().flatten();
                    setup::present(
                        &application,
                        Rc::new(RefCell::new(VecDeque::new())),
                        existing,
                    );
                }
            ));
            row_box.append(&repair);
        }

        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .activatable(destination.is_available())
            .selectable(true)
            .build();
        if index < 9 {
            let shortcut = format!("<Alt>{}", index + 1);
            row.update_property(&[
                gtk::accessible::Property::Label(&destination.label),
                gtk::accessible::Property::Description(&description_text),
                gtk::accessible::Property::KeyShortcuts(&shortcut),
            ]);
        } else {
            row.update_property(&[
                gtk::accessible::Property::Label(&destination.label),
                gtk::accessible::Property::Description(&description_text),
            ]);
        }
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
        gtk::accessible::Property::KeyShortcuts("<Control><Shift>p"),
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
        list,
        #[weak]
        host,
        #[weak]
        host_details,
        #[weak]
        full_target,
        #[weak]
        reveal,
        #[weak]
        search,
        #[weak]
        private_mode,
        #[strong]
        pending,
        #[strong]
        destinations,
        move |_, row| {
            let index = row.index() as usize;
            launch_destination(
                &PickerSurface {
                    window: &window,
                    list: &list,
                    error: &error,
                    host: &host,
                    host_details: &host_details,
                    full_target: &full_target,
                    reveal: &reveal,
                    search: &search,
                    private_mode: &private_mode,
                },
                &destinations[index],
                &pending,
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
        move |_| {
            let existing = configuration::load_optional().ok().flatten();
            setup::present(
                &application,
                Rc::new(RefCell::new(VecDeque::new())),
                existing,
            );
        }
    ));

    search.connect_activate(glib::clone!(
        #[weak]
        window,
        #[weak]
        list,
        #[weak]
        error,
        #[weak]
        host,
        #[weak]
        host_details,
        #[weak]
        full_target,
        #[weak]
        reveal,
        #[weak]
        search,
        #[weak]
        private_mode,
        #[strong]
        pending,
        #[strong]
        destinations,
        move |_| {
            activate_selected(
                &PickerSurface {
                    window: &window,
                    list: &list,
                    error: &error,
                    host: &host,
                    host_details: &host_details,
                    full_target: &full_target,
                    reveal: &reveal,
                    search: &search,
                    private_mode: &private_mode,
                },
                &pending,
                &destinations,
            );
        }
    ));

    let toggle_private = gio::SimpleAction::new("toggle-private", None);
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
                .is_some_and(BrowserDestination::supports_private);
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
    let host_weak = host.downgrade();
    let host_details_weak = host_details.downgrade();
    let full_target_weak = full_target.downgrade();
    let reveal_weak = reveal.downgrade();
    let search_weak = search.downgrade();
    let private_mode_weak = private_mode.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let (
            Some(window),
            Some(list),
            Some(error),
            Some(host),
            Some(host_details),
            Some(full_target),
            Some(reveal),
            Some(search),
            Some(private_mode),
        ) = (
            window_weak.upgrade(),
            list_weak.upgrade(),
            error_weak.upgrade(),
            host_weak.upgrade(),
            host_details_weak.upgrade(),
            full_target_weak.upgrade(),
            reveal_weak.upgrade(),
            search_weak.upgrade(),
            private_mode_weak.upgrade(),
        )
        else {
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
                    &PickerSurface {
                        window: &window,
                        list: &list,
                        error: &error,
                        host: &host,
                        host_details: &host_details,
                        full_target: &full_target,
                        reveal: &reveal,
                        search: &search,
                        private_mode: &private_mode,
                    },
                    &destinations[index],
                    &pending,
                    private_mode.is_active(),
                );
            }
            return glib::Propagation::Stop;
        }

        match key {
            gdk::Key::Escape => {
                pending.borrow_mut().clear();
                window.close();
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                activate_selected(
                    &PickerSurface {
                        window: &window,
                        list: &list,
                        error: &error,
                        host: &host,
                        host_details: &host_details,
                        full_target: &full_target,
                        reveal: &reveal,
                        search: &search,
                        private_mode: &private_mode,
                    },
                    &pending,
                    &destinations,
                );
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

fn host_forms(target: &WebTarget) -> String {
    if target.unicode_host() == target.ascii_host() {
        i18n::text_with("Host: {host}", &[("{host}", target.ascii_host())])
    } else {
        i18n::text_with(
            "Unicode host: {unicode}\nASCII host: {ascii}",
            &[
                ("{unicode}", target.unicode_host()),
                ("{ascii}", target.ascii_host()),
            ],
        )
    }
}

fn present_current_target(
    host: &gtk::Label,
    host_details: &gtk::Label,
    full_target: &gtk::Label,
    reveal: &gtk::CheckButton,
    error: &gtk::Label,
    search: &gtk::SearchEntry,
    target: &WebTarget,
) {
    host.set_label(target.unicode_host());
    host_details.set_label(&host_forms(target));
    full_target.set_label(target.as_str());
    reveal.set_active(false);
    error.set_visible(false);
    error.set_label("");
    search.set_text("");
}

fn update_private_mode(
    list: &gtk::ListBox,
    private_mode: &gtk::CheckButton,
    destinations: &[BrowserDestination],
) {
    let supported = list
        .selected_row()
        .and_then(|row| destinations.get(row.index() as usize))
        .is_some_and(BrowserDestination::supports_private);
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

fn activate_selected(
    surface: &PickerSurface<'_>,
    pending: &Rc<RefCell<VecDeque<WebTarget>>>,
    destinations: &[BrowserDestination],
) {
    let Some(row) = surface.list.selected_row() else {
        return;
    };
    let Some(destination) = destinations.get(row.index() as usize) else {
        return;
    };
    launch_destination(
        surface,
        destination,
        pending,
        surface.private_mode.is_active(),
    );
}

fn launch_destination(
    surface: &PickerSurface<'_>,
    destination: &BrowserDestination,
    pending: &Rc<RefCell<VecDeque<WebTarget>>>,
    private: bool,
) {
    let Some(target) = pending.borrow().front().cloned() else {
        return;
    };
    if let Some(reason) = &destination.unavailable_reason {
        surface.error.set_label(&i18n::text_with(
            "Browser Destination '{label}' is unavailable: {reason}. Use Repair to configure another destination.",
            &[("{label}", &destination.label), ("{reason}", reason)],
        ));
        surface.error.set_visible(true);
        surface.error.grab_focus();
        return;
    }
    match launcher::dispatch(destination, &target, private) {
        Ok(()) => {
            pending.borrow_mut().pop_front();
            if let Some(next) = pending.borrow().front().cloned() {
                present_current_target(
                    surface.host,
                    surface.host_details,
                    surface.full_target,
                    surface.reveal,
                    surface.error,
                    surface.search,
                    &next,
                );
            } else {
                surface.window.close();
            }
        }
        Err(failure) => {
            let reason = match failure.reason {
                launcher::FailureReason::NotFound => i18n::text("executable was not found"),
                launcher::FailureReason::PermissionDenied => {
                    i18n::text("executable permission was denied")
                }
                launcher::FailureReason::Other => i18n::text("process could not be started"),
            };
            surface.error.set_label(&i18n::text_with(
                "Browser Destination '{label}' could not accept dispatch: {reason}. Choose it again to retry, or choose another destination.",
                &[("{label}", &destination.label), ("{reason}", &reason)],
            ));
            surface.error.set_visible(true);
            surface.error.grab_focus();
        }
    }
}

fn destination_icon(destination: &BrowserDestination) -> gtk::Image {
    let image = if let Some(name) = destination
        .icon_name
        .as_deref()
        .filter(|name| !name.is_empty())
    {
        gtk::Image::from_icon_name(name)
    } else if let DestinationLaunch::Discovered { desktop_id } = &destination.launch {
        crate::discovery::icon(desktop_id)
            .map(|icon| gtk::Image::from_gicon(&icon))
            .unwrap_or_else(|| gtk::Image::from_icon_name("web-browser"))
    } else {
        gtk::Image::from_icon_name("web-browser")
    };
    image.set_pixel_size(32);
    image
}
