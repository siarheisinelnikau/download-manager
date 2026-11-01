//! Embedded UI catalogs. File names, URLs and diagnostic payloads never enter this lookup.
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "ru")]
    Russian,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "it")]
    Italian,
    #[default]
    #[serde(rename = "en", other)]
    English,
}

static CATALOG: LazyLock<HashMap<&'static str, [&'static str; 6]>> = LazyLock::new(|| {
    include_str!("locales.tsv")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let columns: Vec<_> = line.split('\t').collect();
            let row: [&str; 6] = columns.try_into().expect("six language columns");
            (row[0], row)
        })
        .collect()
});

impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];

    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }

    pub fn text(self, english: &str) -> &str {
        let index = match self {
            Self::English => 0,
            Self::Russian => 1,
            Self::French => 2,
            Self::German => 3,
            Self::Spanish => 4,
            Self::Italian => 5,
        };
        CATALOG.get(english).map_or(english, |row| row[index])
    }

    /// Localize the consumer-facing summary while preserving raw diagnostics separately.
    pub fn error_summary(self, diagnostic: &str) -> &str {
        if self == Self::English || CATALOG.contains_key(diagnostic) {
            self.text(diagnostic)
        } else if let Some((context, _)) = diagnostic.split_once(": ")
            && CATALOG.contains_key(context)
        {
            self.text(context)
        } else {
            self.text("The action couldn't finish. Open error details for more information.")
        }
    }

    pub fn format(self, english: &str, values: &[(&str, String)]) -> String {
        let mut remaining = self.text(english);
        let mut message = String::with_capacity(remaining.len());
        while let Some(start) = remaining.find('{') {
            message.push_str(&remaining[..start]);
            let Some(end) = remaining[start..].find('}') else {
                message.push_str(&remaining[start..]);
                return message;
            };
            let end = start + end;
            let name = &remaining[start + 1..end];
            if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
                message.push_str(value);
            } else {
                message.push_str(&remaining[start..=end]);
            }
            remaining = &remaining[end + 1..];
        }
        message.push_str(remaining);
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .map(|part| part.split('}').next().unwrap())
            .collect()
    }

    #[test]
    fn catalogs_are_complete_unique_and_preserve_placeholders() {
        assert_eq!(
            Language::ALL.map(|language| language.text("Settings")),
            [
                "Settings",
                "Настройки",
                "Paramètres",
                "Einstellungen",
                "Configuración",
                "Impostazioni"
            ]
        );
        let mut keys = BTreeSet::new();
        for line in include_str!("locales.tsv")
            .lines()
            .filter(|line| !line.is_empty())
        {
            let row: Vec<_> = line.split('\t').collect();
            assert_eq!(row.len(), 6, "{}", row[0]);
            assert!(keys.insert(row[0]), "duplicate key: {}", row[0]);
            for translation in &row {
                assert!(!translation.trim().is_empty(), "{}", row[0]);
                assert_eq!(
                    placeholders(row[0]),
                    placeholders(translation),
                    "{}",
                    row[0]
                );
            }
        }
        assert!(keys.len() > 100);
    }

    #[test]
    fn old_settings_keep_their_values_and_default_to_english() {
        let old = r#"{"concurrency":3,"bandwidth_kib":128,"theme":"Light","close_to_tray":false}"#;
        let settings: crate::model::Settings = serde_json::from_str(old).unwrap();
        assert_eq!(settings.language, Language::English);
        assert_eq!(settings.theme, crate::model::Theme::Light);
        assert_eq!(settings.concurrency, 3);
        assert_eq!(settings.bandwidth_kib, 128);
        assert!(!settings.close_to_tray);
        assert_eq!(
            serde_json::from_str::<crate::model::Settings>(r#"{"language":"unknown"}"#)
                .unwrap()
                .language,
            Language::English
        );
        for language in Language::ALL {
            let mut updated = settings.clone();
            updated.language = language;
            let round_trip: crate::model::Settings =
                serde_json::from_str(&serde_json::to_string(&updated).unwrap()).unwrap();
            assert_eq!(round_trip, updated);
        }
    }

    #[test]
    fn formatting_preserves_user_values_and_unknown_diagnostics() {
        let value = "a file {size}.zip".to_owned();
        assert_eq!(
            Language::French.format(
                "{size} · Saved to {folder}",
                &[("size", "1 MiB".into()), ("folder", value.clone())]
            ),
            format!("1 MiB · Enregistré dans {value}")
        );
        assert_eq!(Language::Italian.text("OS error 12345"), "OS error 12345");
    }
}
