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
use tokio::sync::Mutex;
use crate::api::SplunkClient;
use crate::models::splunk::JobStatus;
use serde_json::Value;

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
        }
    }

    async fn perform_search(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }

        self.status_message = format!("Creating search job for '{}'...", self.input);
        self.current_job_sid = None;
        self.current_job_status = None;
        self.search_results.clear();
        self.results_fetched = false;
        self.scroll_offset = 0;

        match self.client.create_search(&self.input).await {
            Ok(sid) => {
                self.current_job_sid = Some(sid.clone());
                self.status_message = format!("Job created (SID: {}). Waiting for results...", sid);
            }
            Err(e) => {
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
                         self.status_message = String::from("Job done. Fetching results...");
                         match self.client.get_results(sid, 100, 0).await {
                             Ok(results) => {
                                 self.search_results = results;
                                 self.results_fetched = true;
                                 self.status_message = format!("Loaded {} results.", self.search_results.len());
                             }
                             Err(e) => {
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
                Err(_) => {
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

async fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: Arc<Mutex<App>>) -> io::Result<()> {
    let tick_rate = std::time::Duration::from_millis(250);
    let mut last_tick = std::time::Instant::now();

    loop {
        let mut app_guard = app.lock().await;
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
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            drop(app_guard);
                            let mut app_guard_search = app.lock().await;
                            app_guard_search.perform_search().await;
                            app_guard_search.input_mode = InputMode::Normal;
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
    let footer_text = vec![
        Line::from(vec![
            Span::styled(" e ", Style::default().fg(app.theme.title_main)),
            Span::styled("Search  |  ", Style::default().fg(app.theme.text)),
            Span::styled(" x ", Style::default().fg(app.theme.title_main)),
            Span::styled("Kill Job  |  ", Style::default().fg(app.theme.text)),
            Span::styled(" ↑/↓ ", Style::default().fg(app.theme.title_main)),
            Span::styled("Scroll  |  ", Style::default().fg(app.theme.text)),
            Span::styled(" t ", Style::default().fg(app.theme.title_main)),
            Span::styled("Theme  |  ", Style::default().fg(app.theme.text)),
            Span::styled(" q ", Style::default().fg(app.theme.title_main)),
            Span::styled("Quit", Style::default().fg(app.theme.text)),
        ]),
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
