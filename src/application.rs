use std::cell::RefCell;
use std::collections::VecDeque;
use std::env;
use std::ffi::OsStr;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use crate::configuration::{self, BrowserDestination, LaunchMode};
use crate::i18n;
use crate::launcher;
use crate::open_target::OpenTarget;
use crate::routing::{self, Preselection};
use crate::setup;

pub const ID: &str = "io.github.TheAnachronism.BrowserPicker";
const MAX_TARGETS_PER_ACTIVATION: usize = 100;
const MAX_PENDING_REQUESTS: usize = 100;
const STATUS_OVERFLOW: u8 = 6;
#[derive(Clone, Debug)]
pub(crate) struct PendingRequest {
    pub target: OpenTarget,
    pub preselection: Option<Preselection>,
}

#[derive(Clone)]
pub(crate) struct PickerSession {
    pub pending: Rc<RefCell<VecDeque<PendingRequest>>>,
    pub destinations: Rc<RefCell<Vec<BrowserDestination>>>,
    remaining: Rc<RefCell<glib::WeakRef<gtk::Label>>>,
    picker_window: Rc<RefCell<glib::WeakRef<adw::ApplicationWindow>>>,
    rebuilding: Rc<RefCell<bool>>,
    configuration_open: Rc<RefCell<bool>>,
}

impl PickerSession {
    pub(crate) fn new() -> Self {
        Self {
            pending: Rc::new(RefCell::new(VecDeque::new())),
            destinations: Rc::new(RefCell::new(Vec::new())),
            remaining: Rc::new(RefCell::new(glib::WeakRef::new())),
            picker_window: Rc::new(RefCell::new(glib::WeakRef::new())),
            rebuilding: Rc::new(RefCell::new(false)),
            configuration_open: Rc::new(RefCell::new(false)),
        }
    }

    fn update_remaining(&self) {
        if let Some(label) = self.remaining.borrow().upgrade() {
            label.set_label(&pending_count_text(self.pending.borrow().len()));
        }
    }

    fn rebuild_picker(&self, application: &adw::Application) {
        *self.rebuilding.borrow_mut() = true;
        if let Some(window) = self.picker_window.borrow().upgrade() {
            window.close();
        }
        self.picker_window.borrow().set(None);
        *self.rebuilding.borrow_mut() = false;
        if !self.pending.borrow().is_empty() && !self.configuration_is_open() {
            show_picker(application, self.clone());
        }
    }

    pub(crate) fn configuration_is_open(&self) -> bool {
        *self.configuration_open.borrow()
    }

    pub(crate) fn set_configuration_open(&self, open: bool) {
        *self.configuration_open.borrow_mut() = open;
        if let Some(window) = self.picker_window.borrow().upgrade() {
            window.set_sensitive(!open);
        }
    }

    pub(crate) fn resume_picker(&self, application: &adw::Application) {
        self.set_configuration_open(false);
        if self.pending.borrow().is_empty() {
            return;
        }
        if let Some(window) = self.picker_window.borrow().upgrade() {
            window.present();
        } else {
            show_picker(application, self.clone());
        }
    }
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
    remaining: &'a gtk::Label,
}

pub fn run() -> glib::ExitCode {
    let mut flags =
        gio::ApplicationFlags::HANDLES_COMMAND_LINE | gio::ApplicationFlags::HANDLES_OPEN;
    if env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let application = adw::Application::builder()
        .application_id(ID)
        .flags(flags)
        .build();
    let session = PickerSession::new();

    application.connect_activate(glib::clone!(
        #[strong]
        session,
        move |application| present_activation(application, &session)
    ));
    application.connect_command_line(glib::clone!(
        #[strong]
        session,
        move |application, command_line| {
            let arguments = command_line.arguments();
            let arguments = &arguments[1..];
            if arguments.is_empty() {
                present_activation(application, &session);
                return glib::ExitCode::SUCCESS;
            }

            let mut status = glib::ExitCode::SUCCESS;
            let mut needs_setup = false;
            for (index, argument) in arguments.iter().enumerate() {
                if index == MAX_TARGETS_PER_ACTIVATION {
                    command_line.printerr_literal(&format!(
                        "{}\n",
                        i18n::text("One activation accepts at most 100 Open Targets")
                    ));
                    status = glib::ExitCode::from(STATUS_OVERFLOW);
                    break;
                }
                match accept_target(&session, argument) {
                    Ok(target_needs_setup) => needs_setup |= target_needs_setup,
                    Err((message, error_status)) => {
                        command_line.printerr_literal(&format!("{message}\n"));
                        if status == glib::ExitCode::SUCCESS {
                            status = glib::ExitCode::from(error_status);
                        }
                    }
                }
            }

            present_accepted(application, &session, needs_setup);
            status
        }
    ));
    application.connect_open(glib::clone!(
        #[strong]
        session,
        move |application, files, _| {
            let mut needs_setup = false;
            let mut errors = Vec::new();
            for (index, file) in files.iter().enumerate() {
                if index == MAX_TARGETS_PER_ACTIVATION {
                    errors.push(i18n::text(
                        "One activation accepts at most 100 Open Targets",
                    ));
                    break;
                }
                let argument = file.uri();
                match accept_target(&session, OsStr::new(argument.as_str())) {
                    Ok(target_needs_setup) => needs_setup |= target_needs_setup,
                    Err((message, _)) => errors.push(message),
                }
            }
            present_accepted(application, &session, needs_setup);
            if !errors.is_empty() {
                show_request_error(application, &errors.join("\n"));
            }
        }
    ));
    application.run()
}

fn accept_target(session: &PickerSession, argument: &OsStr) -> Result<bool, (String, u8)> {
    match routing::route(argument) {
        Ok(routing::Outcome::Dispatched) => Ok(false),
        Ok(routing::Outcome::Pick {
            target,
            destinations,
            preselection,
        }) => {
            if session.pending.borrow().len() == MAX_PENDING_REQUESTS {
                return Err((
                    i18n::text("The Pending Request queue is full"),
                    STATUS_OVERFLOW,
                ));
            }
            *session.destinations.borrow_mut() = destinations;
            session.pending.borrow_mut().push_back(PendingRequest {
                target,
                preselection,
            });
            Ok(false)
        }
        Ok(routing::Outcome::Setup { target }) => {
            if session.pending.borrow().len() == MAX_PENDING_REQUESTS {
                return Err((
                    i18n::text("The Pending Request queue is full"),
                    STATUS_OVERFLOW,
                ));
            }
            session.pending.borrow_mut().push_back(PendingRequest {
                target,
                preselection: None,
            });
            Ok(true)
        }
        Err(error) => Err(crate::routing_error_message(error)),
    }
}

fn present_accepted(application: &adw::Application, session: &PickerSession, needs_setup: bool) {
    if needs_setup {
        if let Some(window) = application.active_window() {
            window.present();
        } else {
            present_setup(application, session);
        }
    } else if !session.pending.borrow().is_empty() {
        session.update_remaining();
        if session.configuration_is_open() {
            return;
        }
        if let Some(window) = session.picker_window.borrow().upgrade() {
            window.present();
        } else {
            show_picker(application, session.clone());
        }
    }
}

pub(crate) fn reapply_front(
    application: &adw::Application,
    session: &PickerSession,
) -> Result<(), (String, u8)> {
    let Some(request) = session.pending.borrow_mut().pop_front() else {
        return Ok(());
    };
    match routing::route(OsStr::new(request.target.as_str())) {
        Ok(routing::Outcome::Dispatched) => {
            session.rebuild_picker(application);
            Ok(())
        }
        Ok(routing::Outcome::Pick {
            target,
            destinations,
            preselection,
        }) => {
            *session.destinations.borrow_mut() = destinations;
            session.pending.borrow_mut().push_front(PendingRequest {
                target,
                preselection,
            });
            session.rebuild_picker(application);
            Ok(())
        }
        Ok(routing::Outcome::Setup { target }) => {
            session.pending.borrow_mut().push_front(PendingRequest {
                target,
                preselection: None,
            });
            Err((
                i18n::text("Saved configuration could not be reloaded"),
                crate::STATUS_CONFIGURATION,
            ))
        }
        Err(failure) => {
            session.pending.borrow_mut().push_front(request);
            Err(crate::routing_error_message(failure))
        }
    }
}

fn open_configuration_store() -> Result<configuration::ConfigurationStore, configuration::Error> {
    let path = configuration::default_path()?;
    match configuration::ConfigurationStore::open_path(&path) {
        Err(configuration::Error::Read(std::io::ErrorKind::NotFound)) => {
            configuration::ConfigurationStore::create_path(&path)
        }
        result => result,
    }
}

fn present_setup(application: &adw::Application, session: &PickerSession) {
    match open_configuration_store() {
        Ok(store) => setup::present(application, session.clone(), store),
        Err(_) => show_configuration_window(application),
    }
}

fn show_request_error(application: &adw::Application, message: &str) {
    let page = adw::StatusPage::builder()
        .title(i18n::text("Open Target rejected"))
        .description(message)
        .build();
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(i18n::text("Browser Picker"))
        .default_width(480)
        .default_height(240)
        .content(&page)
        .build();
    window.present();
}

fn present_activation(application: &adw::Application, session: &PickerSession) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }
    if !session.pending.borrow().is_empty() {
        show_picker(application, session.clone());
        return;
    }
    present_setup(application, session);
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
    let destinations = Rc::new(session.destinations.borrow().clone());
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
        .label(current.target.title())
        .xalign(0.0)
        .selectable(true)
        .build();
    host.add_css_class("title-1");
    host.update_property(&[gtk::accessible::Property::Description("Open Target title")]);
    content.append(&host);

    let host_details = gtk::Label::builder()
        .label(current.target.summary())
        .xalign(0.0)
        .selectable(true)
        .build();
    content.append(&host_details);

    let remaining = gtk::Label::builder()
        .label(pending_count_text(pending.borrow().len()))
        .xalign(0.0)
        .build();
    remaining.add_css_class("dim-label");
    remaining.update_property(&[gtk::accessible::Property::Label("Pending Request count")]);
    session.remaining.borrow().set(Some(&remaining));
    content.append(&remaining);

    let reveal = gtk::CheckButton::with_label(&current.target.reveal_label());
    reveal.update_property(&[gtk::accessible::Property::Description(
        current.target.reveal_description().as_str(),
    )]);
    content.append(&reveal);
    let full_target = gtk::Label::builder()
        .label(current.target.reveal_text())
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
    let edit_label = i18n::text("_Edit Routing Rules for This URL");
    let edit_rules = gtk::Button::with_mnemonic(&edit_label);
    edit_rules.update_property(&[
        gtk::accessible::Property::Label(edit_label.trim_start_matches('_')),
        gtk::accessible::Property::KeyShortcuts("<Alt>e"),
    ]);
    edit_rules.connect_clicked(glib::clone!(
        #[weak]
        application,
        #[strong]
        session,
        move |_| present_setup(&application, &session)
    ));
    content.append(&edit_rules);

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
                #[strong]
                session,
                move |_| present_setup(&application, &session)
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
    let disclaimer = gtk::Label::builder()
        .label(i18n::text(
            "Private Launch Mode uses the browser-native private or incognito context. It does not promise a separate process, an isolated profile, or the absence of disk traces.",
        ))
        .xalign(0.0)
        .wrap(true)
        .build();
    let disclaimer_description = i18n::text("Private Launch Mode privacy boundary");
    disclaimer.update_property(&[gtk::accessible::Property::Description(
        disclaimer_description.as_str(),
    )]);
    content.append(&disclaimer);

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
    session.picker_window.borrow().set(Some(&window));
    search.set_key_capture_widget(Some(&window));

    apply_preselection(&list, &private_mode, &destinations, &current);
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
        #[weak]
        remaining,
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
                    remaining: &remaining,
                },
                &destinations[index],
                &pending,
                &destinations,
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
        #[strong]
        session,
        move |_| present_setup(&application, &session)
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
        #[weak]
        remaining,
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
                    remaining: &remaining,
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
    let close = gio::SimpleAction::new("close", None);
    close.connect_activate(glib::clone!(
        #[weak]
        window,
        move |_, _| window.close()
    ));
    window.add_action(&close);
    application.set_accels_for_action("win.close", &["<Ctrl>w"]);
    let edit_routing = gio::SimpleAction::new("edit-routing", None);
    edit_routing.connect_activate(glib::clone!(
        #[weak]
        application,
        #[strong]
        session,
        move |_, _| present_setup(&application, &session)
    ));
    window.add_action(&edit_routing);
    application.set_accels_for_action("win.edit-routing", &["<Alt>e"]);
    let pending_on_close = Rc::clone(&pending);

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
    let remaining_weak = remaining.downgrade();
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
            Some(remaining),
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
            remaining_weak.upgrade(),
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
                        remaining: &remaining,
                    },
                    &destinations[index],
                    &pending,
                    &destinations,
                    private_mode.is_active(),
                );
            }
            return glib::Propagation::Stop;
        }

        match key {
            gdk::Key::Escape => {
                advance_pending_request(
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
                        remaining: &remaining,
                    },
                    &pending,
                    &destinations,
                );
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
                        remaining: &remaining,
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
    let rebuilding_on_close = Rc::clone(&session.rebuilding);
    window.connect_close_request(move |_| {
        if !*rebuilding_on_close.borrow() {
            pending_on_close.borrow_mut().clear();
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    window.present();
    if current.preselection.is_none() {
        search.grab_focus();
    }
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

fn pending_count_text(count: usize) -> String {
    if count == 1 {
        i18n::text("1 Pending Request")
    } else {
        i18n::text_with(
            "{count} Pending Requests",
            &[("{count}", &count.to_string())],
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
    target: &OpenTarget,
) {
    host.set_label(target.title());
    host_details.set_label(&target.summary());
    full_target.set_label(target.reveal_text());
    reveal.set_label(Some(&target.reveal_label()));
    reveal.update_property(&[gtk::accessible::Property::Description(
        target.reveal_description().as_str(),
    )]);
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

fn apply_preselection(
    list: &gtk::ListBox,
    private_mode: &gtk::CheckButton,
    destinations: &[BrowserDestination],
    request: &PendingRequest,
) {
    let selected = request
        .preselection
        .as_ref()
        .and_then(|preselection| {
            destinations
                .iter()
                .position(|destination| destination.id == preselection.destination_id)
        })
        .and_then(|index| list.row_at_index(index as i32))
        .or_else(|| list.row_at_index(0));
    list.select_row(selected.as_ref());
    update_private_mode(list, private_mode, destinations);
    let private = request
        .preselection
        .as_ref()
        .is_some_and(|preselection| preselection.mode == LaunchMode::Private);
    private_mode.set_active(private && private_mode.is_sensitive());
    if request.preselection.is_some()
        && let Some(selected) = selected
    {
        selected.grab_focus();
    }
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
    pending: &Rc<RefCell<VecDeque<PendingRequest>>>,
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
        destinations,
        surface.private_mode.is_active(),
    );
}

fn advance_pending_request(
    surface: &PickerSurface<'_>,
    pending: &Rc<RefCell<VecDeque<PendingRequest>>>,
    destinations: &[BrowserDestination],
) {
    let next = {
        let mut pending = pending.borrow_mut();
        pending.pop_front();
        surface
            .remaining
            .set_label(&pending_count_text(pending.len()));
        pending.front().cloned()
    };
    if let Some(next) = next {
        present_current_target(
            surface.host,
            surface.host_details,
            surface.full_target,
            surface.reveal,
            surface.error,
            surface.search,
            &next.target,
        );
        apply_preselection(surface.list, surface.private_mode, destinations, &next);
    } else {
        surface.window.close();
    }
}

fn launch_destination(
    surface: &PickerSurface<'_>,
    destination: &BrowserDestination,
    pending: &Rc<RefCell<VecDeque<PendingRequest>>>,
    destinations: &[BrowserDestination],
    private: bool,
) {
    let Some(request) = pending.borrow().front().cloned() else {
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
    if let Err(failure) = request.target.revalidate() {
        surface.error.set_label(&failure.message());
        surface.error.set_visible(true);
        surface.error.grab_focus();
        return;
    }
    match launcher::dispatch(destination, &request.target, private) {
        Ok(()) => advance_pending_request(surface, pending, destinations),
        Err(failure) => {
            let reason = match failure.reason {
                launcher::FailureReason::NotFound => i18n::text("executable was not found"),
                launcher::FailureReason::PermissionDenied => {
                    i18n::text("executable permission was denied")
                }
                launcher::FailureReason::UnsupportedPrivate => {
                    i18n::text("private Launch Mode is not available")
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
    } else if let Some(desktop_id) = destination.desktop_id() {
        crate::discovery::icon(desktop_id)
            .map(|icon| gtk::Image::from_gicon(&icon))
            .unwrap_or_else(|| gtk::Image::from_icon_name("web-browser"))
    } else {
        gtk::Image::from_icon_name("web-browser")
    };
    image.set_pixel_size(32);
    image
}
