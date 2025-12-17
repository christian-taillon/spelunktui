use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Alignment},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Padding, BorderType, Wrap},
    Frame, Terminal,
};
use std::{error::Error, io, sync::Arc};
use std::process::Command;
use std::fs::File;
use std::io::Write;
use tokio::sync::Mutex;
use crate::api::SplunkClient;
use crate::models::splunk::JobStatus;
use serde_json::Value;
use log::{info, error};

#[derive(Clone, Copy)]
pub enum ThemeVariant {
    Default,
    ColorPop,
}

#[derive(Clone)]
pub struct AppTheme {
    pub variant: ThemeVariant,
    pub border: Color,
    pub text: Color,
    pub input_edit: Color,
    pub title_main: Color,
    pub title_secondary: Color,
    pub highlight: Color,
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
            highlight: Color::Magenta,
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
            highlight: Color::Blue,
        }
    }
}

enum InputMode {
    Normal,
    Editing,
}

pub struct App {
    input: String,
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
}

impl App {
    pub fn new(client: Arc<SplunkClient>) -> App {
        App {
            input: String::new(),
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
            // Don't poll if done and fetched
            if self.results_fetched {
                return;
            }

            match self.client.get_job_status(sid).await {
                Ok(status) => {
                    let is_done = status.is_done;
                    self.current_job_status = Some(status);

                    if is_done && !self.results_fetched {
                         // Fetch results
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
                                 // Even if failed, mark as fetched so we don't loop?
                                 // Or maybe retry? Let's stop to avoid infinite loop.
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
                     // self.status_message = format!("Failed to check status: {}", e);
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

        // Create temp file
        let mut temp_dir = std::env::temp_dir();
        temp_dir.push("splunk_results.json");
        let file_path = temp_dir.to_str().unwrap().to_string();

        if let Ok(mut file) = File::create(&file_path) {
            let json_content = serde_json::to_string_pretty(&self.search_results).unwrap_or_default();
            if file.write_all(json_content.as_bytes()).is_ok() {
                // Open editor
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

                // Suspend raw mode is handled by execute! in main loop effectively if we were blocking,
                // but here we are in an async function called from event loop.
                // We need to restore terminal, run command, and enable raw mode again.
                // However, doing this deep in App struct is tricky.
                // We will try to just spawn the command.
                // NOTE: In a TUI, spawning an editor requires giving it control of stdin/stdout.
                // This usually requires temporarily leaving the TUI state.
                // For this MVP, we will try `Command::new(editor).arg(file_path).status()`.
                // But we need to signal the main loop to pause/resume or handle it here.

                // We'll set a status message that we saved it,
                // actually OPENING it properly inside this async method without breaking TUI is hard
                // without passing the terminal handle.

                // Let's rely on the fact that we can't easily suspend from here without refactoring.
                // Alternative: Just save to file and tell user.
                // BUT the requirement is "open in default OS editor".

                // We will use a flag or simple hack:
                // We can use `std::process::Command` but we need to release the terminal first.
                // Since `App` doesn't own `Terminal`, we can't.

                // Refactoring opportunity: Return an Action enum from input handling instead of modifying state directly,
                // then handle Action in `run_loop`.

                // For now, let's just save the file. Opening it in TUI is advanced.
                // WAIT, I can implement it in `run_loop` if I change how `App` communicates intentions.
                // Let's add a `pending_action` field to App.
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
            ThemeVariant::ColorPop => AppTheme::default_theme(),
        };
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

        // Handle external editor opening request
        if app_guard.should_open_editor {
            app_guard.should_open_editor = false;
            drop(app_guard); // Unlock to allow external process

            // Restore terminal
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
            terminal.show_cursor()?;

            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let mut temp_dir = std::env::temp_dir();
            temp_dir.push("splunk_results.json");
            let file_path = temp_dir.to_str().unwrap().to_string();

            let _ = Command::new(editor).arg(file_path).status();

            // Resume TUI
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
            terminal.hide_cursor()?;
            terminal.clear()?;

            // Re-acquire lock
            app_guard = app.lock().await;
        }

        terminal.draw(|f| ui(f, &mut app_guard))?;

        // Periodic updates for job status
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
                        // Navigation
                        KeyCode::Down | KeyCode::Char('j') => {
                            app_guard.scroll_down();
                        }
                        KeyCode::Up | KeyCode::Char('k') => { // Conflict with kill if we don't check modifier
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
                        KeyCode::Enter => {
                            drop(app_guard);
                            let mut app_guard_search = app.lock().await;
                            app_guard_search.perform_search().await;
                            app_guard_search.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            app_guard.input.push('\n');
                        }
                        KeyCode::Char(c) => {
                            app_guard.input.push(c);
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
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(6), // Header: Search + Job Info
                Constraint::Min(10),   // Results
                Constraint::Length(3), // Footer
            ]
            .as_ref(),
        )
        .split(f.area());

    // --- Header ---
    let header_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(chunks[0]);

    let input_style = match app.input_mode {
        InputMode::Normal => Style::default().fg(app.theme.text),
        InputMode::Editing => Style::default().fg(app.theme.input_edit),
    };

    let input = Paragraph::new(format!("> {}", app.input.as_str()))
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("SPL Search")
                .border_style(Style::default().fg(app.theme.title_main))
                .padding(Padding::horizontal(4)),
        );
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

        // Shareable URL
        if let Some(sid) = &app.current_job_sid {
            let url = app.client.get_shareable_url(sid);
            stats_text.push(Line::from(vec![
                Span::styled("URL: ", Style::default().fg(app.theme.title_secondary)),
                Span::styled(url, Style::default().fg(app.theme.highlight)),
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

            // Assuming result is a JSON object, nice print it
            if let Some(obj) = result.as_object() {
                 for (k, v) in obj {
                     if k.starts_with("_") && k != "_time" && k != "_raw" { continue; } // Skip internal fields except time and raw

                     let val_str = if let Some(s) = v.as_str() { s.to_string() } else { v.to_string() };

                     content.push(Line::from(vec![
                         Span::styled(format!("{}: ", k), Style::default().fg(app.theme.highlight)),
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

    // Set cursor
    if let InputMode::Editing = app.input_mode {
        f.set_cursor_position(ratatui::layout::Position::new(
            header_chunks[0].x + 1 + 4 + 2 + app.input.len() as u16,
            header_chunks[0].y + 1,
        ))
    }
}
