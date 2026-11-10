//! The face in every language (ocoeco SPEC.md section 9): Project Fluent catalogs, compiled in
//! (i18n/<language>.ftl), and the language of the page being rendered. A message missing in a
//! language falls back to the default language, then to its id - a partly translated page is still
//! a whole page.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use std::cell::RefCell;

pub struct Language {
    pub code: &'static str,
    /// its name in itself, as the switcher shows it
    pub name: &'static str,
    source: &'static str,
}

pub const LANGUAGES: [Language; 2] = [
    Language {
        code: "en",
        name: "English",
        source: include_str!("../i18n/en.ftl"),
    },
    Language {
        code: "ru",
        name: "Русский",
        source: include_str!("../i18n/ru.ftl"),
    },
];

pub const DEFAULT: &str = "en";

type Bundle = FluentBundle<FluentResource>;

fn bundle(code: &str) -> Bundle {
    let l = LANGUAGES
        .iter()
        .find(|l| l.code == code)
        .unwrap_or(&LANGUAGES[0]);
    let mut b = FluentBundle::new(vec![l.code.parse().expect("a language tag")]);
    b.set_use_isolating(false);
    b.add_resource(FluentResource::try_new(l.source.to_string()).expect("a catalog that parses"))
        .expect("messages named once");
    b
}

thread_local! {
    static CURRENT: RefCell<(String, Bundle, Bundle)> = RefCell::new((DEFAULT.into(), bundle(DEFAULT), bundle(DEFAULT)));
}

/// Renders from now on in this language.
pub fn set(code: &str) {
    CURRENT.with(|c| *c.borrow_mut() = (code.to_string(), bundle(code), bundle(DEFAULT)));
}

pub fn current() -> String {
    CURRENT.with(|c| c.borrow().0.clone())
}

fn format(b: &Bundle, id: &str, args: Option<&FluentArgs>) -> Option<String> {
    let pattern = b.get_message(id)?.value()?;
    let mut errors = vec![];
    Some(b.format_pattern(pattern, args, &mut errors).into_owned())
}

fn lookup(id: &str, args: Option<&FluentArgs>) -> String {
    CURRENT.with(|c| {
        let c = c.borrow();
        format(&c.1, id, args)
            .or_else(|| format(&c.2, id, args))
            .unwrap_or_else(|| id.to_string())
    })
}

/// A message in the current language.
pub fn t(id: &str) -> String {
    lookup(id, None)
}

/// A message with a count - in the right plural form of the current language.
pub fn tn(id: &str, count: i64) -> String {
    let mut a = FluentArgs::new();
    a.set("count", FluentValue::from(count));
    lookup(id, Some(&a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_plurals_are_right() {
        set("ru");
        assert_eq!(tn("commits-90", 1), "1 коммит за последние 90 дней");
        assert_eq!(tn("commits-90", 2), "2 коммита за последние 90 дней");
        assert_eq!(tn("commits-90", 5), "5 коммитов за последние 90 дней");
        assert_eq!(tn("commits-90", 21), "21 коммит за последние 90 дней");
        set("en");
        assert_eq!(tn("commits-90", 1), "1 commit in the last 90 days");
        assert_eq!(tn("commits-90", 103), "103 commits in the last 90 days");
    }

    #[test]
    fn a_missing_message_falls_back() {
        set("ru");
        assert_eq!(t("nav-observatory"), "Обсерватория");
        assert_eq!(
            t("only-in-english"),
            "Only in English",
            "missing in Russian: the default language"
        );
        assert_eq!(
            t("no-such-message"),
            "no-such-message",
            "missing everywhere: its id"
        );
        set("en");
    }
}
