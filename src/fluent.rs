use fluent_bundle::{bundle::FluentBundle, FluentArgs, FluentResource};
use intl_memoizer::concurrent::IntlLangMemoizer;
use log::{error, warn};
use std::{borrow::Cow, collections::HashMap, error::Error, fs, path::PathBuf};
use unic_langid::LanguageIdentifier;

pub struct FluentLocalizer {
    pub fallback_locale: LanguageIdentifier,
    resources: HashMap<LanguageIdentifier, FluentBundle<FluentResource, IntlLangMemoizer>>,
}
impl FluentLocalizer {
    pub fn new(
        fallback_locale: LanguageIdentifier,
        resource_dir: PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        let res_dir = fs::read_dir(resource_dir)?;
        let mut map = HashMap::new();

        for entry in res_dir.flatten() {
            if !entry.path().is_dir() {
                continue;
            }

            let locale = entry
                .file_name()
                .to_string_lossy()
                .parse::<LanguageIdentifier>()?;
            let mut bundle = FluentBundle::new_concurrent(vec![locale.clone()]);

            for resource in (entry.path().read_dir()?).flatten() {
                if !resource.path().is_file()
                    || resource
                        .path()
                        .extension()
                        .expect("Invalid file in localizations!")
                        != "ftl"
                {
                    continue;
                }

                let resource_string = fs::read_to_string(resource.path())?;
                let resource_parsed = FluentResource::try_new(resource_string);

                if let Ok(resource_parsed) = resource_parsed {
                    if let Err(why) = bundle.add_resource(resource_parsed) {
                        error!("Could not add resource: {why:?}");
                    }
                } else if let Err((_, why)) = resource_parsed {
                    error!(
                        "Could not parse resource {}: {:?}",
                        resource.path().display(),
                        why
                    )
                }
            }

            map.insert(locale, bundle);
        }

        if !map.contains_key(&fallback_locale) {
            panic!("Fallback locale is not provided!");
        }

        Ok(FluentLocalizer {
            fallback_locale,
            resources: map,
        })
    }

    pub fn get_available_localizations(&self) -> Vec<String> {
        let mut localizations = vec![];

        for id in self.resources.keys() {
            localizations.push(id.to_string());
        }

        localizations
    }

    pub fn get_bundle(&self, lang_id: &str) -> &FluentBundle<FluentResource, IntlLangMemoizer> {
        let mut lang_id = match lang_id.parse::<LanguageIdentifier>() {
            Ok(id) => id,
            Err(why) => {
                error!("Invalid lang_id: '{lang_id}': {why} - Using fallback...");
                self.fallback_locale.clone()
            }
        };

        if !self.resources.contains_key(&lang_id) {
            error!("No localization for '{lang_id}' available! - Using fallback...");
            lang_id = self.fallback_locale.clone();
        }
        self.resources.get(&lang_id).unwrap()
    }

    pub fn localize<'r>(
        &'r self,
        lang_id: &str,
        msg: &str,
        args: Option<&'r FluentArgs>,
    ) -> Cow<str> {
        // TODO this also returns "not translated" when set of translated msgs for each initially available locale are not equal to each other
        let bundle = self.get_bundle(lang_id);

        let msg = match bundle.get_message(msg) {
            Some(fluent_message) => match fluent_message.value() {
                Some(pattern) => pattern,
                None => {
                    error!("Translation for '{msg}' in lang '{lang_id}' has no pattern!");
                    return Cow::Borrowed("Not translated :b");
                }
            },
            None => {
                error!("Translation for '{msg}' in lang '{lang_id}' not available!");
                return Cow::Borrowed("Not translated :b");
            }
        };

        let mut errors = vec![];
        let retval = bundle.format_pattern(msg, args, &mut errors);

        if !errors.is_empty() {
            warn!("Errors while formatting: {msg:?}: {errors:?}");
        }

        retval
    }
}
