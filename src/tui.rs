use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    cursor::SetCursorStyle,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Padding, BorderType, Wrap, List, ListItem, ListState},
    Frame, Terminal,
};
use std::{error::Error, io, sync::Arc};
use std::process::{Command, Stdio};
use std::fs::File;
use std::io::Write;
use tokio::sync::Mutex;
use crate::api::SplunkClient;
use crate::models::splunk::JobStatus;
use crate::utils::saved_searches::SavedSearchManager;
use serde_json::Value;
use log::{info, error};

#[derive(Clone, Copy)]
pub enum ThemeVariant {
    Default,
    ColorPop,
    Splunk,
}

#[derive(Clone)]
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
}

enum InputMode {
    Normal,
    Editing,
    SaveSearch,
    LoadSearch,
    ConfirmOverwrite,
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

pub struct App {
    input: String,
    input_scroll: u16,
    input_mode: InputMode,
    client: Arc<SplunkClient>,
    status_message: String,
    pub theme: AppTheme,

    // Search State
    current_job_sid: Option<String>,
    current_job_status: Option<JobStatus>,
    search_results: Vec<Value>,
    results_fetched: bool,
    scroll_offset: u16,

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

    // Timing
    job_created_at: Option<std::time::Instant>,
}

impl App {
    pub fn new(client: Arc<SplunkClient>) -> App {
        App {
            input: String::new(),
            input_scroll: 0,
            input_mode: InputMode::Normal,
            client,
            status_message: String::from("Press 'q' to quit, 'e' to enter search mode, 't' to toggle theme."),
            theme: AppTheme::default_theme(),
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
            job_created_at: None,
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

    pub async fn update_job_status(&mut self) {
        // Deprecated: Logic moved to background task in run_loop to avoid blocking UI
    }

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
            let json_content = serde_json::to_string_pretty(&self.search_results).unwrap_or_default();
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
                self.status_message = format!("Editing query in external editor...");
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

    fn toggle_theme(&mut self) {
        self.theme = match self.theme.variant {
            ThemeVariant::Default => AppTheme::color_pop(),
            ThemeVariant::ColorPop => AppTheme::splunk(),
            ThemeVariant::Splunk => AppTheme::default_theme(),
        };
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
            self.status_message = String::from("Enter name for saved search (Enter to save, Esc to cancel):");
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
                self.status_message = String::from("Select saved search (Enter to load, Esc to cancel):");
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
            let prev_line_start = text_before_prev_line.rfind('\n').map(|i| i + 1).unwrap_or(0);

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
            let next_line_end_rel = text_after_next_line.find('\n').unwrap_or(text_after_next_line.len());
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

    let client = Arc::new(SplunkClient::new(config.splunk_base_url, config.splunk_token, config.splunk_verify_ssl));
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

async fn run_loop<B: Backend + std::io::Write>(terminal: &mut Terminal<B>, app: Arc<Mutex<App>>) -> io::Result<()> {
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
                execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                terminal.show_cursor()?;

                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let _ = Command::new(editor)
                    .arg(&file_path)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status();

                enable_raw_mode()?;
                execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
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
                                        app.status_message = format!("Loaded {} results.", app.search_results.len());
                                        app.is_status_fetching = false;
                                    },
                                    Err(e) => {
                                        let mut app = app_clone.lock().await;
                                        error!("Failed to fetch results for job {}: {}", sid, e);
                                        app.status_message = format!("Failed to fetch results: {}", e);
                                        app.is_status_fetching = false;
                                    }
                                }
                            } else {
                                // Not done
                                app.status_message = format!("Job running... Dispatched: {}", status.dispatch_state);
                                app.is_status_fetching = false;
                            }
                        },
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
            if let Event::Key(key) = event::read()? {
                match app_guard.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('e') => {
                            app_guard.input_mode = InputMode::Editing;
                            app_guard.status_message = String::from("Editing... Press Enter to search, Esc to cancel.");
                            // If re-entering, ensure cursor is valid
                            app_guard.clamp_cursor();
                        }
                        KeyCode::Char('t') => {
                            app_guard.toggle_theme();
                        }
                        KeyCode::Char('q') => {
                            return Ok(());
                        }
                        KeyCode::Char('k') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.kill_search().await;
                        }
                        KeyCode::Char('x') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.open_in_editor();
                        }
                        // Rebind Clear to ^R (Reset) to free ^L for Fast Scroll
                        KeyCode::Char('r') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.clear_results();
                        }
                        // Fast Scroll
                        KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.scroll_down_fast();
                        }
                        KeyCode::Char('l') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.scroll_up_fast();
                        }
                        // Open URL
                        KeyCode::Char('E') if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
                             app_guard.open_job_url();
                        }
                        // Saved Searches
                        KeyCode::Char('s') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.initiate_load_search();
                        }
                        KeyCode::Char('w') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.initiate_save_search();
                        }

                        KeyCode::Down | KeyCode::Char('j') => {
                            app_guard.scroll_down();
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app_guard.scroll_up();
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
                        if key.code == KeyCode::Char('v') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                            app_guard.toggle_vim_mode();
                            let mode_msg = match app_guard.editor_mode {
                                EditorMode::Standard => "Standard Mode",
                                EditorMode::Vim(_) => "Vim Mode",
                            };
                            app_guard.status_message = format!("Switched to {}.", mode_msg);
                            continue; // Skip other handlers
                        }

                        match app_guard.editor_mode {
                            EditorMode::Standard => {
                                match key.code {
                                    KeyCode::Enter if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
                                        app_guard.insert_char('\n');
                                    }
                                    KeyCode::Enter => {
                                        drop(app_guard);
                                        let mut app_guard_search = app.lock().await;
                                        app_guard_search.perform_search().await;
                                        app_guard_search.input_mode = InputMode::Normal;
                                    }
                                    KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                        app_guard.insert_char('\n');
                                    }
                                    KeyCode::Char('x') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
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
                                        app_guard.status_message = String::from("Search cancelled.");
                                    }
                                    _ => {}
                                }
                            }
                            EditorMode::Vim(state) => match state {
                                VimState::Normal => match key.code {
                                    KeyCode::Char('i') => {
                                        app_guard.editor_mode = EditorMode::Vim(VimState::Insert);
                                        app_guard.status_message = String::from("-- INSERT --");
                                    }
                                    KeyCode::Char('h') | KeyCode::Left => app_guard.move_cursor_left(),
                                    KeyCode::Char('l') | KeyCode::Right => app_guard.move_cursor_right(),
                                    KeyCode::Char('k') | KeyCode::Up => app_guard.move_cursor_up(),
                                    KeyCode::Char('j') | KeyCode::Down => app_guard.move_cursor_down(),
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
                                        app_guard.status_message = String::from("Search cancelled.");
                                    }
                                    _ => {}
                                },
                                VimState::Insert => match key.code {
                                    KeyCode::Esc => {
                                        app_guard.editor_mode = EditorMode::Vim(VimState::Normal);
                                        app_guard.status_message = String::from("-- NORMAL --");
                                        app_guard.move_cursor_left(); // Vim usually moves cursor left on Esc
                                    }
                                    KeyCode::Enter if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
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
                                }
                            }
                        }
                    },
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
                            app_guard.save_search_name = app_guard.current_saved_search_name.clone().unwrap_or_default();
                            app_guard.status_message = String::from("Enter name for saved search:");
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
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    // Determine dynamic input height
    let input_lines = app.input.lines().count().max(1) as u16;
    let max_height = f.area().height / 2;
    let desired_input_height = input_lines + 2;
    let actual_input_height = desired_input_height.min(max_height);
    let header_height = actual_input_height + 3;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(header_height), // Dynamic Header
                Constraint::Min(10),   // Results
                Constraint::Length(3), // Footer
            ]
            .as_ref(),
        )
        .split(f.area());

    // --- Header ---
    let header_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(actual_input_height), Constraint::Length(3)])
        .split(chunks[0]);

    let input_style = match app.input_mode {
        InputMode::Normal => Style::default().fg(app.theme.text),
        InputMode::Editing => Style::default().fg(app.theme.input_edit),
        _ => Style::default().fg(app.theme.text),
    };

    let input_display_height = actual_input_height.saturating_sub(2);
    let mut input_scroll = 0;

    // Auto-scroll logic: Ensure cursor is visible
    // We need to know which line the cursor is on.
    let cursor_byte_idx = app.cursor_position;
    let text_before = &app.input[..cursor_byte_idx.min(app.input.len())];
    let cursor_line_idx = text_before.matches('\n').count() as u16;

    if cursor_line_idx >= input_display_height {
        input_scroll = cursor_line_idx - input_display_height + 1;
    }
    app.input_scroll = input_scroll;

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
                .padding(Padding::horizontal(1)),
        )
        .scroll((input_scroll, 0));
    f.render_widget(input, header_chunks[0]);

    // Job Stats & Results logic
    let mut stats_text = vec![];

    // Elapsed Time calculation
    let elapsed_text = if let Some(start_time) = app.job_created_at {
        let elapsed = start_time.elapsed().as_secs();
        format!("(Elapsed: {}s) ", elapsed)
    } else {
        String::new()
    };

    if let Some(status) = &app.current_job_status {
        stats_text.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(format!("{} {} ", if status.is_done { "Done" } else { "Running" }, elapsed_text), Style::default().fg(app.theme.text)),
            Span::styled(" | Count: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(format!("{} ", status.result_count), Style::default().fg(app.theme.text)),
            Span::styled(" | Time: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(format!("{:.2}s ", status.run_duration), Style::default().fg(app.theme.text)),
            Span::styled(" | State: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(format!("{}", status.dispatch_state), Style::default().fg(app.theme.text)),
        ]));

        if let Some(sid) = &app.current_job_sid {
            let url = app.client.get_shareable_url(sid);
            stats_text.push(Line::from(vec![
                Span::styled("URL: ", Style::default().fg(app.theme.title_secondary)),
                Span::styled(url, Style::default().fg(app.theme.summary_highlight)),
            ]));
        }
    } else if let Some(sid) = &app.current_job_sid {
        // Job created but status not yet fetched
        stats_text.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(format!("Running {} ", elapsed_text), Style::default().fg(app.theme.text)),
            Span::styled(format!("(SID: {})", sid), Style::default().fg(app.theme.title_secondary)),
        ]));
    } else {
        stats_text.push(Line::from("No active job."));
    }

    let stats_paragraph = Paragraph::new(stats_text)
        .block(
            Block::default()
                // Removed borders(Borders::BOTTOM) as requested
                .title("Job Status")
                .border_style(Style::default().fg(app.theme.title_main))
                .padding(Padding::horizontal(4)),
        )
        .style(Style::default().fg(app.theme.text));

    f.render_widget(stats_paragraph, header_chunks[1]);

    // --- Results ---
    let results_area = chunks[1];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("Search Results")
        .border_style(Style::default().fg(app.theme.border))
        .padding(Padding::new(2, 2, 1, 1));

    if app.search_results.is_empty() {
        let text = Paragraph::new("No results available.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(app.theme.text))
            .block(block);
        f.render_widget(text, results_area);
    } else {
        let mut content = vec![];
        for (i, result) in app.search_results.iter().enumerate() {
            if i > 0 {
                content.push(Line::from(Span::styled("-".repeat(results_area.width as usize - 6), Style::default().fg(app.theme.border))));
            }
            if let Some(obj) = result.as_object() {
                 for (k, v) in obj {
                     if k.starts_with("_") && k != "_time" && k != "_raw" { continue; }
                     let val_str = if let Some(s) = v.as_str() { s.to_string() } else { v.to_string() };
                     content.push(Line::from(vec![
                         Span::styled(format!("{}: ", k), Style::default().fg(app.theme.summary_highlight)),
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

    // --- Footer ---
    let mut footer_spans = vec![
        Span::styled(" e ", Style::default().fg(app.theme.title_main)),
        Span::styled("Search  |  ", Style::default().fg(app.theme.text)),
    ];
    if let Some(status) = &app.current_job_status {
        if !status.is_done {
            footer_spans.push(Span::styled(" x ", Style::default().fg(app.theme.title_main)));
            footer_spans.push(Span::styled("Kill Job  |  ", Style::default().fg(app.theme.text)));
        }
    }
    footer_spans.extend(vec![
        Span::styled(" ↑/↓ ", Style::default().fg(app.theme.title_main)),
        Span::styled("Scroll  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^X ", Style::default().fg(app.theme.title_main)),
        Span::styled("Open in Editor  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^L ", Style::default().fg(app.theme.title_main)),
        Span::styled("Clear  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^J ", Style::default().fg(app.theme.title_main)),
        Span::styled("NewLine  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^S ", Style::default().fg(app.theme.title_main)),
        Span::styled("Load  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" ^W ", Style::default().fg(app.theme.title_main)),
        Span::styled("Save  |  ", Style::default().fg(app.theme.text)),
        Span::styled(" q ", Style::default().fg(app.theme.title_main)),
        Span::styled("Quit", Style::default().fg(app.theme.text)),
    ]);

    let footer_text = vec![
        Line::from(footer_spans),
        Line::from(Span::styled(app.status_message.clone(), Style::default().fg(app.theme.text))),
    ];
    let footer = Paragraph::new(footer_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Navigation")
                .border_style(Style::default().fg(app.theme.border))
                .padding(Padding::horizontal(4)),
        );
    f.render_widget(footer, chunks[2]);

    // --- Modals ---
    if let InputMode::SaveSearch = app.input_mode {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let input_block = Paragraph::new(app.save_search_name.as_str())
            .style(Style::default().fg(app.theme.input_edit))
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Save Search As")
                .border_style(Style::default().fg(app.theme.title_main)));
        f.render_widget(input_block, area);
    }

    if let InputMode::ConfirmOverwrite = app.input_mode {
        let area = centered_rect(60, 10, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let msg = Paragraph::new("Press 'y' to overwrite, 'n' to cancel, 'r' to rename.")
            .style(Style::default().fg(app.theme.text))
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Confirm Overwrite")
                .border_style(Style::default().fg(app.theme.evilness_label))); // Use red for warning
        f.render_widget(msg, area);
    }

    if let InputMode::LoadSearch = app.input_mode {
        let area = centered_rect(60, 40, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let items: Vec<ListItem> = app.saved_searches
            .iter()
            .map(|i| ListItem::new(i.as_str()).style(Style::default().fg(app.theme.text)))
            .collect();

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Saved Searches")
                .border_style(Style::default().fg(app.theme.title_main)))
            .highlight_style(Style::default().bg(app.theme.summary_highlight).fg(Color::White))
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut app.saved_search_list_state);
    }

    // Set cursor
    if let InputMode::Editing = app.input_mode {
        // Calculate visual cursor position from byte index
        // We found cursor_line_idx and col earlier implicitly
        let cursor_byte_idx = app.cursor_position;
        let text_before = &app.input[..cursor_byte_idx.min(app.input.len())];
        let last_nl_idx = text_before.rfind('\n');
        let col = if let Some(nl) = last_nl_idx {
            cursor_byte_idx - (nl + 1)
        } else {
            cursor_byte_idx
        };

        // displayed_y = cursor_line_idx - app.input_scroll
        let cursor_line_idx = text_before.matches('\n').count() as u16;
        let displayed_y = cursor_line_idx.saturating_sub(app.input_scroll);

        // Ensure cursor is within displayed area
        if displayed_y < input_display_height {
             f.set_cursor_position(ratatui::layout::Position::new(
                header_chunks[0].x + 1 + 1 + col as u16,
                header_chunks[0].y + 1 + displayed_y,
            ));
        }
    } else if let InputMode::SaveSearch = app.input_mode {
         let area = centered_rect(60, 20, f.area());
         f.set_cursor_position(ratatui::layout::Position::new(
             area.x + 1 + app.save_search_name.len() as u16,
             area.y + 1
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
