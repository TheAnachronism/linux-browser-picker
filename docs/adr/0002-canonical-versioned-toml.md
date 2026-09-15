# Use canonical, versioned TOML configuration

Browser Picker will treat `$XDG_CONFIG_HOME/browser-picker/config.toml` as a public, human-editable compatibility surface. TOML was chosen over YAML and JSON because it combines comments, strict configuration-oriented syntax, and ordered arrays of rule tables without YAML's schema ambiguity or JSON's poor editing ergonomics; the GUI must preserve unaffected syntax where possible, reject unknown or invalid data without rewriting it, detect external changes, save atomically, and use explicit backed-up migrations from every released schema version.
