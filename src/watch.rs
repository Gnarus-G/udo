use anyhow::Result;
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::progress;

pub fn run(task_id: Option<String>) -> Result<()> {
    let terminal = ratatui::init();
    let result = run_app(terminal, task_id);
    ratatui::restore();
    result
}

struct App {
    selected: Option<String>,
    task_ids: Vec<String>,
    list_state: ListState,
    focused_task_id: Option<String>,
    refresh_counter: usize,
}

impl App {
    fn new(focused: Option<String>) -> Self {
        let mut app = Self {
            selected: None,
            task_ids: Vec::new(),
            list_state: ListState::default(),
            focused_task_id: focused,
            refresh_counter: 0,
        };
        app.refresh();
        if !app.task_ids.is_empty() && app.selected.is_none() {
            app.list_state.select(Some(0));
            app.selected = app.task_ids.first().cloned();
        }
        if let Some(ref tid) = app.focused_task_id {
            app.selected = Some(tid.clone());
            if let Some(idx) = app.task_ids.iter().position(|t| t == tid) {
                app.list_state.select(Some(idx));
            }
        }
        app
    }

    fn refresh(&mut self) {
        if let Ok(ids) = progress::list_task_ids() {
            let mut ids = ids;
            ids.reverse();
            self.task_ids = ids;
        }
    }

    fn select_up(&mut self) {
        if self.task_ids.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        if current > 0 {
            self.list_state.select(Some(current - 1));
            self.selected = self.task_ids.get(current - 1).cloned();
        }
    }

    fn select_down(&mut self) {
        if self.task_ids.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        if current < self.task_ids.len() - 1 {
            self.list_state.select(Some(current + 1));
            self.selected = self.task_ids.get(current + 1).cloned();
        }
    }
}

fn run_app(mut terminal: ratatui::DefaultTerminal, task_id: Option<String>) -> Result<()> {
    let mut app = App::new(task_id);
    let tick_rate = std::time::Duration::from_millis(250);

    loop {
        terminal.draw(|f| render(f, &app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Up | KeyCode::Char('k') => app.select_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.select_down(),
                        _ => {}
                    }
                }
            }
        }

        app.refresh_counter += 1;
        if app.refresh_counter % 4 == 0 {
            app.refresh();
        }
    }
    Ok(())
}

fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    if app.task_ids.is_empty() {
        let msg = if app.focused_task_id.is_some() {
            format!("no task found with id {}", app.focused_task_id.as_deref().unwrap_or("?"))
        } else {
            "no tasks yet".to_string()
        };
        let widget = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::bordered().title(" udo "));
        f.render_widget(widget, area);
        return;
    }

    let [left, right] = Layout::horizontal([Constraint::Length(20), Constraint::Fill(1)]).areas(area);
    let [list_area, detail_area] = Layout::vertical([Constraint::Fill(1), Constraint::Length(8)]).areas(right);

    render_list(f, left, app);
    render_detail(f, list_area, app);
    render_events(f, detail_area, app);
}

fn render_list(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .task_ids
        .iter()
        .map(|id| {
            let snap = progress::snapshot(id);
            let (state_char, color) = match snap.as_ref().map(|s| &s.state) {
                Some(progress::TaskState::Running) => ("▶", Color::Cyan),
                Some(progress::TaskState::Completed) => ("✓", Color::Green),
                Some(progress::TaskState::Failed) => ("✗", Color::Red),
                None => ("?", Color::DarkGray),
            };
            let line = Line::from(vec![
                Span::styled(format!("{state_char} "), Style::default().fg(color)),
                Span::raw(id.clone()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(
        Block::bordered()
            .title(" tasks ")
            .style(Style::default()),
    );
    f.render_stateful_widget(list, area, &mut app.list_state.clone());
}

fn render_detail(f: &mut Frame, area: Rect, app: &App) {
    let selected_id = match &app.selected {
        Some(id) => id.clone(),
        None => {
            let widget = Paragraph::new("select a task").style(Style::default().fg(Color::DarkGray));
            f.render_widget(widget, area);
            return;
        }
    };

    let snap = match progress::snapshot(&selected_id) {
        Some(s) => s,
        None => {
            let widget = Paragraph::new("no data").style(Style::default().fg(Color::DarkGray));
            f.render_widget(widget, area);
            return;
        }
    };

    let state_style = match &snap.state {
        progress::TaskState::Running => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        progress::TaskState::Completed => Style::default().fg(Color::Green),
        progress::TaskState::Failed => Style::default().fg(Color::Red),
    };

    let age = (Utc::now() - snap.updated_at).num_seconds();
    let age_str = if age < 60 {
        format!("{age}s ago")
    } else if age < 3600 {
        format!("{}m ago", age / 60)
    } else {
        format!("{}h ago", age / 3600)
    };

    let state_str = format!("{:?}", snap.state).to_lowercase();
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{state_str}"), state_style),
            Span::raw(format!("  iter {}/25", snap.iter)),
            Span::raw(format!("  model {}", snap.model)),
        ]),
        Line::from(vec![
            Span::styled("prompt: ", Style::default().fg(Color::DarkGray)),
            Span::raw(truncate_str(&snap.prompt, 120)),
        ]),
        Line::from(vec![
            Span::styled("updated: ", Style::default().fg(Color::DarkGray)),
            Span::raw(age_str),
        ]),
    ];

    let mut extra = vec![];
    if let Some(ref cmd) = snap.current_command {
        extra.push(Line::from(vec![
            Span::styled("running: ", Style::default().fg(Color::Yellow)),
            Span::raw(truncate_str(cmd, 120)),
        ]));
    }
    if let Some(ref summary) = snap.summary {
        extra.push(Line::from(vec![
            Span::styled("result: ", Style::default().fg(Color::Green)),
            Span::raw(truncate_str(summary, 120)),
        ]));
    }
    if let Some(ref err) = snap.error {
        extra.push(Line::from(vec![
            Span::styled("error: ", Style::default().fg(Color::Red)),
            Span::raw(truncate_str(err, 120)),
        ]));
    }

    let all_lines: Vec<Line> = lines.into_iter().chain(extra).collect();
    let widget = Paragraph::new(all_lines)
        .block(Block::bordered().title(format!(" {selected_id} ")))
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

fn render_events(f: &mut Frame, area: Rect, app: &App) {
    let selected_id = match &app.selected {
        Some(id) => id.as_str(),
        None => {
            f.render_widget(
                Paragraph::new("").block(Block::bordered().title(" events ")),
                area,
            );
            return;
        }
    };

    let events = progress::read_events(selected_id).unwrap_or_default();
    let now = Utc::now();
    let items: Vec<ListItem> = events
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .map(|te| {
            let age = (now - te.ts).num_seconds();
            let age_str = if age < 60 {
                format!("{age}s")
            } else if age < 3600 {
                format!("{}m", age / 60)
            } else {
                format!("{}h", age / 3600)
            };
            let desc = match &te.event {
                progress::Event::Started { model, .. } => format!("started (model: {model})"),
                progress::Event::IterationStarted { iter } => format!("iteration {iter}"),
                progress::Event::ModelRequestStarted { .. } => "model request".into(),
                progress::Event::ModelResponseReceived { .. } => "model response".into(),
                progress::Event::ToolStarted { command, .. } => format!("bash: {}", truncate_str(command, 80)),
                progress::Event::ToolFinished {
                    command, exit_code, ..
                } => {
                    let ec: String = if *exit_code == 0 { "ok".into() } else { format!("exit({exit_code})") };
                    format!("{ec} {}", truncate_str(command, 70))
                }
                progress::Event::Completed { summary } => format!("done: {}", truncate_str(summary, 80)),
                progress::Event::Failed { error } => format!("failed: {}", truncate_str(error, 80)),
            };
            let (marker, color) = match &te.event {
                progress::Event::Started { .. } => ("▶", Color::Cyan),
                progress::Event::IterationStarted { .. } => ("↻", Color::DarkGray),
                progress::Event::ModelRequestStarted { .. } => ("↑", Color::Blue),
                progress::Event::ModelResponseReceived { .. } => ("↓", Color::Blue),
                progress::Event::ToolStarted { .. } => ("$", Color::Yellow),
                progress::Event::ToolFinished { exit_code, .. } if *exit_code != 0 => ("✗", Color::Red),
                progress::Event::ToolFinished { .. } => ("✓", Color::Green),
                progress::Event::Completed { .. } => ("✓", Color::Green),
                progress::Event::Failed { .. } => ("✗", Color::Red),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{age_str:>4} "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{marker} "), Style::default().fg(color)),
                Span::raw(desc),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::bordered().title(" events "));
    f.render_widget(list, area);
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}