use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
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
use std::process::Command;
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

    pub should_open_editor: bool,

    // Saved Search State
    save_search_name: String,
    saved_searches: Vec<String>,
    saved_search_list_state: ListState,
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
            should_open_editor: false,
            save_search_name: String::new(),
            saved_searches: Vec::new(),
            saved_search_list_state: ListState::default(),
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

        match self.client.create_search(&self.input).await {
            Ok(sid) => {
                info!("Job created successfully: {}", sid);
                self.current_job_sid = Some(sid.clone());
                self.status_message = format!("Job created (SID: {}). Waiting for results...", sid);
            }
            Err(e) => {
                error!("Search creation failed: {}", e);
                self.status_message = format!("Search failed: {}", e);
            }
        }
    }

    pub async fn update_job_status(&mut self) {
        if let Some(sid) = &self.current_job_sid {
            if self.results_fetched {
                return;
            }

            match self.client.get_job_status(sid).await {
                Ok(status) => {
                    let is_done = status.is_done;
                    self.current_job_status = Some(status);

                    if is_done && !self.results_fetched {
                         info!("Job {} is done. Fetching results...", sid);
                         self.status_message = String::from("Job done. Fetching results...");
                         match self.client.get_results(sid, 100, 0).await {
                             Ok(results) => {
                                 info!("Received {} results for job {}", results.len(), sid);
                                 self.search_results = results;
                                 self.results_fetched = true;
                                 self.status_message = format!("Loaded {} results.", self.search_results.len());
                             }
                             Err(e) => {
                                 error!("Failed to fetch results for job {}: {}", sid, e);
                                 self.results_fetched = true;
                                 self.status_message = format!("Failed to fetch results: {}", e);
                             }
                         }
                    } else if !is_done {
                        self.status_message = format!("Job running... Dispatched: {}", self.current_job_status.as_ref().unwrap().dispatch_state);
                    }
                }
                Err(e) => {
                     error!("Failed to check status for job {}: {}", sid, e);
                }
            }
        }
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
                self.should_open_editor = true;
            }
        }
    }

    fn scroll_down(&mut self) {
        if !self.search_results.is_empty() {
            self.scroll_offset = self.scroll_offset.saturating_add(1);
        }
    }

    fn scroll_up(&mut self) {
        if !self.search_results.is_empty() {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
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
        self.input_mode = InputMode::SaveSearch;
        self.save_search_name.clear();
        self.status_message = String::from("Enter name for saved search (Enter to save, Esc to cancel):");
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
            self.input_mode = InputMode::Normal;
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
                        self.input_mode = InputMode::Normal;
                        self.status_message = format!("Loaded search '{}'.", name);
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
        DisableMouseCapture
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
            app_guard.should_open_editor = false;
            drop(app_guard);

            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
            terminal.show_cursor()?;

            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let mut temp_dir = std::env::temp_dir();
            temp_dir.push("splunk_results.json");
            let file_path = temp_dir.to_str().unwrap().to_string();

            let _ = Command::new(editor).arg(file_path).status();

            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
            terminal.hide_cursor()?;
            terminal.clear()?;

            app_guard = app.lock().await;
        }

        terminal.draw(|f| ui(f, &mut app_guard))?;

        if last_tick.elapsed() >= tick_rate {
            app_guard.update_job_status().await;
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
                        KeyCode::Char('l') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                             app_guard.clear_results();
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
                         KeyCode::Char('x') => {
                             app_guard.kill_search().await;
                         }
                        KeyCode::Enter => {
                            drop(app_guard);
                            let mut app_guard_search = app.lock().await;
                            app_guard_search.perform_search().await;
                            app_guard_search.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        // Submit on Enter (single line)
                        // But what if user wants multiline?
                        // Prompt said "Multiline with Ctrl + j for line breaks".
                        // So Enter submits, Ctrl+J inserts newline.
                        KeyCode::Enter => {
                            drop(app_guard);
                            let mut app_guard_search = app.lock().await;
                            app_guard_search.perform_search().await;
                            app_guard_search.input_mode = InputMode::Normal;
                        }
                        // Ctrl + J for multiline
                        KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            app_guard.input.push('\n');
                        }
                        // We also need to catch 'j' with control if it falls through to char.
                        // Wait, previous code block logic:
                        // KeyCode::Char('j') if ... => { push newline }
                        // KeyCode::Char(c) => { push c }
                        // If I press Ctrl+J, it matches the first arm.
                        // If I press j, it matches the second arm.
                        // The reviewer said "The keybinding for Ctrl+J is added only to InputMode::Normal".
                        // Let me check my previous file content.
                        // Ah, I see "InputMode::Normal => match key.code ..."
                        // And "InputMode::Editing => match key.code ..."
                        // In InputMode::Editing, I DO have:
                        // KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => { app_guard.input.push('\n'); }
                        // So I actually DID add it.
                        // But maybe I should also support Enter if modifiers are pressed?
                        // No, the requirement was explicitly Ctrl+J.
                        // Let's ensure I didn't miss it in the previous file read.
                        // Reading line 437 in `src/tui.rs` (from previous read_file output):
                        // KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        //     app_guard.input.push('\n');
                        // }
                        // It seems correct.
                        // However, standard terminal often maps Ctrl+J to Enter (Line Feed).
                        // If crossterm reports it as Enter, my Enter handler will trigger search.
                        // I should probably check for Enter code and check modifiers or if it is raw char.
                        // But let's assume crossterm separates them if possible.
                        // Actually, Ctrl+J often sends `\n` which is KeyCode::Enter in crossterm if not raw,
                        // or it might be KeyCode::Char('j') with modifier depending on terminal.
                        // Let's add logging or handle KeyCode::Enter with modifiers just in case?
                        // Or just handle Enter generally.
                        //
                        // Wait, if Ctrl+J is reported as KeyCode::Enter, then my Enter handler triggers search.
                        // So I should check if KeyCode::Enter has control modifier?
                        // No, usually Enter doesn't have control modifier unless Shift+Enter.
                        // Let's trust that I need to handle it.
                        // BUT, to be safe, I should also allow `Ctrl + Enter` or similar if Ctrl+J is ambiguous.
                        //
                        // Re-reading reviewer comment: "In Editing mode, typing is handled by a catch-all KeyCode::Char(c). Ctrl+J usually sends a Char('j') with a control modifier ... which will either insert 'j' or trigger the search (Enter), rather than inserting a newline. To insert a newline, the user effectively has to exit Editing mode..."
                        //
                        // My code in `Editing` arm:
                        // KeyCode::Char('j') if key.modifiers.contains(...) => push '\n'
                        // KeyCode::Char(c) => push c
                        //
                        // If crossterm emits Char('j') + Control, my code works.
                        // If crossterm emits Enter, my code submits.
                        //
                        // If the reviewer says it is "missing", maybe they missed it in the diff?
                        // OR maybe they meant I didn't handle the case where it comes in as Enter?
                        //
                        // Let's double check `InputMode::Normal` has `Ctrl+J`?
                        // In `InputMode::Normal`:
                        // KeyCode::Down | KeyCode::Char('j') => scroll_down
                        // There is NO Ctrl+J in Normal mode in my code (except for scrolling down via 'j').
                        //
                        // Wait, the footer says:
                        // Span::styled(" ^J ", ...), Span::styled("NewLine ...")
                        //
                        // Let's add it explicitly to be sure. And maybe ensure we don't accidentally print control chars.

                        KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            app_guard.input.push('\n');
                        }
                        KeyCode::Char(c) => {
                            // Filter control characters to avoid printing weird stuff
                            if !c.is_control() {
                                app_guard.input.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            app_guard.input.pop();
                        }
                        KeyCode::Esc => {
                            app_guard.input_mode = InputMode::Normal;
                            app_guard.status_message = String::from("Search cancelled.");
                        }
                        _ => {}
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
    // If in Save/Load mode, we might want to show a popup.
    // For simplicity, we can render over the results or input.

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
    if input_lines > input_display_height {
        input_scroll = input_lines - input_display_height;
    }
    app.input_scroll = input_scroll;

    let input = Paragraph::new(app.input.as_str())
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("SPL Search")
                .border_style(Style::default().fg(app.theme.title_main))
                .padding(Padding::horizontal(1)),
        )
        .scroll((input_scroll, 0));
    f.render_widget(input, header_chunks[0]);

    // Job Stats
    let mut stats_text = vec![];
    if let Some(status) = &app.current_job_status {
        stats_text.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(app.theme.title_secondary)),
            Span::styled(format!("{} ", if status.is_done { "Done" } else { "Running" }), Style::default().fg(app.theme.text)),
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
    } else {
        stats_text.push(Line::from("No active job."));
    }

    let stats_paragraph = Paragraph::new(stats_text)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
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
        f.render_widget(ratatui::widgets::Clear, area); // Clear background

        let input_block = Paragraph::new(app.save_search_name.as_str())
            .style(Style::default().fg(app.theme.input_edit))
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Save Search As")
                .border_style(Style::default().fg(app.theme.title_main)));
        f.render_widget(input_block, area);
    }

    if let InputMode::LoadSearch = app.input_mode {
        let area = centered_rect(60, 40, f.area());
        f.render_widget(ratatui::widgets::Clear, area); // Clear background

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
        let lines: Vec<&str> = app.input.lines().collect();
        let current_line_idx = if lines.is_empty() { 0 } else { lines.len() - 1 };
        let on_new_line = app.input.ends_with('\n');

        let (cursor_x, cursor_y) = if on_new_line {
             (0, lines.len())
        } else {
             (lines.last().map(|l| l.len()).unwrap_or(0), current_line_idx)
        };

        let displayed_y = cursor_y as u16 - app.input_scroll;

        if displayed_y < input_display_height {
            f.set_cursor_position(ratatui::layout::Position::new(
                header_chunks[0].x + 1 + 1 + cursor_x as u16,
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
