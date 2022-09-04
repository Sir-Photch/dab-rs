use std::{
    error::Error,
    path::PathBuf,
    fs,
    collections::HashSet
};
use log::warn;
use fluent_bundle::{FluentBundle, FluentResource};
use fluent_resmgr::resource_manager::ResourceManager;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use unic_langid::LanguageIdentifier;

pub struct FluentLocalizer {
    pub default_locale : LanguageIdentifier,
    manager : ResourceManager,
    available_locales : Vec<LanguageIdentifier>,
    resources : Vec<String>
}
impl FluentLocalizer {
    pub fn init(resource_path : &PathBuf, default_locale : &str) -> Result<Self, Box<dyn Error>> {
        let mut locales = vec![];
        let mut resources = HashSet::new();

        let res_dir = fs::read_dir(resource_path)?;
        for entry in res_dir {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.to_str() {
                        let langid = name.parse::<LanguageIdentifier>().expect("Bad folder in locales");
                        locales.push(langid);
                    }

                    let resource_files = fs::read_dir(path)?;

                    for resource_file in resource_files {
                        if let Ok(resource_file) = resource_file {
                            let resource_path = resource_file.path();
                            if resource_path.is_file() && resource_path.ends_with(".ftl") {
                                resources.insert(resource_path.file_name().expect("Bad file in resources").to_str().expect("Bad file in resources").to_owned());
                            }
                        }
                    }
                }
            }
        }

        let default_locale = default_locale.parse::<LanguageIdentifier>()?;

        if !locales.contains(&default_locale) {
            panic!("Default locale is not available!");
        }

        let mut path_scheme = resource_path.canonicalize()?.to_str().expect("Could not parse resource_path").to_owned();
        path_scheme.push_str("/{locale}/{res_id}");

        Ok(FluentLocalizer { 
            default_locale,
            manager : ResourceManager::new(path_scheme),
            available_locales : locales,
            resources : resources.into_iter().collect()
        })
    }

    pub fn get_bundle(&self, lang_id : &str) -> FluentBundle<&FluentResource> {
        let lang_id = match lang_id.parse::<LanguageIdentifier>() {
            Ok(id) => id,
            Err(why) => {
                warn!("Could not parse lang_id '{lang_id}': {why} | Falling back to default: {}", self.default_locale);
                self.default_locale.clone()
            }
        };

        let resolved_locales = negotiate_languages(
            &vec![lang_id], 
            &self.available_locales, 
            Some(&self.default_locale), 
            NegotiationStrategy::Filtering
        );

        self.manager.get_bundle(
            resolved_locales.into_iter().map(|l| l.to_owned()).collect(), 
            self.resources.to_owned()
        )
    }
}