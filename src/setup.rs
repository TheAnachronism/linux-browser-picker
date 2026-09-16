use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use gtk::gio;
use gtk::glib;

use crate::application::{self, PickerSession};
use crate::configuration::{
    self, BrowserDestination, Configuration, DestinationLaunch, FallbackAction,
};
use crate::discovery::{self, BrowserCandidate};
use crate::i18n;
use crate::open_target::WebTarget;

#[derive(Clone)]
struct EditorItem {
    launch: DestinationLaunch,
    application_label: String,
    profile_label: Option<String>,
    enabled: gtk::CheckButton,
    id: gtk::Entry,
    label: gtk::Entry,
    icon: gtk::Entry,
    row: gtk::ListBoxRow,
}

pub fn present(
    application: &adw::Application,
    pending: Rc<RefCell<VecDeque<WebTarget>>>,
    existing: Option<Configuration>,
) {
    if let Some(window) = application
        .windows()
        .into_iter()
        .find(|window| window.widget_name() == "destination-setup")
    {
        window.present();
        return;
    }

    let discovery = discovery::discover();
    let items = Rc::new(RefCell::new(editor_items(&existing, &discovery.ordinary)));
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
            .label(target.unicode_host())
            .xalign(0.0)
            .selectable(true)
            .build();
        host.add_css_class("heading");
        host.update_property(&[gtk::accessible::Property::Description(
            "Waiting Open Target host",
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
    move_up.update_property(&[gtk::accessible::Property::Label("Move destination up")]);
    let move_down = gtk::Button::from_icon_name("go-down-symbolic");
    move_down.update_property(&[gtk::accessible::Property::Label("Move destination down")]);
    order.append(&move_up);
    order.append(&move_down);
    content.append(&order);

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

    let error = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    error.add_css_class("error");
    error.set_focusable(true);
    error.update_property(&[gtk::accessible::Property::Description(
        "Destination configuration error",
    )]);
    content.append(&error);

    let save = gtk::Button::with_mnemonic(&i18n::text("_Save destinations"));
    save.add_css_class("suggested-action");
    save.update_property(&[
        gtk::accessible::Property::Label("Save destinations"),
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

    let preferred_fallback = Rc::new(RefCell::new(existing.as_ref().and_then(|configuration| {
        match &configuration.fallback {
            FallbackAction::Open(destination) => Some(destination.clone()),
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
        move || {
            restore_fallback(&fallback, &items.borrow(), &preferred_fallback);
        }
    );
    refresh_fallback();
    fallback.connect_selected_notify(glib::clone!(
        #[strong]
        items,
        #[strong]
        preferred_fallback,
        move |fallback| {
            update_preferred_fallback(fallback, &items.borrow(), &preferred_fallback);
            update_remove_blocks(&items.borrow(), &preferred_fallback);
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

    move_up.connect_clicked(glib::clone!(
        #[weak]
        list,
        #[strong]
        items,
        #[strong]
        refresh_fallback,
        move |_| {
            reorder(&list, &items, -1);
            refresh_fallback();
        }
    ));
    move_down.connect_clicked(glib::clone!(
        #[weak]
        list,
        #[strong]
        items,
        #[strong]
        refresh_fallback,
        move |_| {
            reorder(&list, &items, 1);
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
        error,
        #[strong]
        items,
        #[strong]
        pending,
        move |_| {
            save_destinations(&application, &window, &fallback, &error, &items, &pending);
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
        error,
        #[strong]
        items,
        #[strong]
        pending,
        move |_, _| {
            save_destinations(&application, &window, &fallback, &error, &items, &pending);
        }
    ));
    window.add_action(&save_action);
    application.set_accels_for_action("win.save-destinations", &["<Alt>s"]);
    save.set_receives_default(true);
    window.set_default_widget(Some(&save));

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let application_weak = application.downgrade();
    let window_weak = window.downgrade();
    let fallback_weak = fallback.downgrade();
    let error_weak = error.downgrade();
    let key_items = Rc::clone(&items);
    let key_pending = Rc::clone(&pending);
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let (Some(application), Some(window), Some(fallback), Some(error)) = (
            application_weak.upgrade(),
            window_weak.upgrade(),
            fallback_weak.upgrade(),
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
                &error,
                &key_items,
                &key_pending,
            );
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);

    window.present();
    if first_run && let Some(first) = items.borrow().first() {
        let enable = first.enabled.clone();
        glib::idle_add_local_once(move || {
            enable.grab_focus();
        });
    }
}

fn save_destinations(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    fallback: &gtk::DropDown,
    error: &gtk::Label,
    items: &Rc<RefCell<Vec<EditorItem>>>,
    pending: &Rc<RefCell<VecDeque<WebTarget>>>,
) {
    match collect_configuration(&items.borrow(), fallback) {
        Ok(configuration) => match configuration::save(&configuration) {
            Ok(()) => {
                let saved = configuration::load().unwrap_or(configuration);
                if pending.borrow().front().is_some() {
                    application::show_picker(
                        application,
                        PickerSession {
                            pending: Rc::clone(pending),
                            destinations: Rc::new(saved.destinations),
                        },
                    );
                    window.close();
                } else {
                    error.set_visible(false);
                    window.present();
                }
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

fn editor_items(
    existing: &Option<Configuration>,
    ordinary: &[BrowserCandidate],
) -> Vec<EditorItem> {
    let mut used_ids = HashSet::new();
    let mut items = Vec::new();
    let mut configured_desktop_ids = HashSet::new();

    if let Some(configuration) = existing {
        for destination in &configuration.destinations {
            used_ids.insert(destination.id.clone());
            if let DestinationLaunch::Discovered { desktop_id } = &destination.launch {
                configured_desktop_ids.insert(desktop_id.clone());
            }
            items.push(item_from_destination(destination));
        }
    }

    for candidate in ordinary {
        if configured_desktop_ids.contains(&candidate.desktop_id) {
            continue;
        }
        let slug = discovery::unique_slug(
            &discovery::suggested_slug(&candidate.desktop_id),
            &mut used_ids,
        );
        items.push(item_from_candidate(candidate, slug));
    }
    items
}

fn item_from_destination(destination: &BrowserDestination) -> EditorItem {
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
) -> EditorItem {
    let enable = gtk::CheckButton::with_label(name);
    enable.set_active(enabled);
    enable.update_property(&[gtk::accessible::Property::Label(&i18n::text_with(
        "Enable Browser Candidate {name}",
        &[("{name}", name)],
    ))]);

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
            DestinationLaunch::Discovered { desktop_id } => discovery::icon(desktop_id)
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
        item.enabled.set_sensitive(!blocked);
        if blocked {
            item.enabled.set_tooltip_text(Some(&i18n::text(
                "Cannot remove a destination referenced by the Fallback Action",
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
    fallback: &gtk::DropDown,
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
        FallbackAction::Open(id.clone())
    };
    configuration::assemble(destinations, fallback)
}
