use anyhow::Result;
use crossterm::event::{self, Event};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashSet;
use std::io::stdout;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::UnboundedSender;

pub mod inputs;
pub mod prompts;
pub mod theme;
pub mod viewport;

use crate::github::{RepoLocator, RepoTree};

/// High-level actions emitted by the UI layer and handled by the application controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Up,
    Down,
    Left,
    Right,
    Select,
    ToggleMark,
    Help,
    Quit,
}

/// Messages sent from the UI loop to the application controller.
#[derive(Debug, Clone)]
pub enum AppMessage {
    CopyRequest {
        #[allow(dead_code)]
        path: String,
    },
}

struct AppState {
    locator: RepoLocator,
    tree: RepoTree,
    dir_path: String, // current directory path (empty for root)
    items: Vec<crate::github::RepoNode>,
    viewport: viewport::Viewport,
    breadcrumb: String,
    marked: HashSet<usize>,
    show_help: bool,
    loading: bool,
    last_tick: Instant,
    error: Option<String>,
    show_hidden: bool,
    tx: UnboundedSender<AppMessage>,
}

impl AppState {
    fn new(repo: &RepoLocator, show_hidden: bool, tx: UnboundedSender<AppMessage>) -> Self {
        let tree = RepoTree::new();
        let items = Vec::new();
        Self {
            locator: repo.clone(),
            tree,
            dir_path: String::new(),
            items,
            viewport: viewport::Viewport::new(),
            breadcrumb: format!("{}/{}", repo.owner, repo.repo),
            marked: HashSet::new(),
            show_help: false,
            loading: false,
            last_tick: Instant::now(),
            error: None,
            show_hidden,
            tx,
        }
    }
}

/// Launch the interactive browser UI. This is a blocking call that returns when the user exits.
/// For now it is a stub that renders a placeholder screen and quits on `q` / Ctrl-C.
pub async fn run(
    _repo: &RepoLocator,
    tx: UnboundedSender<AppMessage>,
    show_hidden: bool,
) -> Result<()> {
    // 1. Enter alternate screen + raw mode
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    // 2. Setup ratatui terminal
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 3. Event loop
    let mut app = AppState::new(_repo, show_hidden, tx);
    let res = run_app(&mut terminal, &mut app).await;

    // 4. Restore terminal state no matter what
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut AppState,
) -> Result<()> {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};

    loop {
        // Ensure children loaded for current dir
        if app.items.is_empty() {
            app.loading = true;
            match app.tree.children(&app.locator, &app.dir_path, false).await {
                Ok(children) => {
                    app.items = children
                        .iter()
                        .filter(|n| app.show_hidden || !n.name.starts_with('.'))
                        .cloned()
                        .collect()
                }
                Err(e) => {
                    app.error = Some(format!("Fetch error: {e}"));
                }
            }
            app.loading = false;
        }

        // 1. Draw UI
        terminal.draw(|f| {
            let size = f.area();

            // Split layout: breadcrumb (1 line), main list (rest -1), footer (1 line)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(size);

            // Breadcrumb bar
            let bc = Paragraph::new(Line::from(vec![Span::styled(
                app.breadcrumb.clone(),
                Style::default().fg(theme::Palette::BREADCRUMB),
            )]));
            f.render_widget(bc, chunks[0]);

            // Determine visible items based on viewport
            let list_height = chunks[1].height as usize;
            // Ensure selected index visible
            app.viewport.ensure_visible(list_height);

            let start = app.viewport.scroll_offset;
            let end = usize::min(start + list_height, app.items.len());

            let mut styled_lines: Vec<Line> = Vec::with_capacity(end - start);
            for (idx, node) in app.items[start..end].iter().enumerate() {
                let absolute_idx = start + idx;
                if absolute_idx == app.viewport.selected_index {
                    styled_lines.push(Line::from(Span::styled(
                        format!("{} {}{}", icon_for(node), node.name, bubble(node)),
                        Style::default()
                            .fg(theme::Palette::SELECTED_FG)
                            .bg(theme::Palette::SELECTED_BG)
                            .add_modifier(Modifier::BOLD),
                    )));
                } else {
                    styled_lines.push(Line::from(Span::styled(
                        format!("{} {}{}", icon_for(node), node.name, bubble(node)),
                        Style::default().fg(fg_color(node)),
                    )));
                }
            }

            let list_widget =
                Paragraph::new(styled_lines).block(Block::default().borders(Borders::NONE));
            f.render_widget(list_widget, chunks[1]);

            // Footer hints
            let footer_text = "↑/↓ move → enter ← back q quit ? help";
            let footer =
                Paragraph::new(footer_text).style(Style::default().fg(theme::Palette::FOOTER));
            f.render_widget(footer, chunks[2]);

            // Help modal overlay
            if app.show_help {
                let help_text = "Controls:\n\n↑/k down  ↓/j up\n→/l/Enter expand/select\n←/h back\nSpace mark for copy\nq quit  ? help";
                let area = centered_rect(60, 40, size);
                let block = Block::default()
                    .title("Help")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::Palette::BREADCRUMB));
                let help = Paragraph::new(help_text).block(block);
                f.render_widget(help, area);
            }

            // Loading spinner overlay
            if app.loading {
                let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let idx = ((Instant::now().duration_since(app.last_tick).as_millis() / 100) % 10) as usize;
                let area = centered_rect(10, 10, size);
                let spinner = Paragraph::new(spinner_frames[idx]).block(Block::default().borders(Borders::ALL));
                f.render_widget(spinner, area);
            }

            // Error banner
            if let Some(err) = &app.error {
                let banner = Paragraph::new(err.as_str())
                    .style(Style::default().fg(ratatui::style::Color::Red).bg(ratatui::style::Color::Black));
                let area = ratatui::layout::Rect::new(0, size.height.saturating_sub(2), size.width, 1);
                f.render_widget(banner, area);
            }
        })?;

        // 2. Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = inputs::key_event_to_action(&key) {
                    match action {
                        AppAction::Quit => break,
                        AppAction::Up => app.viewport.up(),
                        AppAction::Down => app.viewport.down(app.items.len()),
                        AppAction::Right | AppAction::Select => {
                            if let Some(node) = app.items.get(app.viewport.selected_index) {
                                if node.is_dir() {
                                    // Enter directory
                                    app.dir_path = if app.dir_path.is_empty() {
                                        node.path.clone()
                                    } else {
                                        format!("{}/{}", app.dir_path, node.name)
                                    };
                                    app.viewport = viewport::Viewport::new();
                                    app.items.clear();
                                } else {
                                    // file or manifest selection triggers copy request event
                                    let _ = app.tx.send(AppMessage::CopyRequest {
                                        path: node.path.clone(),
                                    });
                                }
                            }
                        }
                        AppAction::Left => {
                            if !app.dir_path.is_empty() {
                                if let Some(pos) = app.dir_path.rfind('/') {
                                    app.dir_path.truncate(pos);
                                } else {
                                    app.dir_path.clear();
                                }
                                app.viewport = viewport::Viewport::new();
                                app.items.clear();
                            }
                        }
                        AppAction::ToggleMark => {
                            let idx = app.viewport.selected_index;
                            if !app.marked.insert(idx) {
                                app.marked.remove(&idx);
                            }
                        }
                        AppAction::Help => app.show_help = !app.show_help,
                    }
                }
            }
        }
        app.last_tick = Instant::now();
    }

    Ok(())
}

/// Helper to create a centered rect with given percentage width/height
fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
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

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

fn icon_for(node: &crate::github::RepoNode) -> char {
    use crate::github::NodeKind::*;
    match node.kind {
        Dir => '📁',
        RuleFile => '📄',
        Manifest => '📦',
    }
}

fn fg_color(node: &crate::github::RepoNode) -> ratatui::style::Color {
    if node.name.starts_with('.') {
        // hidden entry
        theme::Palette::HIDDEN
    } else {
        theme::Palette::NORMAL
    }
}

fn bubble(node: &crate::github::RepoNode) -> String {
    if let Some(count) = node.manifest_count {
        format!("  [{count} files]")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{NodeKind, RepoNode};

    #[test]
    fn icon_and_color() {
        let file = RepoNode {
            name: "file.mdc".into(),
            path: "file.mdc".into(),
            kind: NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        };
        let dir = RepoNode {
            name: ".hidden".into(),
            path: ".hidden".into(),
            kind: NodeKind::Dir,
            children: None,
            manifest_count: None,
        };
        assert_eq!(icon_for(&file), '📄');
        assert_eq!(icon_for(&dir), '📁');
        assert_eq!(fg_color(&dir), theme::Palette::HIDDEN);
    }
}
