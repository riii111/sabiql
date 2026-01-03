pub trait Dialect: Send + Sync {
    fn quote_ident(&self, name: &str) -> String;
    fn quote_literal(&self, value: &str) -> String;
}
