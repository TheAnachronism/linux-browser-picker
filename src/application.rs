use adw::prelude::*;

use crate::i18n;

pub const ID: &str = "io.github.TheAnachronism.BrowserPicker";

pub fn run() -> gtk::glib::ExitCode {
    let application = adw::Application::builder().application_id(ID).build();
    application.connect_activate(show_configuration);
    application.run()
}

fn show_configuration(application: &adw::Application) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }

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
