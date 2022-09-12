pub trait Nameable {
    fn name(&mut self, name: &str) -> &mut Self;
}
impl Nameable for serenity::builder::CreateApplicationCommand {
    fn name(&mut self, name: &str) -> &mut Self {
        self.name(name)
    }
}
impl Nameable for serenity::builder::CreateApplicationCommandOption {
    fn name(&mut self, name: &str) -> &mut Self {
        self.name(name)
    }
}
