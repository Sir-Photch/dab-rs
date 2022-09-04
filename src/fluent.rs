use std::{
    error::Error,
    path::PathBuf,
    fs,
    collections::HashMap, borrow::Cow
};
use intl_memoizer::concurrent::IntlLangMemoizer;
use log::{warn, error};
use fluent_bundle::{FluentArgs, FluentResource, bundle::FluentBundle};
use unic_langid::LanguageIdentifier;

pub struct FluentLocalizer {
    fallback_locale : LanguageIdentifier,
    resources : HashMap<LanguageIdentifier, FluentBundle<FluentResource, IntlLangMemoizer>>
}
impl FluentLocalizer {
    pub fn new(
        fallback_locale : LanguageIdentifier, 
        resource_dir : PathBuf
    ) -> Result<Self, Box<dyn Error>> {

        let res_dir = fs::read_dir(resource_dir)?;
        let mut map = HashMap::new();
        
        for entry in res_dir.flatten() {
            if !entry.path().is_dir() {
                continue
            }

            let locale = entry.file_name().to_string_lossy().parse::<LanguageIdentifier>()?;
            let mut bundle = FluentBundle::new_concurrent(vec![locale.clone()]);

            for resource in entry.path().read_dir()?.flatten() {

                if !resource.path().is_file() || !resource.path().ends_with(".ftl") {
                    continue
                }

                let resource_string = fs::read_to_string(resource.path())?;       
                let resource_parsed = FluentResource::try_new(resource_string);
                        
                if let Ok(resource_parsed) = resource_parsed {
                    if let Err(why) = bundle.add_resource(resource_parsed) {
                        error!("Could not add resource: {why:?}");
                    }
                } else if let Err((_, why)) = resource_parsed {
                    error!("Could not parse resource {}: {:?}", resource.path().display(), why)
                }                       

            }

            map.insert(locale, bundle);
        }

        if !map.contains_key(&fallback_locale) {
            panic!("Fallback locale is not provided!");
        }

        Ok(FluentLocalizer {
            fallback_locale,
            resources : map
        })
    }   

    pub fn localize<'r>(&'r self, lang_id : &str, msg : &str, args : Option<&'r FluentArgs>) -> Cow<str> {
        let mut lang_id = match lang_id.parse::<LanguageIdentifier>() {
            Ok(id) => id,
            Err(why) => {
                error!("Invalid lang_id: '{lang_id}': {why} - Using fallback...");
                self.fallback_locale.clone()
            }
        };

        if !self.resources.contains_key(&lang_id) {
            error!("No localization for {lang_id} available! - Using fallback...");
            lang_id = self.fallback_locale.clone();
        }
        let bundle = self.resources.get(&lang_id).unwrap();

        let msg = match bundle.get_message(msg) {
            Some(fluent_message) => {
                match fluent_message.value() {
                    Some(pattern) => pattern,
                    None => {
                        error!("Translation for {msg} in lang {lang_id} has no pattern!");
                        return Cow::Borrowed("Not translated :b");
                    }
                }
            },
            None => {
                error!("Translation for {msg} in lang {lang_id} not available!");
                return Cow::Borrowed("Not translated :b");
            }
        };

        let mut errors = vec![];
        let retval = bundle.format_pattern(msg, args, &mut errors);

        if !errors.is_empty() {
            warn!("Errors while formatting {msg:?}: {errors:?}");
        }

        retval
    }
}