use super::input_mode::InputMode;
use super::mode::Mode;

#[allow(dead_code)]
pub struct AppState {
    pub mode: Mode,
    pub should_quit: bool,
    pub project_name: String,
    pub profile_name: String,
    pub database_name: Option<String>,
    pub current_table: Option<String>,
    pub focus_mode: bool,
    pub active_tab: usize,
    pub input_mode: InputMode,
    pub command_line_input: String,
    pub filter_input: String,
    pub explorer_selected: usize,
    pub picker_selected: usize,
    pub tables: Vec<String>,
}

impl AppState {
    pub fn new(project_name: String, profile_name: String) -> Self {
        Self {
            mode: Mode::default(),
            should_quit: false,
            project_name,
            profile_name,
            database_name: None,
            current_table: None,
            focus_mode: false,
            active_tab: 0,
            input_mode: InputMode::default(),
            command_line_input: String::new(),
            filter_input: String::new(),
            explorer_selected: 0,
            picker_selected: 0,
            tables: vec![
                "public.users".to_string(),
                "public.orders".to_string(),
                "public.products".to_string(),
                "public.categories".to_string(),
                "public.order_items".to_string(),
            ],
        }
    }
}
