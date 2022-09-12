pub trait Localizable {
    fn localize_default(&mut self, _localization: &str) -> &mut Self;
    fn localize(&mut self, locale: &str, localization: &str) -> &mut Self;
}
impl Localizable for serenity::builder::CreateApplicationCommandOption {
    fn localize_default(&mut self, localization: &str) -> &mut Self {
        self.description(localization)
    }

    fn localize(&mut self, locale: &str, localization: &str) -> &mut Self {
        self.description_localized(locale, localization)
    }
}
impl Localizable for serenity::builder::CreateApplicationCommand {
    fn localize_default(&mut self, localization: &str) -> &mut Self {
        self.description(localization)
    }

    fn localize(&mut self, locale: &str, localization: &str) -> &mut Self {
        self.description_localized(locale, localization)
    }
}
