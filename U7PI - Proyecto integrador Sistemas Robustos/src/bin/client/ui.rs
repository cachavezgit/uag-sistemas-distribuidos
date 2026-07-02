// ─────────────────────────────────────────────────────────
// client/ui.rs — Layout WhatsApp con Ratatui
// ─────────────────────────────────────────────────────────

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::app::{AppState, Focus};

const COLOR_HEADER_BG: Color = Color::Rgb(31, 56, 100);
const COLOR_SELECTED: Color = Color::Blue;
const COLOR_ONLINE: Color = Color::Green;
const COLOR_OWN_MSG_BG: Color = Color::Rgb(220, 248, 198);
const COLOR_OTHER_MSG_BG: Color = Color::Rgb(235, 235, 235);
const COLOR_DIM: Color = Color::DarkGray;

pub fn render(frame: &mut Frame, app: &AppState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // cuerpo
            Constraint::Length(3), // input
        ])
        .split(frame.area());

    render_header(frame, rows[0], app);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(0)])
        .split(rows[1]);

    render_contacts(frame, body[0], app);
    render_chat(frame, body[1], app);
    render_input(frame, rows[2], app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(24)])
        .split(area);

    let left = Paragraph::new(Line::from(vec![Span::styled(
        format!("{} {}", app.my_info.emoji, app.my_info.username),
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )]))
    .style(Style::default().bg(COLOR_HEADER_BG))
    .block(Block::default().borders(Borders::ALL));

    let right = Paragraph::new(Line::from(vec![Span::styled(
        "[G] Grupo  [F4] Video",
        Style::default().fg(Color::White),
    )]))
    .style(Style::default().bg(COLOR_HEADER_BG))
    .alignment(Alignment::Right)
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(left, cols[0]);
    frame.render_widget(right, cols[1]);
}

fn render_contacts(frame: &mut Frame, area: Rect, app: &AppState) {
    let mut items: Vec<ListItem> = Vec::new();

    for node in app.directory.iter().filter(|n| n.username != app.my_info.username) {
        let selected = app.selected_contact.as_deref() == Some(node.username.as_str());
        let style = if selected {
            Style::default().bg(COLOR_SELECTED).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let line = Line::from(vec![
            Span::raw(format!("{} {}  ", node.emoji, node.username)),
            Span::styled("●", Style::default().fg(COLOR_ONLINE)),
        ]);
        items.push(ListItem::new(line).style(style));
    }

    if !app.groups.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "── GRUPOS ──",
            Style::default().fg(COLOR_DIM),
        ))));
        for group in &app.groups {
            let selected = app.selected_contact.as_deref() == Some(group.name.as_str());
            let style = if selected {
                Style::default().bg(COLOR_SELECTED).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            items.push(ListItem::new(Line::from(format!("👥 {}", group.name))).style(style));
        }
    }

    let border_style = if app.focus == Focus::Contacts {
        Style::default().fg(Color::Blue)
    } else {
        Style::default()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" CONTACTOS ")
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}

fn render_chat(frame: &mut Frame, area: Rect, app: &AppState) {
    let title = match &app.selected_contact {
        Some(name) => {
            if app.is_group(name) {
                format!(" Grupo: {} ", name)
            } else if let Some(node) = app.directory.iter().find(|n| &n.username == name) {
                format!(" Chat con: {} {} ", node.emoji, node.username)
            } else {
                format!(" Chat con: {} ", name)
            }
        }
        None => " Sin contacto seleccionado ".to_string(),
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(key) = &app.selected_contact else { return };
    let Some(messages) = app.chats.get(key) else { return };

    let lines: Vec<Line> = messages
        .iter()
        .map(|m| {
            let es_propio = m.from == app.my_info.username;
            let texto = format!(" [{}] {}: {} ", m.timestamp, m.from, m.content);
            let (bg, alignment) = if es_propio {
                (COLOR_OWN_MSG_BG, Alignment::Right)
            } else {
                (COLOR_OTHER_MSG_BG, Alignment::Left)
            };
            Line::from(Span::styled(texto, Style::default().bg(bg).fg(Color::Black)))
                .alignment(alignment)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_input(frame: &mut Frame, area: Rect, app: &AppState) {
    let border_style = if app.focus == Focus::Input {
        Style::default().fg(Color::Blue)
    } else {
        Style::default()
    };

    let texto = format!("[📎] Mensaje: {}_", app.input_buffer);

    let input = Paragraph::new(texto).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" [Enter] ✓ "),
    );

    frame.render_widget(input, area);
}
