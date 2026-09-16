use gettextrs::{
    LocaleCategory, bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain,
};

const DOMAIN: &str = "browser-picker";
const LOCALE_DIRECTORY: &str = match option_env!("BROWSER_PICKER_LOCALEDIR") {
    Some(directory) => directory,
    None => "/usr/share/locale",
};

pub fn initialize() {
    // SAFETY: localization is initialized before GTK or any worker thread starts.
    unsafe { setlocale(LocaleCategory::LcAll, "") };
    bindtextdomain(DOMAIN, LOCALE_DIRECTORY).expect("translation directory should be usable");
    bind_textdomain_codeset(DOMAIN, "UTF-8").expect("UTF-8 translations should be supported");
    textdomain(DOMAIN).expect("translation domain should be usable");
}

pub fn text(message: &str) -> String {
    gettext(message)
}

pub fn text_with(message: &str, replacements: &[(&str, &str)]) -> String {
    debug_assert!(
        replacements
            .iter()
            .all(|(placeholder, _)| message.matches(placeholder).count() == 1)
    );

    let translated = gettext(message);
    let mut rendered = if replacements
        .iter()
        .all(|(placeholder, _)| translated.matches(placeholder).count() == 1)
    {
        translated
    } else {
        message.to_owned()
    };
    for (placeholder, value) in replacements {
        rendered = rendered.replacen(placeholder, value, 1);
    }
    rendered
}
