use crate::api::SplunkClient;
use crate::config::Config;
use crate::models::splunk::JobStatus;
use crate::utils::saved_searches::SavedSearchManager;
use crossterm::{
    cursor::SetCursorStyle,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Row, Sparkline,
        Table, TableState, Wrap,
    },
    Frame, Terminal,
};
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::process::{Command, Stdio};
use std::{error::Error, io, sync::Arc};
use tokio::sync::Mutex;

fn is_inside(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}
use log::{error, info};
use syntect::highlighting::FontStyle;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};

#[derive(Clone, Copy, PartialEq)]
pub enum ThemeVariant {
    Default,
    ColorPop,
    Splunk,
    Neon,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AppTheme {
    pub variant: ThemeVariant,
    pub border: Color,
    pub text: Color,
    pub input_edit: Color,
    pub title_main: Color,
    pub title_secondary: Color,
    pub summary_highlight: Color,
    pub owner_label: Color,
    pub date_label: Color,
    pub active_label: Color,
    pub evilness_label: Color,
    pub confidence_filled: Color,
    pub confidence_empty: Color,
    pub separator: Color,
}

impl AppTheme {
    pub fn default_theme() -> Self {
        Self {
            variant: ThemeVariant::Default,
            border: Color::Green,
            text: Color::White,
            input_edit: Color::Yellow,
            title_main: Color::Green,
            title_secondary: Color::Cyan,
            summary_highlight: Color::Magenta,
            owner_label: Color::Cyan,
            date_label: Color::DarkGray,
            active_label: Color::Green,
            evilness_label: Color::Red,
            confidence_filled: Color::Green,
            confidence_empty: Color::DarkGray,
            separator: Color::DarkGray,
        }
    }

    pub fn color_pop() -> Self {
        Self {
            variant: ThemeVariant::ColorPop,
            border: Color::Cyan,
            text: Color::White,
            input_edit: Color::Red,
            title_main: Color::Yellow,
            title_secondary: Color::Green,
            summary_highlight: Color::Blue,
            owner_label: Color::Green,
            date_label: Color::Gray,
            active_label: Color::Yellow,
            evilness_label: Color::Red,
            confidence_filled: Color::Yellow,
            confidence_empty: Color::DarkGray,
            separator: Color::Gray,
        }
    }

    pub fn splunk() -> Self {
        Self {
            variant: ThemeVariant::Splunk,
            border: Color::Rgb(115, 165, 52),
            text: Color::Rgb(255, 255, 255),
            input_edit: Color::Rgb(245, 130, 32),
            title_main: Color::Rgb(115, 165, 52),
            title_secondary: Color::Rgb(0, 122, 195),
            summary_highlight: Color::Rgb(214, 61, 139),
            owner_label: Color::Rgb(45, 156, 219),
            date_label: Color::Rgb(164, 164, 164),
            active_label: Color::Rgb(115, 165, 52),
            evilness_label: Color::Rgb(208, 2, 27),
            confidence_filled: Color::Rgb(115, 165, 52),
            confidence_empty: Color::Rgb(60, 68, 77),
            separator: Color::Rgb(80, 80, 80),
        }
    }

    pub fn neon() -> Self {
        // Active BG: #FF1493 (DeepPink), Active FG: Black, Inactive BG: #00FF00 (Lime), Inactive FG: Black
        Self {
            variant: ThemeVariant::Neon,
            border: Color::Rgb(0, 255, 0), // Inactive Pill BG (Lime)
            text: Color::White,
            input_edit: Color::Rgb(255, 20, 147), // Active Pill BG (DeepPink)
            title_main: Color::Rgb(0, 255, 0),    // Lime
            title_secondary: Color::Cyan,
            summary_highlight: Color::Rgb(255, 20, 147), // DeepPink
            owner_label: Color::Rgb(0, 255, 0),
            date_label: Color::DarkGray,
            active_label: Color::Rgb(255, 20, 147), // DeepPink
            evilness_label: Color::Red,
            confidence_filled: Color::Rgb(255, 20, 147),
            confidence_empty: Color::DarkGray,
            separator: Color::DarkGray,
        }
    }
}

enum InputMode {
    Normal,
    Editing,
    SaveSearch,
    LoadSearch,
    ConfirmOverwrite,
    LocalSearch,
    ThemeSelect,
    Help,
}

#[derive(Clone, Copy, PartialEq)]
pub enum EditorMode {
    Standard,
    Vim(VimState),
}

#[derive(Clone, Copy, PartialEq)]
pub enum VimState {
    Normal,
    Insert,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ViewMode {
    RawEvents,
    Table,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ViewFocus {
    Search,
    ContentList,
    ContentDetail,
}

pub struct App {
    input: String,
    input_scroll: u16,
    input_scroll_x: u16,
    input_mode: InputMode,
    client: Arc<SplunkClient>,
    status_message: String,
    pub theme: AppTheme,

    // View Mode
    pub view_mode: ViewMode,
    pub view_focus: ViewFocus,
    pub table_state: TableState,
    pub detail_scroll: u16,

    // Search State
    current_job_sid: Option<String>,
    current_job_status: Option<JobStatus>,
    search_results: Vec<Value>,
    results_fetched: bool,
    scroll_offset: u16,

    // Local Search
    local_search_query: String,
    search_matches: Vec<usize>,
    current_match_index: Option<usize>,

    // Syntax Highlighting
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    syntax_theme: Theme,
    cached_detail: ratatui::text::Text<'static>,

    // Layout Areas (for mouse interaction)
    pub search_area: Rect,
    pub main_area: Rect,
    pub detail_area: Rect,

    // Status polling
    is_status_fetching: bool,

    pub should_open_editor: bool,

    // Saved Search State
    save_search_name: String,
    saved_searches: Vec<String>,
    saved_search_list_state: ListState,
    current_saved_search_name: Option<String>,

    // Editor Logic
    editor_mode: EditorMode,
    cursor_position: usize, // Byte index into input string
    editor_file_path: Option<String>,

    // Theme Selection
    theme_list_state: ListState,
    theme_options: Vec<&'static str>,

    // Timing
    job_created_at: Option<std::time::Instant>,
}

impl App {
    pub fn new(client: Arc<SplunkClient>) -> App {
        let theme_set = ThemeSet::load_defaults();
        let syntax_theme = theme_set.themes["base16-ocean.dark"].clone();
        let mut app = App {
            input: String::new(),
            input_scroll: 0,
            input_scroll_x: 0,
            input_mode: InputMode::Normal,
            local_search_query: String::new(),
            search_matches: Vec::new(),
            current_match_index: None,
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set,
            syntax_theme,
            cached_detail: ratatui::text::Text::default(),
            search_area: Rect::default(),
            main_area: Rect::default(),
            detail_area: Rect::default(),
            client,
            status_message: String::from(
                "Press 'q' to quit, 'e' to enter search mode, 't' to toggle theme.",
            ),
            theme: AppTheme::default_theme(),
            view_mode: ViewMode::Table,
            view_focus: ViewFocus::Search,
            table_state: TableState::default(),
            detail_scroll: 0,
            current_job_sid: None,
            current_job_status: None,
            search_results: Vec::new(),
            results_fetched: false,
            scroll_offset: 0,
            is_status_fetching: false,
            should_open_editor: false,
            save_search_name: String::new(),
            saved_searches: Vec::new(),
            saved_search_list_state: ListState::default(),
            current_saved_search_name: None,
            editor_mode: EditorMode::Standard,
            cursor_position: 0,
            editor_file_path: None,
            theme_list_state: ListState::default(),
            theme_options: vec!["Default", "ColorPop", "Splunk", "Neon"],
            job_created_at: None,
        };

        // Load saved theme
        if let Ok(config) = Config::load() {
            if let Some(theme_name) = config.theme {
                app.apply_theme(&theme_name, false);
            }
        }
        app
    }

    fn perform_local_search(&mut self) {
        if self.local_search_query.trim().is_empty() {
            return;
        }

        self.search_matches.clear();
        self.current_match_index = None;

        let pattern = match regex::RegexBuilder::new(&self.local_search_query)
            .case_insensitive(true)
            .build()
        {
            Ok(re) => re,
            Err(e) => {
                self.status_message = format!("Invalid Regex: {}", e);
                return;
            }
        };

        for (i, result) in self.search_results.iter().enumerate() {
            // Search in _raw or full JSON dump
            let text = result.get("_raw").and_then(|v| v.as_str()).unwrap_or("");
            if pattern.is_match(text) {
                self.search_matches.push(i);
            }
        }

        if self.search_matches.is_empty() {
            self.status_message = format!("No matches found for '{}'", self.local_search_query);
        } else {
            self.current_match_index = Some(0);
            self.jump_to_match(0);
            self.status_message = format!(
                "Found {} matches. (1/{})",
                self.search_matches.len(),
                self.search_matches.len()
            );
        }
    }

    fn next_match(&mut self) {
        if let Some(curr) = self.current_match_index {
            let next = if curr + 1 >= self.search_matches.len() {
                0
            } else {
                curr + 1
            };
            self.current_match_index = Some(next);
            self.jump_to_match(next);
            self.status_message = format!("Match {}/{}", next + 1, self.search_matches.len());
        }
    }

    fn prev_match(&mut self) {
        if let Some(curr) = self.current_match_index {
            let prev = if curr == 0 {
                self.search_matches.len() - 1
            } else {
                curr - 1
            };
            self.current_match_index = Some(prev);
            self.jump_to_match(prev);
            self.status_message = format!("Match {}/{}", prev + 1, self.search_matches.len());
        }
    }

    fn jump_to_match(&mut self, match_index: usize) {
        if let Some(row_index) = self.search_matches.get(match_index) {
            match self.view_mode {
                ViewMode::RawEvents => {
                    self.scroll_offset = *row_index as u16;
                }
                ViewMode::Table => {
                    self.table_state.select(Some(*row_index));
                    self.detail_scroll = 0;
                    self.update_detail_view();
                }
            }
        }
    }

    async fn perform_search(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }

        info!("Starting search for: {}", self.input);
        self.status_message = format!("Creating search job for '{}'...", self.input);
        self.current_job_sid = None;
        self.current_job_status = None;
        self.search_results.clear();
        self.results_fetched = false;
        self.scroll_offset = 0;
        self.job_created_at = None;

        match self.client.create_search(&self.input).await {
            Ok(sid) => {
                info!("Job created successfully: {}", sid);
                self.current_job_sid = Some(sid.clone());
                self.status_message = format!("Job created (SID: {}). Running...", sid);
                self.job_created_at = Some(std::time::Instant::now());
            }
            Err(e) => {
                error!("Search creation failed: {}", e);
                self.status_message = format!("Search failed: {}", e);
            }
        }
    }

    #[allow(dead_code)]
    pub async fn update_job_status(&mut self) {
        // Deprecated: Logic moved to background task in run_loop to avoid blocking UI
    }

    #[allow(dead_code)]
    async fn kill_search(&mut self) {
        if let Some(sid) = &self.current_job_sid {
            if let Err(e) = self.client.delete_job(sid).await {
                self.status_message = format!("Failed to kill job: {}", e);
            } else {
                self.status_message = String::from("Job killed.");
                self.current_job_sid = None;
                self.current_job_status = None;
            }
        }
    }

    fn clear_results(&mut self) {
        self.search_results.clear();
        self.results_fetched = false;
        self.current_job_sid = None;
        self.current_job_status = None;
        self.scroll_offset = 0;
        self.status_message = String::from("Results cleared.");
    }

    fn open_in_editor(&mut self) {
        if self.search_results.is_empty() {
            self.status_message = String::from("No results to open.");
            return;
        }

        let mut temp_dir = std::env::temp_dir();
        temp_dir.push("splunk_results.json");
        let file_path = temp_dir.to_str().unwrap().to_string();

        if let Ok(mut file) = File::create(&file_path) {
            let json_content =
                serde_json::to_string_pretty(&self.search_results).unwrap_or_default();
            if file.write_all(json_content.as_bytes()).is_ok() {
                self.status_message = format!("Saved to {}. Opening...", file_path);
                self.editor_file_path = Some(file_path);
                self.should_open_editor = true;
            }
        }
    }

    fn open_query_in_editor(&mut self) {
        let mut temp_dir = std::env::temp_dir();
        temp_dir.push("splunk_query.spl");
        let file_path = temp_dir.to_str().unwrap().to_string();

        if let Ok(mut file) = File::create(&file_path) {
            if file.write_all(self.input.as_bytes()).is_ok() {
                self.status_message = "Editing query in external editor...".to_string();
                self.editor_file_path = Some(file_path);
                self.should_open_editor = true;
            }
        }
    }

    fn open_job_url(&mut self) {
        if let Some(sid) = &self.current_job_sid {
            let url = self.client.get_shareable_url(sid);
            if url.starts_with("http") {
                let _ = open::that(url);
                self.status_message = String::from("Opened URL in browser.");
            } else {
                self.status_message = String::from("Invalid URL.");
            }
        } else {
            self.status_message = String::from("No active job URL.");
        }
    }

    fn scroll_down(&mut self) {
        if !self.search_results.is_empty() {
            self.scroll_offset = self.scroll_offset.saturating_add(1);
        }
    }

    fn scroll_down_fast(&mut self) {
        if !self.search_results.is_empty() {
            self.scroll_offset = self.scroll_offset.saturating_add(10);
        }
    }

    fn scroll_up(&mut self) {
        if !self.search_results.is_empty() {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
        }
    }

    fn scroll_up_fast(&mut self) {
        if !self.search_results.is_empty() {
            self.scroll_offset = self.scroll_offset.saturating_sub(10);
        }
    }

    fn apply_theme(&mut self, theme_name: &str, save: bool) {
        self.theme = match theme_name {
            "Default" => {
                if let Some(t) = self.theme_set.themes.get("base16-ocean.dark") {
                    self.syntax_theme = t.clone();
                }
                AppTheme::default_theme()
            }
            "ColorPop" => {
                if let Some(t) = self.theme_set.themes.get("base16-eighties.dark") {
                    self.syntax_theme = t.clone();
                }
                AppTheme::color_pop()
            }
            "Splunk" => {
                if let Some(t) = self.theme_set.themes.get("base16-mocha.dark") {
                    self.syntax_theme = t.clone();
                } else if let Some(t) = self.theme_set.themes.get("InspiredGitHub") {
                    self.syntax_theme = t.clone();
                }
                AppTheme::splunk()
            }
            "Neon" => {
                if let Some(t) = self.theme_set.themes.get("base16-ocean.dark") {
                    self.syntax_theme = t.clone();
                }
                AppTheme::neon()
            }
            _ => AppTheme::default_theme(),
        };
        self.update_detail_view();
        if save {
            let _ = Config::save_theme(theme_name);
        }
    }

    fn toggle_theme_selector(&mut self) {
        self.input_mode = InputMode::ThemeSelect;
        self.theme_list_state.select(Some(0));
        self.status_message = String::from("Select theme (Up/Down/Enter), Esc to cancel.");
    }

    fn initiate_save_search(&mut self) {
        if self.input.trim().is_empty() {
            self.status_message = String::from("Cannot save empty search.");
            return;
        }

        if let Some(name) = &self.current_saved_search_name {
            self.input_mode = InputMode::ConfirmOverwrite;
            self.status_message = format!("Overwrite saved search '{}'? (y/n/r)", name);
        } else {
            self.input_mode = InputMode::SaveSearch;
            self.save_search_name.clear();
            self.status_message =
                String::from("Enter name for saved search (Enter to save, Esc to cancel):");
        }
    }

    fn save_current_search(&mut self) {
        let name = self.save_search_name.trim();
        if name.is_empty() {
            self.status_message = String::from("Name cannot be empty.");
            return;
        }

        if let Err(e) = SavedSearchManager::save_search(name, &self.input) {
            self.status_message = format!("Failed to save search: {}", e);
        } else {
            self.status_message = format!("Search saved as '{}'.", name);
            self.current_saved_search_name = Some(name.to_string());
            self.input_mode = InputMode::Normal;
        }
    }

    fn overwrite_current_search(&mut self) {
        if let Some(name) = self.current_saved_search_name.clone() {
            if let Err(e) = SavedSearchManager::save_search(&name, &self.input) {
                self.status_message = format!("Failed to save search: {}", e);
            } else {
                self.status_message = format!("Search '{}' overwritten.", name);
                self.input_mode = InputMode::Normal;
            }
        }
    }

    fn initiate_load_search(&mut self) {
        match SavedSearchManager::list_searches() {
            Ok(searches) => {
                if searches.is_empty() {
                    self.status_message = String::from("No saved searches found.");
                    return;
                }
                self.saved_searches = searches;
                self.input_mode = InputMode::LoadSearch;
                self.saved_search_list_state.select(Some(0));
                self.status_message =
                    String::from("Select saved search (Enter to load, Esc to cancel):");
            }
            Err(e) => {
                self.status_message = format!("Failed to list saved searches: {}", e);
            }
        }
    }

    fn load_selected_search(&mut self) {
        if let Some(idx) = self.saved_search_list_state.selected() {
            if let Some(name) = self.saved_searches.get(idx) {
                match SavedSearchManager::load_search(name) {
                    Ok(query) => {
                        self.input = query;
                        self.current_saved_search_name = Some(name.clone());
                        self.input_mode = InputMode::Normal;
                        self.status_message = format!("Loaded search '{}'.", name);
                        self.cursor_position = self.input.len(); // Reset cursor to end
                    }
                    Err(e) => {
                        self.status_message = format!("Failed to load search: {}", e);
                    }
                }
            }
        }
    }

    fn list_next(&mut self) {
        let i = match self.saved_search_list_state.selected() {
            Some(i) => {
                if i >= self.saved_searches.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.saved_search_list_state.select(Some(i));
    }

    fn list_previous(&mut self) {
        let i = match self.saved_search_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.saved_searches.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.saved_search_list_state.select(Some(i));
    }

    // --- Cursor Logic ---
    fn clamp_cursor(&mut self) {
        if self.cursor_position > self.input.len() {
            self.cursor_position = self.input.len();
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            // Find start of previous char (UTF-8 safe)
            let mut new_pos = self.cursor_position - 1;
            while new_pos > 0 && !self.input.is_char_boundary(new_pos) {
                new_pos -= 1;
            }
            self.cursor_position = new_pos;
        }
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            // Find start of next char
            let mut new_pos = self.cursor_position + 1;
            while new_pos < self.input.len() && !self.input.is_char_boundary(new_pos) {
                new_pos += 1;
            }
            self.cursor_position = new_pos;
        }
    }

    fn move_cursor_up(&mut self) {
        // Find the last newline before cursor.
        let cursor_byte_idx = self.cursor_position;
        let text_before = &self.input[..cursor_byte_idx];
        let last_newline = text_before.rfind('\n');

        if let Some(last_nl_idx) = last_newline {
            // We are not on the first line.
            let col = cursor_byte_idx - (last_nl_idx + 1);

            // Find the newline BEFORE that one to identify the previous line.
            let text_before_prev_line = &self.input[..last_nl_idx];
            let prev_line_start = text_before_prev_line
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);

            let prev_line_len = last_nl_idx - prev_line_start;
            let new_col = col.min(prev_line_len);

            self.cursor_position = prev_line_start + new_col;
        }
    }

    fn move_cursor_down(&mut self) {
        let cursor_byte_idx = self.cursor_position;

        // Find current line start and end
        let text_before = &self.input[..cursor_byte_idx];
        let line_start = text_before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = cursor_byte_idx - line_start;

        // Find next newline
        let text_after = &self.input[cursor_byte_idx..];
        if let Some(next_nl_rel) = text_after.find('\n') {
            let next_nl_idx = cursor_byte_idx + next_nl_rel;
            let next_line_start = next_nl_idx + 1;

            // Find end of next line
            let text_after_next_line = &self.input[next_line_start..];
            let next_line_end_rel = text_after_next_line
                .find('\n')
                .unwrap_or(text_after_next_line.len());
            let next_line_len = next_line_end_rel;

            let new_col = col.min(next_line_len);
            self.cursor_position = next_line_start + new_col;
        }
    }

    fn insert_char(&mut self, c: char) {
        self.clamp_cursor();
        self.input.insert(self.cursor_position, c);
        self.cursor_position += c.len_utf8();
    }

    fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.move_cursor_left(); // Go back one char
            self.input.remove(self.cursor_position);
            // Cursor position is already updated by move_cursor_left
        }
    }

    fn toggle_vim_mode(&mut self) {
        self.editor_mode = match self.editor_mode {
            EditorMode::Standard => EditorMode::Vim(VimState::Normal),
            EditorMode::Vim(_) => EditorMode::Standard,
        };
        // Ensure cursor is style updated by next render
    }

    fn update_detail_view(&mut self) {
        if self.view_mode == ViewMode::Table {
            let selected_idx = self.table_state.selected().unwrap_or(0);
            if let Some(item) = self.search_results.get(selected_idx) {
                self.cached_detail = render_yaml_detail(&self.syntax_set, &self.syntax_theme, item);
            } else {
                self.cached_detail = ratatui::text::Text::from("Select an event...");
            }
        }
    }
}

pub async fn run_app() -> Result<(), Box<dyn Error>> {
    let config = crate::config::Config::load()?;
    config.validate()?;
    info!("Loaded Config URL: '{}'", config.splunk_base_url);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let client = Arc::new(SplunkClient::new(
        config.splunk_base_url,
        config.splunk_token,
        config.splunk_verify_ssl,
    ));
    let app = Arc::new(Mutex::new(App::new(client)));

    let res = run_loop(&mut terminal, app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        SetCursorStyle::DefaultUserShape
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_loop<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: Arc<Mutex<App>>,
) -> Result<(), Box<dyn Error>>
where
    <B as Backend>::Error: 'static,
{
    let tick_rate = std::time::Duration::from_millis(250);
    let mut last_tick = std::time::Instant::now();

    loop {
        let mut app_guard = app.lock().await;

        if app_guard.should_open_editor {
            if let Some(file_path) = app_guard.editor_file_path.clone() {
                let is_editing_query = file_path.ends_with("splunk_query.spl");

                app_guard.should_open_editor = false;
                app_guard.editor_file_path = None;
                drop(app_guard);

                disable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
                terminal.show_cursor()?;

                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let _ = Command::new(editor)
                    .arg(&file_path)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status();

                enable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    EnterAlternateScreen,
                    EnableMouseCapture
                )?;
                terminal.hide_cursor()?;
                terminal.clear()?;

                app_guard = app.lock().await;

                // If we were editing the query, reload it
                if is_editing_query {
                    if let Ok(content) = std::fs::read_to_string(file_path) {
                        app_guard.input = content;
                        app_guard.cursor_position = app_guard.input.len(); // Reset cursor to end
                        app_guard.status_message = String::from("Query updated from editor.");
                    }
                }
            } else {
                app_guard.should_open_editor = false; // Reset if path missing
            }
        }

        // Set cursor style based on mode
        let cursor_style = match app_guard.input_mode {
            InputMode::Editing => match app_guard.editor_mode {
                EditorMode::Standard => SetCursorStyle::SteadyBar,
                EditorMode::Vim(VimState::Insert) => SetCursorStyle::SteadyBar,
                EditorMode::Vim(VimState::Normal) => SetCursorStyle::SteadyBlock,
            },
            _ => SetCursorStyle::DefaultUserShape,
        };
        // We can't easily execute! inside loop efficiently without check, but it's fine for TUI
        let _ = execute!(terminal.backend_mut(), cursor_style);

        terminal.draw(|f| ui(f, &mut app_guard))?;

        if last_tick.elapsed() >= tick_rate {
            // Check if we need to spawn a status check
            let needs_fetch = app_guard.current_job_sid.is_some()
                && !app_guard.results_fetched
                && !app_guard.is_status_fetching;

            if needs_fetch {
                app_guard.is_status_fetching = true;
                // Don't overwrite running message with "Checking status..."
                // app_guard.status_message = String::from("Checking status...");

                let client = app_guard.client.clone();
                let sid = app_guard.current_job_sid.as_ref().unwrap().clone();
                let app_clone = app.clone();

                tokio::spawn(async move {
                    // 1. Check Status
                    match client.get_job_status(&sid).await {
                        Ok(status) => {
                            let mut app = app_clone.lock().await;
                            app.current_job_status = Some(status.clone());

                            if status.is_done {
                                // 2. If done, Fetch Results (still in background task)
                                app.status_message = String::from("Job done. Fetching results...");
                                drop(app); // Drop lock while fetching results

                                match client.get_results(&sid, 100, 0).await {
                                    Ok(results) => {
                                        let mut app = app_clone.lock().await;
                                        app.search_results = results;
                                        app.results_fetched = true;
                                        app.status_message =
                                            format!("Loaded {} results.", app.search_results.len());
                                        app.is_status_fetching = false;
                                        if app.view_mode == ViewMode::Table {
                                            app.update_detail_view();
                                        }
                                    }
                                    Err(e) => {
                                        let mut app = app_clone.lock().await;
                                        error!("Failed to fetch results for job {}: {}", sid, e);
                                        app.status_message =
                                            format!("Failed to fetch results: {}", e);
                                        app.is_status_fetching = false;
                                    }
                                }
                            } else {
                                // Not done
                                app.status_message =
                                    format!("Job running... Dispatched: {}", status.dispatch_state);
                                app.is_status_fetching = false;
                            }
                        }
                        Err(e) => {
                            let mut app = app_clone.lock().await;
                            error!("Failed to check status for job {}: {}", sid, e);
                            app.is_status_fetching = false;
                        }
                    }
                });
            }
            last_tick = std::time::Instant::now();
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| std::time::Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            let evt = event::read()?;
            match evt {
                Event::Mouse(mouse_event) => {
                    match mouse_event.kind {
                        MouseEventKind::ScrollDown => {
                            if let ViewFocus::Search = app_guard.view_focus {
                                let line_count = app_guard.input.lines().count();
                                let max_scroll = line_count.saturating_sub(3); // 3 lines visible (header height 5)
                                if app_guard.input_scroll < max_scroll as u16 {
                                    app_guard.input_scroll =
                                        app_guard.input_scroll.saturating_add(1);
                                }
                            } else {
                                match app_guard.view_mode {
                                    ViewMode::RawEvents => app_guard.scroll_down(),
                                    ViewMode::Table => {
                                        match app_guard.view_focus {
                                            ViewFocus::ContentList => {
                                                // Scroll table
                                                let next = match app_guard.table_state.selected() {
                                                    Some(i) => {
                                                        if i >= app_guard
                                                            .search_results
                                                            .len()
                                                            .saturating_sub(1)
                                                        {
                                                            i // Stop at bottom (no wrap)
                                                        } else {
                                                            i + 1
                                                        }
                                                    }
                                                    None => 0,
                                                };
                                                if !app_guard.search_results.is_empty() {
                                                    app_guard.table_state.select(Some(next));
                                                    app_guard.detail_scroll = 0;
                                                    app_guard.update_detail_view();
                                                }
                                            }
                                            ViewFocus::ContentDetail => {
                                                app_guard.detail_scroll =
                                                    app_guard.detail_scroll.saturating_add(1);
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if let ViewFocus::Search = app_guard.view_focus {
                                app_guard.input_scroll = app_guard.input_scroll.saturating_sub(1);
                            } else {
                                match app_guard.view_mode {
                                    ViewMode::RawEvents => app_guard.scroll_up(),
                                    ViewMode::Table => {
                                        match app_guard.view_focus {
                                            ViewFocus::ContentList => {
                                                let prev = match app_guard.table_state.selected() {
                                                    Some(i) => {
                                                        if i == 0 {
                                                            0 // Stop at top (no wrap)
                                                        } else {
                                                            i - 1
                                                        }
                                                    }
                                                    None => 0,
                                                };
                                                if !app_guard.search_results.is_empty() {
                                                    app_guard.table_state.select(Some(prev));
                                                    app_guard.detail_scroll = 0;
                                                    app_guard.update_detail_view();
                                                }
                                            }
                                            ViewFocus::ContentDetail => {
                                                app_guard.detail_scroll =
                                                    app_guard.detail_scroll.saturating_sub(1);
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
                        let col = mouse_event.column;
                        let row = mouse_event.row;

                        if is_inside(app_guard.search_area, col, row) {
                            app_guard.view_focus = ViewFocus::Search;
                            app_guard.input_mode = InputMode::Editing;

                            // Calculate cursor position from mouse click
                            let rel_x = col.saturating_sub(app_guard.search_area.x + 1); // +1 for border
                            let rel_y = row.saturating_sub(app_guard.search_area.y + 1); // +1 for border

                            let target_line_idx = (rel_y + app_guard.input_scroll) as usize;
                            let target_col_idx = (rel_x + app_guard.input_scroll_x) as usize;

                            let lines: Vec<&str> = app_guard.input.lines().collect();
                            if target_line_idx < lines.len() {
                                let line = lines[target_line_idx];
                                // Calculate byte offset up to this line
                                let mut offset = 0;
                                for line in lines.iter().take(target_line_idx) {
                                    offset += line.len() + 1; // +1 for newline
                                }

                                // Add column offset (clamped to line length)
                                let col_bytes = line
                                    .chars()
                                    .take(target_col_idx)
                                    .map(|c| c.len_utf8())
                                    .sum::<usize>();
                                offset += col_bytes.min(line.len());

                                app_guard.cursor_position = offset;
                            } else if !lines.is_empty() {
                                // Clicked below text, move to end
                                app_guard.cursor_position = app_guard.input.len();
                            }
                        } else if is_inside(app_guard.main_area, col, row) {
                            app_guard.view_focus = ViewFocus::ContentList;
                        } else if is_inside(app_guard.detail_area, col, row) {
                            app_guard.view_focus = ViewFocus::ContentDetail;
                        }
                    }
                }
                Event::Key(key) => {
                    info!("Key event received: {:?}", key);
                    // Global Key Handlers (Pre-InputMode)
                    // Check for Ctrl + / (and variants like Ctrl + _ or Ctrl + ?)
                    if key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        match key.code {
                            KeyCode::Char('/')
                            | KeyCode::Char('_')
                            | KeyCode::Char('?')
                            | KeyCode::Char('7') => {
                                app_guard.input_mode = InputMode::Help;
                                // Skip further processing for this key to prevent typing it
                                continue;
                            }
                            _ => {}
                        }
                    }

                    match app_guard.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('e') => {
                                app_guard.input_mode = InputMode::Editing;
                                app_guard.status_message = String::from(
                                    "Editing... Press Enter to search, Esc to cancel.",
                                );
                                // If re-entering, ensure cursor is valid
                                app_guard.clamp_cursor();
                            }
                            KeyCode::Char('t')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.toggle_theme_selector();
                            }
                            // Help
                            KeyCode::Char('/')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.input_mode = InputMode::Help;
                            }
                            KeyCode::Char('t') => {
                                app_guard.toggle_theme_selector();
                            }
                            KeyCode::Char('q') => {
                                return Ok(());
                            }
                            KeyCode::Char('k')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.scroll_up_fast();
                            }
                            KeyCode::Char('x')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.open_in_editor();
                            }
                            // Rebind Clear to ^R (Reset) to free ^L for Fast Scroll
                            KeyCode::Char('r')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.clear_results();
                            }
                            // Fast Scroll
                            KeyCode::Char('j')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.scroll_down_fast();
                            }
                            KeyCode::Char('l')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.initiate_load_search();
                            }
                            // Open URL
                            KeyCode::Char('E')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::SHIFT) =>
                            {
                                app_guard.open_job_url();
                            }
                            // Saved Searches
                            KeyCode::Char('s')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                app_guard.initiate_save_search();
                            }

                            // Toggle View Mode (Ctrl+v OR Ctrl+m)
                            KeyCode::Char('v') | KeyCode::Char('m')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                match app_guard.view_mode {
                                    ViewMode::RawEvents => {
                                        app_guard.view_mode = ViewMode::Table;
                                        // Sync selection from scroll_offset
                                        let idx = app_guard.scroll_offset as usize;
                                        if idx < app_guard.search_results.len() {
                                            app_guard.table_state.select(Some(idx));
                                            app_guard.update_detail_view();
                                        } else if !app_guard.search_results.is_empty() {
                                            app_guard.table_state.select(Some(0));
                                            app_guard.update_detail_view();
                                        }
                                    }
                                    ViewMode::Table => {
                                        app_guard.view_mode = ViewMode::RawEvents;
                                        // Sync scroll_offset from table selection
                                        if let Some(idx) = app_guard.table_state.selected() {
                                            app_guard.scroll_offset = idx as u16;
                                            // Warning: truncation if > u16
                                        }
                                    }
                                }
                                app_guard.status_message =
                                    format!("Switched to {:?} mode.", app_guard.view_mode);
                            }

                            // Local Search Trigger
                            KeyCode::Char('/') => {
                                app_guard.input_mode = InputMode::LocalSearch;
                                app_guard.local_search_query.clear();
                                app_guard.status_message =
                                    String::from("Enter regex search query...");
                            }

                            // Local Search Navigation
                            KeyCode::Char('n') => {
                                app_guard.next_match();
                            }
                            KeyCode::Char('N') => {
                                app_guard.prev_match();
                            }

                            KeyCode::Tab => {
                                app_guard.view_focus = match app_guard.view_focus {
                                    ViewFocus::Search => ViewFocus::ContentList,
                                    ViewFocus::ContentList => {
                                        if app_guard.view_mode == ViewMode::Table {
                                            ViewFocus::ContentDetail
                                        } else {
                                            ViewFocus::Search
                                        }
                                    }
                                    ViewFocus::ContentDetail => ViewFocus::Search,
                                };
                            }

                            KeyCode::Left | KeyCode::Char('h') => {
                                app_guard.view_focus = ViewFocus::ContentList;
                            }

                            KeyCode::Right | KeyCode::Char('l')
                                if !key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                if app_guard.view_mode == ViewMode::Table {
                                    app_guard.view_focus = ViewFocus::ContentDetail;
                                }
                            }

                            KeyCode::Down | KeyCode::Char('j')
                                if !key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                if let ViewFocus::Search = app_guard.view_focus {
                                    // Optional: Down from Search goes to Content
                                    app_guard.view_focus = ViewFocus::ContentList;
                                } else {
                                    match app_guard.view_mode {
                                        ViewMode::RawEvents => app_guard.scroll_down(),
                                        ViewMode::Table => {
                                            match app_guard.view_focus {
                                                ViewFocus::ContentList => {
                                                    let next =
                                                        match app_guard.table_state.selected() {
                                                            Some(i) => {
                                                                if i >= app_guard
                                                                    .search_results
                                                                    .len()
                                                                    .saturating_sub(1)
                                                                {
                                                                    i // No wrap
                                                                } else {
                                                                    i + 1
                                                                }
                                                            }
                                                            None => 0,
                                                        };
                                                    if !app_guard.search_results.is_empty() {
                                                        app_guard.table_state.select(Some(next));
                                                        app_guard.detail_scroll = 0; // Reset detail scroll on row change
                                                        app_guard.update_detail_view();
                                                    }
                                                }
                                                ViewFocus::ContentDetail => {
                                                    app_guard.detail_scroll =
                                                        app_guard.detail_scroll.saturating_add(1);
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k')
                                if !key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                match app_guard.view_mode {
                                    ViewMode::RawEvents => app_guard.scroll_up(),
                                    ViewMode::Table => {
                                        match app_guard.view_focus {
                                            ViewFocus::ContentList => {
                                                let prev = match app_guard.table_state.selected() {
                                                    Some(i) => {
                                                        if i == 0 {
                                                            0 // No wrap
                                                        } else {
                                                            i - 1
                                                        }
                                                    }
                                                    None => 0,
                                                };
                                                if !app_guard.search_results.is_empty() {
                                                    app_guard.table_state.select(Some(prev));
                                                    app_guard.detail_scroll = 0;
                                                    app_guard.update_detail_view();
                                                }
                                            }
                                            ViewFocus::ContentDetail => {
                                                app_guard.detail_scroll =
                                                    app_guard.detail_scroll.saturating_sub(1);
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }

                            KeyCode::Char('x')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                if let ViewFocus::Search = app_guard.view_focus {
                                    app_guard.open_query_in_editor();
                                } else {
                                    app_guard.open_in_editor();
                                }
                            }
                            // 'x' mapping removed as requested
                            KeyCode::Enter => {
                                drop(app_guard);
                                let mut app_guard_search = app.lock().await;
                                app_guard_search.perform_search().await;
                                app_guard_search.input_mode = InputMode::Normal;
                            }
                            _ => {}
                        },
                        InputMode::Editing => {
                            // Toggle Vim Mode
                            if key.code == KeyCode::Char('v')
                                && key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL)
                            {
                                app_guard.toggle_vim_mode();
                                let mode_msg = match app_guard.editor_mode {
                                    EditorMode::Standard => "Standard Mode",
                                    EditorMode::Vim(_) => "Vim Mode",
                                };
                                app_guard.status_message = format!("Switched to {}.", mode_msg);
                                continue; // Skip other handlers
                            }

                            match app_guard.editor_mode {
                                EditorMode::Standard => match key.code {
                                    KeyCode::Enter
                                        if key
                                            .modifiers
                                            .contains(crossterm::event::KeyModifiers::SHIFT) =>
                                    {
                                        app_guard.insert_char('\n');
                                    }
                                    KeyCode::Enter => {
                                        drop(app_guard);
                                        let mut app_guard_search = app.lock().await;
                                        app_guard_search.perform_search().await;
                                        app_guard_search.input_mode = InputMode::Normal;
                                    }
                                    KeyCode::Char('j')
                                        if key
                                            .modifiers
                                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                                    {
                                        app_guard.insert_char('\n');
                                    }
                                    KeyCode::Char('x')
                                        if key
                                            .modifiers
                                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                                    {
                                        app_guard.open_query_in_editor();
                                    }
                                    KeyCode::Char(c) => {
                                        if !c.is_control() {
                                            app_guard.insert_char(c);
                                        }
                                    }
                                    KeyCode::Backspace => {
                                        app_guard.delete_char();
                                    }
                                    KeyCode::Left => app_guard.move_cursor_left(),
                                    KeyCode::Right => app_guard.move_cursor_right(),
                                    KeyCode::Up => app_guard.move_cursor_up(),
                                    KeyCode::Down => app_guard.move_cursor_down(),
                                    KeyCode::Esc => {
                                        app_guard.input_mode = InputMode::Normal;
                                        app_guard.status_message =
                                            String::from("Search cancelled.");
                                    }
                                    _ => {}
                                },
                                EditorMode::Vim(state) => match state {
                                    VimState::Normal => match key.code {
                                        KeyCode::Char('i') => {
                                            app_guard.editor_mode =
                                                EditorMode::Vim(VimState::Insert);
                                            app_guard.status_message = String::from("-- INSERT --");
                                        }
                                        KeyCode::Char('h') | KeyCode::Left => {
                                            app_guard.move_cursor_left()
                                        }
                                        KeyCode::Char('l') | KeyCode::Right => {
                                            app_guard.move_cursor_right()
                                        }
                                        KeyCode::Char('k') | KeyCode::Up => {
                                            app_guard.move_cursor_up()
                                        }
                                        KeyCode::Char('j') | KeyCode::Down => {
                                            app_guard.move_cursor_down()
                                        }
                                        KeyCode::Char('x') => { // Delete char in normal mode
                                             // vim 'x' deletes char under cursor.
                                             // Current delete_char deletes BEFORE cursor (backspace style).
                                             // We need delete_curr_char.
                                             // For now, let's just ignore or implement later.
                                             // Let's implement basics.
                                        }
                                        KeyCode::Enter => {
                                            // Allow search submission from Normal mode?
                                            // Usually 'Enter' in Normal mode goes down.
                                            // But this is a search editor.
                                            // Let's keep Enter to submit.
                                            drop(app_guard);
                                            let mut app_guard_search = app.lock().await;
                                            app_guard_search.perform_search().await;
                                            app_guard_search.input_mode = InputMode::Normal;
                                        }
                                        KeyCode::Esc => {
                                            // Exit editing entirely?
                                            app_guard.input_mode = InputMode::Normal;
                                            app_guard.status_message =
                                                String::from("Search cancelled.");
                                        }
                                        _ => {}
                                    },
                                    VimState::Insert => match key.code {
                                        KeyCode::Esc => {
                                            app_guard.editor_mode =
                                                EditorMode::Vim(VimState::Normal);
                                            app_guard.status_message = String::from("-- NORMAL --");
                                            app_guard.move_cursor_left(); // Vim usually moves cursor left on Esc
                                        }
                                        KeyCode::Enter
                                            if key.modifiers.contains(
                                                crossterm::event::KeyModifiers::SHIFT,
                                            ) =>
                                        {
                                            app_guard.insert_char('\n');
                                        }
                                        KeyCode::Enter => {
                                            drop(app_guard);
                                            let mut app_guard_search = app.lock().await;
                                            app_guard_search.perform_search().await;
                                            app_guard_search.input_mode = InputMode::Normal;
                                        }
                                        KeyCode::Char(c) => {
                                            if !c.is_control() {
                                                app_guard.insert_char(c);
                                            }
                                        }
                                        KeyCode::Backspace => app_guard.delete_char(),
                                        _ => {}
                                    },
                                },
                            }
                        }
                        InputMode::SaveSearch => match key.code {
                            KeyCode::Enter => {
                                app_guard.save_current_search();
                            }
                            KeyCode::Char(c) => {
                                app_guard.save_search_name.push(c);
                            }
                            KeyCode::Backspace => {
                                app_guard.save_search_name.pop();
                            }
                            KeyCode::Esc => {
                                app_guard.input_mode = InputMode::Normal;
                                app_guard.status_message = String::from("Save cancelled.");
                            }
                            _ => {}
                        },
                        InputMode::ConfirmOverwrite => match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                app_guard.overwrite_current_search();
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app_guard.input_mode = InputMode::Normal;
                                app_guard.status_message = String::from("Save cancelled.");
                            }
                            KeyCode::Char('r') | KeyCode::Char('R') => {
                                app_guard.input_mode = InputMode::SaveSearch;
                                app_guard.save_search_name = app_guard
                                    .current_saved_search_name
                                    .clone()
                                    .unwrap_or_default();
                                app_guard.status_message =
                                    String::from("Enter name for saved search:");
                            }
                            _ => {}
                        },
                        InputMode::LoadSearch => match key.code {
                            KeyCode::Enter => {
                                app_guard.load_selected_search();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app_guard.list_next();
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                app_guard.list_previous();
                            }
                            KeyCode::Esc => {
                                app_guard.input_mode = InputMode::Normal;
                                app_guard.status_message = String::from("Load cancelled.");
                            }
                            _ => {}
                        },
                        InputMode::LocalSearch => match key.code {
                            KeyCode::Enter => {
                                app_guard.perform_local_search();
                                app_guard.input_mode = InputMode::Normal;
                            }
                            KeyCode::Char(c) => {
                                app_guard.local_search_query.push(c);
                            }
                            KeyCode::Backspace => {
                                app_guard.local_search_query.pop();
                            }
                            KeyCode::Esc => {
                                app_guard.input_mode = InputMode::Normal;
                                app_guard.status_message = String::from("Local search cancelled.");
                            }
                            _ => {}
                        },
                        InputMode::ThemeSelect => match key.code {
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = match app_guard.theme_list_state.selected() {
                                    Some(i) => {
                                        if i >= app_guard.theme_options.len() - 1 {
                                            0
                                        } else {
                                            i + 1
                                        }
                                    }
                                    None => 0,
                                };
                                app_guard.theme_list_state.select(Some(i));
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = match app_guard.theme_list_state.selected() {
                                    Some(i) => {
                                        if i == 0 {
                                            app_guard.theme_options.len() - 1
                                        } else {
                                            i - 1
                                        }
                                    }
                                    None => 0,
                                };
                                app_guard.theme_list_state.select(Some(i));
                            }
                            KeyCode::Enter => {
                                if let Some(idx) = app_guard.theme_list_state.selected() {
                                    let theme_name = app_guard.theme_options[idx];
                                    app_guard.apply_theme(theme_name, true);
                                    app_guard.status_message =
                                        format!("Theme '{}' applied.", theme_name);
                                }
                                app_guard.input_mode = InputMode::Normal;
                            }
                            KeyCode::Esc => {
                                app_guard.input_mode = InputMode::Normal;
                                app_guard.status_message =
                                    String::from("Theme selection cancelled.");
                            }
                            _ => {}
                        },
                        InputMode::Help => match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                                app_guard.input_mode = InputMode::Normal;
                            }
                            _ => {}
                        },
                    } // End InputMode match
                } // End Key match
                _ => {}
            } // End Event match
        }
    }
}

fn recursive_json_parse(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, val) in map {
                new_map.insert(k, recursive_json_parse(val));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(recursive_json_parse).collect()),
        Value::String(s) => {
            // Attempt to parse string as JSON
            if let Ok(parsed) = serde_json::from_str::<Value>(&s) {
                // If it parses, recurse on the result in case it's nested string-json
                recursive_json_parse(parsed)
            } else {
                Value::String(s)
            }
        }
        _ => v,
    }
}

fn syntect_style_to_ratatui(style: syntect::highlighting::Style) -> Style {
    let mut s = Style::default();

    // Foreground
    if style.foreground.a > 0 {
        s = s.fg(Color::Rgb(
            style.foreground.r,
            style.foreground.g,
            style.foreground.b,
        ));
    }

    // Background - Ignored as requested ("Nothing should have background color")
    // if style.background.a > 0 {
    //     s = s.bg(Color::Rgb(style.background.r, style.background.g, style.background.b));
    // }

    // Font Style
    if style.font_style.contains(FontStyle::BOLD) {
        s = s.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        s = s.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        s = s.add_modifier(Modifier::UNDERLINED);
    }

    s
}

fn render_yaml_detail(
    syntax_set: &SyntaxSet,
    theme: &Theme,
    value: &Value,
) -> ratatui::text::Text<'static> {
    // 1. Recursive Parse
    let parsed_value = recursive_json_parse(value.clone());

    // 2. Convert to YAML
    let yaml_str = serde_yaml::to_string(&parsed_value)
        .unwrap_or_else(|e| format!("Error converting to YAML: {}", e));

    // 3. Highlight
    let syntax = syntax_set
        .find_syntax_by_extension("yaml")
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);

    let mut lines = Vec::new();
    for line in yaml_str.lines() {
        let ranges: Vec<(syntect::highlighting::Style, &str)> =
            h.highlight_line(line, syntax_set).unwrap_or_default();
        let spans: Vec<Span> = ranges
            .into_iter()
            .map(|(style, text)| Span::styled(text.to_string(), syntect_style_to_ratatui(style)))
            .collect();
        lines.push(Line::from(spans));
    }

    ratatui::text::Text::from(lines)
}

fn ui(f: &mut Frame, app: &mut App) {
    let header_height = 5; // Fixed height: 5 cells total = 3 content lines + 2 borders

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(header_height), // Header: Search + Sparkline
                Constraint::Length(1),             // Job Status (no block borders)
                Constraint::Min(10),               // Content
                Constraint::Length(1),             // Footer (Navigation, centered, one line)
            ]
            .as_ref(),
        )
        .split(f.area());

    // --- Header ---
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
        .split(chunks[0]);

    // Store search area for mouse interaction
    app.search_area = header_chunks[0];

    // 1. Search Input
    let input_style = match app.input_mode {
        InputMode::Normal => Style::default().fg(app.theme.text),
        InputMode::Editing => Style::default().fg(app.theme.input_edit),
        InputMode::LocalSearch => Style::default().fg(app.theme.input_edit),
        _ => Style::default().fg(app.theme.text),
    };

    // Input Area calculations
    let input_area_width = header_chunks[0].width.saturating_sub(2); // Minus borders
    let input_display_height = header_height.saturating_sub(2); // 1 line visible

    // Auto-scroll logic: Ensure cursor is visible
    let cursor_byte_idx = app.cursor_position;
    let text_before = &app.input[..cursor_byte_idx.min(app.input.len())];
    let cursor_line_idx = text_before.matches('\n').count() as u16;

    // Calculate cursor column (visual width check)
    let last_nl_idx = text_before.rfind('\n');
    let cursor_col_idx = if let Some(nl) = last_nl_idx {
        cursor_byte_idx - (nl + 1)
    } else {
        cursor_byte_idx
    } as u16;

    // Vertical Scroll
    if cursor_line_idx >= app.input_scroll + input_display_height {
        app.input_scroll = cursor_line_idx - input_display_height + 1;
    } else if cursor_line_idx < app.input_scroll {
        app.input_scroll = cursor_line_idx;
    }

    // Horizontal Scroll
    if cursor_col_idx >= app.input_scroll_x + input_area_width {
        app.input_scroll_x = cursor_col_idx - input_area_width + 1;
    } else if cursor_col_idx < app.input_scroll_x {
        app.input_scroll_x = cursor_col_idx;
    }

    let title = if let Some(name) = &app.current_saved_search_name {
        format!("SPL Search [{}]", name)
    } else {
        "SPL Search".to_string()
    };

    let input = Paragraph::new(app.input.as_str())
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .border_style(Style::default().fg(app.theme.title_main))
                .padding(Padding::horizontal(0)), // Remove padding to simplify scroll math
        )
        .scroll((app.input_scroll, app.input_scroll_x)); // Use both scroll offsets
    f.render_widget(input, header_chunks[0]);

    // 2. Sparkline
    let mut spark_data = vec![];
    if !app.search_results.is_empty() {
        // Simple bucketing of time
        // Note: parsing _time string requires chrono
        // format: 2023-10-27T10:00:00.000+00:00
        let timestamps: Vec<i64> = app
            .search_results
            .iter()
            .filter_map(|v| {
                v.get("_time").and_then(|t| t.as_str()).and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|dt| dt.timestamp())
                })
            })
            .collect();

        if !timestamps.is_empty() {
            let min = *timestamps.iter().min().unwrap_or(&0);
            let max = *timestamps.iter().max().unwrap_or(&0);
            if max > min {
                let range = max - min;
                let buckets = 40; // Approx width
                let mut counts = vec![0u64; buckets];
                for t in timestamps {
                    let idx = ((t - min) as f64 / range as f64 * (buckets - 1) as f64) as usize;
                    if idx < buckets {
                        counts[idx] += 1;
                    }
                }
                spark_data = counts;
            }
        }
    }

    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .title("Activity")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.border)),
        )
        .data(&spark_data)
        .style(Style::default().fg(app.theme.summary_highlight));
    f.render_widget(sparkline, header_chunks[1]);

    // --- Job Status (Middle 1) ---
    let mut stats_text = vec![];

    // Elapsed Time calculation
    let elapsed_text = if let Some(status) = &app.current_job_status {
        if status.is_done {
            String::new() // Don't show elapsed if done, rely on "Time" field
        } else if let Some(start_time) = app.job_created_at {
            let elapsed = start_time.elapsed().as_secs();
            format!("(Elapsed: {}s) ", elapsed)
        } else {
            String::new()
        }
    } else if let Some(start_time) = app.job_created_at {
        let elapsed = start_time.elapsed().as_secs();
        format!("(Elapsed: {}s) ", elapsed)
    } else {
        String::new()
    };

    if let Some(status) = &app.current_job_status {
        let mut line_vec = vec![
            Span::styled("Status: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(
                format!(
                    "{} {} ",
                    if status.is_done { "Done" } else { "Running" },
                    elapsed_text
                ),
                Style::default().fg(app.theme.text),
            ),
            Span::styled(" | Count: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(
                format!("{} ", status.result_count),
                Style::default().fg(app.theme.text),
            ),
            Span::styled(" | Time: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(
                format!("{:.2}s ", status.run_duration),
                Style::default().fg(app.theme.text),
            ),
        ];

        if let Some(sid) = &app.current_job_sid {
            let url = app.client.get_shareable_url(sid);
            line_vec.push(Span::styled(
                " | URL: ",
                Style::default().fg(app.theme.title_secondary),
            ));
            line_vec.push(Span::styled(
                url,
                Style::default().fg(app.theme.summary_highlight),
            ));
        }

        stats_text.push(Line::from(line_vec));
    } else if let Some(sid) = &app.current_job_sid {
        // Job created but status not yet fetched
        stats_text.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(
                format!("Running {} ", elapsed_text),
                Style::default().fg(app.theme.text),
            ),
            Span::styled(
                format!("(SID: {})", sid),
                Style::default().fg(app.theme.title_secondary),
            ),
        ]));
    } else {
        stats_text.push(Line::from("No active job."));
    }

    let stats_paragraph = Paragraph::new(stats_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(app.theme.text));
    f.render_widget(stats_paragraph, chunks[1]);

    // --- Results (Middle 2) ---
    let results_area = chunks[2];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(match app.view_mode {
            ViewMode::RawEvents => "Search Results (Raw)",
            ViewMode::Table => "Search Results (Table)",
        })
        .border_style(Style::default().fg(app.theme.border))
        .padding(Padding::new(2, 2, 1, 1));

    // Store main area default (RawEvents uses entire results_area)
    app.main_area = results_area;
    app.detail_area = Rect::default();

    if app.search_results.is_empty() {
        let text = Paragraph::new("No results available.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(app.theme.text))
            .block(block);
        f.render_widget(text, results_area);
    } else {
        match app.view_mode {
            ViewMode::RawEvents => {
                let mut content = vec![];
                for (i, result) in app.search_results.iter().enumerate() {
                    if i > 0 {
                        content.push(Line::from(Span::styled(
                            "-".repeat(results_area.width as usize - 6),
                            Style::default().fg(app.theme.border),
                        )));
                    }
                    if let Some(obj) = result.as_object() {
                        for (k, v) in obj {
                            if k.starts_with("_") && k != "_time" && k != "_raw" {
                                continue;
                            }
                            let val_str = if let Some(s) = v.as_str() {
                                s.to_string()
                            } else {
                                v.to_string()
                            };
                            content.push(Line::from(vec![
                                Span::styled(
                                    format!("{}: ", k),
                                    Style::default().fg(app.theme.summary_highlight),
                                ),
                                Span::styled(val_str, Style::default().fg(app.theme.text)),
                            ]));
                        }
                    } else {
                        content.push(Line::from(format!("{:?}", result)));
                    }
                }
                let paragraph = Paragraph::new(content)
                    .block(block)
                    .wrap(Wrap { trim: true })
                    .scroll((app.scroll_offset, 0));
                f.render_widget(paragraph, results_area);
            }
            ViewMode::Table => {
                // Determine active pane border color
                let table_border_style = if let ViewFocus::ContentList = app.view_focus {
                    Style::default().fg(app.theme.active_label)
                } else {
                    Style::default().fg(app.theme.border)
                };
                let detail_border_style = if let ViewFocus::ContentDetail = app.view_focus {
                    Style::default().fg(app.theme.active_label)
                } else {
                    Style::default().fg(app.theme.border)
                };

                // --- Left Pane: Table ---
                // "Time Sourcetype Host Message should not have a highlighted background. Instead, underline the table headers."
                // "In the Table View: Don't show Hosts."
                let header = Row::new(vec!["Time", "Sourcetype", "Message"])
                    .style(
                        Style::default()
                            .fg(app.theme.title_secondary)
                            .add_modifier(Modifier::UNDERLINED),
                    )
                    .bottom_margin(1);

                let rows: Vec<Row> = app
                    .search_results
                    .iter()
                    .map(|item| {
                        let time = item
                            .get("_time")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let sourcetype = item
                            .get("sourcetype")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        // Host removed
                        let msg = item
                            .get("_raw")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .lines()
                            .next()
                            .unwrap_or("")
                            .to_string();

                        Row::new(vec![time, sourcetype, msg])
                            .style(Style::default().fg(app.theme.text))
                    })
                    .collect();

                // NOTE: The `block` variable defined outside is used for the container border.
                // We render it first to set the boundary.
                f.render_widget(block.clone(), results_area);

                let inner_area = block.inner(results_area);
                let inner_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                    .split(inner_area);

                // Update layout areas for Table mode
                app.main_area = inner_chunks[0];
                app.detail_area = inner_chunks[1];

                // --- Left Pane: Table ---
                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(24), // Time
                        Constraint::Length(20), // Sourcetype
                        Constraint::Min(20),    // Message (Host removed)
                    ],
                )
                .header(header)
                .row_highlight_style(
                    Style::default()
                        .bg(app.theme.summary_highlight)
                        .fg(Color::White),
                )
                .highlight_symbol(">> ");

                // Render table directly into chunk, but we need to handle borders if we want distinct colors.
                // Since we render the outer block, inner widgets shouldn't necessarily have borders unless we want to override the middle separator?
                // The prompt asked: "When moving from the table panel to the details panel, can we use color line to indicate the active section."
                // We can wrap the Table in a Block with a colored border (Right side?).
                // Detail pane has Left side border.
                // Let's re-add blocks to inner widgets to control border color.

                // Use Right border for Table (left pane). Detail pane (right pane) has no left border.
                let table_block = Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(table_border_style);

                f.render_stateful_widget(
                    table.block(table_block),
                    inner_chunks[0],
                    &mut app.table_state,
                );

                // --- Right Pane: Detail ---
                // Use cached detail text
                let detail_paragraph = Paragraph::new(app.cached_detail.clone())
                    .block(
                        Block::default()
                            .borders(Borders::NONE) // Remove left border to avoid double
                            .border_style(detail_border_style)
                            .padding(Padding::new(1, 0, 0, 0)),
                    )
                    .wrap(Wrap { trim: false })
                    .scroll((app.detail_scroll, 0))
                    .style(Style::default().fg(app.theme.text));
                f.render_widget(detail_paragraph, inner_chunks[1]);
            }
        }
    }

    // --- Footer (Navigation) ---
    // "Remove the ' Navigation' and the line its on."
    // "At the bottom of the Search Results, there should just be one line, listing our navigation hints."
    // "Lets center it."
    // "lets remove ' ^J NewLine' from the nav bar."
    // "Remove the status line at the bottom"

    let mut footer_spans = vec![
        Span::styled(" e ", Style::default().fg(app.theme.title_main)),
        Span::styled("Search  |  ", Style::default().fg(app.theme.text)),
    ];
    if let Some(status) = &app.current_job_status {
        if !status.is_done {
            footer_spans.push(Span::styled(
                " x ",
                Style::default().fg(app.theme.title_main),
            ));
            footer_spans.push(Span::styled(
                "Kill Job  |  ",
                Style::default().fg(app.theme.text),
            ));
        }
    }
    footer_spans.extend(vec![
        Span::styled(" ↑/↓ ", Style::default().fg(app.theme.title_main)),
        Span::styled("Scroll  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^V ", Style::default().fg(app.theme.title_main)),
        Span::styled("View Mode  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^X ", Style::default().fg(app.theme.title_main)),
        Span::styled("Open in Editor  |  ", Style::default().fg(app.theme.text)),
        // Removed ^J NewLine
        Span::styled(" ^l ", Style::default().fg(app.theme.title_main)),
        Span::styled("Load  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^s ", Style::default().fg(app.theme.title_main)),
        Span::styled("Save  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" q ", Style::default().fg(app.theme.title_main)),
        Span::styled("Quit", Style::default().fg(app.theme.text)),
    ]);

    let footer = Paragraph::new(Line::from(footer_spans))
        .alignment(Alignment::Center)
        .style(Style::default().fg(app.theme.text));

    f.render_widget(footer, chunks[3]);

    // --- Modals ---
    if let InputMode::LocalSearch = app.input_mode {
        let area = centered_rect(60, 10, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let input_block = Paragraph::new(app.local_search_query.as_str())
            .style(Style::default().fg(app.theme.input_edit))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Local Search (Regex)")
                    .border_style(Style::default().fg(app.theme.title_main)),
            );
        f.render_widget(input_block, area);

        // Cursor for Local Search
        f.set_cursor_position(ratatui::layout::Position::new(
            area.x + 1 + app.local_search_query.len() as u16,
            area.y + 1,
        ));
    }

    if let InputMode::ThemeSelect = app.input_mode {
        let area = centered_rect(40, 40, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let items: Vec<ListItem> = app
            .theme_options
            .iter()
            .map(|i| ListItem::new(*i).style(Style::default().fg(app.theme.text)))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Select Theme")
                    .border_style(Style::default().fg(app.theme.title_main)),
            )
            .highlight_style(
                Style::default()
                    .bg(app.theme.summary_highlight)
                    .fg(Color::White),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut app.theme_list_state);
    }

    if let InputMode::Help = app.input_mode {
        let area = centered_rect(60, 80, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let shortcuts = vec![
            ("General", ""),
            ("Ctrl+/", "Show this Help"),
            ("q", "Quit"),
            ("e", "Enter Search Input Mode"),
            ("Ctrl+t", "Toggle Theme"),
            ("", ""),
            ("Search Input", ""),
            ("Enter", "Run Search"),
            ("Shift+Enter", "Newline (Standard Mode)"),
            ("Ctrl+x", "Edit Query in External Editor"),
            ("Ctrl+v", "Toggle Vim/Standard Mode"),
            ("Ctrl+s", "Save Search"),
            ("", ""),
            ("Results & Navigation", ""),
            ("j / k / Down / Up", "Scroll / Navigate"),
            ("Ctrl+j / Ctrl+k", "Fast Scroll"),
            ("Ctrl+r", "Clear Results"),
            ("Ctrl+l", "Load Saved Search"),
            ("Shift+E", "Open Job in Browser"),
            ("Ctrl+v / Ctrl+m", "Toggle Raw/Table View"),
            ("Ctrl+x", "Open Results in External Editor"),
            ("/ / n / N", "Local Regex Search / Next / Prev"),
            ("", ""),
            ("Pane Navigation", ""),
            ("Tab", "Cycle Focus (Search > List > Detail)"),
            ("h / l / Left / Right", "Focus Panes"),
        ];

        let rows: Vec<Row> = shortcuts
            .iter()
            .map(|(k, d)| {
                let style = if d.is_empty() {
                    Style::default()
                        .fg(app.theme.title_secondary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.text)
                };
                Row::new(vec![k.to_string(), d.to_string()]).style(style)
            })
            .collect();

        let table = Table::new(rows, [Constraint::Length(25), Constraint::Min(30)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Keyboard Shortcuts")
                .border_style(Style::default().fg(app.theme.title_main)),
        );

        f.render_widget(table, area);
    }

    if let InputMode::SaveSearch = app.input_mode {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let input_block = Paragraph::new(app.save_search_name.as_str())
            .style(Style::default().fg(app.theme.input_edit))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Save Search As")
                    .border_style(Style::default().fg(app.theme.title_main)),
            );
        f.render_widget(input_block, area);
    }

    if let InputMode::ConfirmOverwrite = app.input_mode {
        let area = centered_rect(60, 10, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let msg = Paragraph::new("Press 'y' to overwrite, 'n' to cancel, 'r' to rename.")
            .style(Style::default().fg(app.theme.text))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Confirm Overwrite")
                    .border_style(Style::default().fg(app.theme.evilness_label)),
            ); // Use red for warning
        f.render_widget(msg, area);
    }

    if let InputMode::LoadSearch = app.input_mode {
        let area = centered_rect(60, 40, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let items: Vec<ListItem> = app
            .saved_searches
            .iter()
            .map(|i| ListItem::new(i.as_str()).style(Style::default().fg(app.theme.text)))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Saved Searches")
                    .border_style(Style::default().fg(app.theme.title_main)),
            )
            .highlight_style(
                Style::default()
                    .bg(app.theme.summary_highlight)
                    .fg(Color::White),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut app.saved_search_list_state);
    }

    // Set cursor
    if let InputMode::Editing = app.input_mode {
        // displayed_y = cursor_line_idx - app.input_scroll
        let displayed_y = cursor_line_idx.saturating_sub(app.input_scroll);
        let displayed_x = cursor_col_idx.saturating_sub(app.input_scroll_x);

        // Ensure cursor is within displayed area
        if displayed_y < input_display_height && displayed_x < input_area_width {
            f.set_cursor_position(ratatui::layout::Position::new(
                header_chunks[0].x + 1 + displayed_x, // +1 for border
                header_chunks[0].y + 1 + displayed_y, // +1 for border
            ));
        }
    } else if let InputMode::SaveSearch = app.input_mode {
        let area = centered_rect(60, 20, f.area());
        f.set_cursor_position(ratatui::layout::Position::new(
            area.x + 1 + app.save_search_name.len() as u16,
            area.y + 1,
        ));
    }
}

// Helper to center a rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    let vertical_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1]);

    vertical_layout[1]
}
