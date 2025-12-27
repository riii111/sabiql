#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub is_primary_key: bool,
    pub is_unique: bool,
    pub comment: Option<String>,
    pub ordinal_position: i32,
}

impl Column {
    pub fn type_display(&self) -> String {
        let mut display = self.data_type.clone();
        if !self.nullable {
            display.push_str(" NOT NULL");
        }
        if let Some(default) = &self.default {
            display.push_str(&format!(" DEFAULT {}", default));
        }
        display
    }
}
