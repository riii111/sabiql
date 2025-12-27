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

    // Input mode for key routing
    pub input_mode: InputMode,

    // Overlay visibility
    pub show_table_picker: bool,
    pub show_command_palette: bool,
    pub show_help: bool,

    // Input buffers
    pub command_line_input: String,
    pub filter_input: String,

    // Selection state
    pub explorer_selected: usize,
    pub picker_selected: usize,

    // Dummy table list (will be replaced in PR3 with real metadata)
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

            show_table_picker: false,
            show_command_palette: false,
            show_help: false,

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
