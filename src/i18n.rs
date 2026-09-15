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
