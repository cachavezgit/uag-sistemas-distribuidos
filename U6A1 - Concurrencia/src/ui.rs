// ─────────────────────────────────────────────────────────
// ui.rs — Interfaz visual con Ratatui
//
// Renderiza el tablero del juego del gato, el estado de la
// partida y las instrucciones en la terminal.
// ─────────────────────────────────────────────────────────

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell as RataCell, Clear, Gauge, Paragraph, Row, Table},
    Frame,
};
use ratatui_explorer::FileExplorer;

use crate::game::{Cell, Game, GameResult};
use crate::network::TransferProgress;

/// Estado del panel de transferencia de memes que se pasa desde main.rs
pub struct TransferState {
    /// Explorador de archivos modal (Some = abierto, None = cerrado)
    pub explorer: Option<FileExplorer>,
    /// Progreso de la transferencia en curso (None si no hay ninguna activa)
    pub progress: Option<TransferProgress>,
    /// Último mensaje a mostrar cuando no hay transferencia activa
    pub last_event: Option<String>,
    /// Estado del panel de streaming de video
    pub video_state: VideoState,
}

/// Estado del panel de streaming de video (U6 A1)
#[derive(Debug, Clone, PartialEq)]
pub enum VideoState {
    Inactivo,
    Explorando,
    Transmitiendo { chunk_actual: usize, total: usize },
    Reconstruyendo,
    Reproduciendo,
    Error(String),
}

impl Default for VideoState {
    fn default() -> Self {
        VideoState::Inactivo
    }
}

/// Colores del juego
const COLOR_X: Color = Color::Cyan;
const COLOR_O: Color = Color::Yellow;
const COLOR_WIN: Color = Color::Green;
const COLOR_LOSE: Color = Color::Red;
const COLOR_DRAW: Color = Color::Magenta;
const COLOR_TITLE: Color = Color::White;
const COLOR_DIM: Color = Color::DarkGray;

/// Renderiza toda la interfaz en un frame de Ratatui
pub fn render(frame: &mut Frame, game: &Game, transfer: &TransferState) {
    // ── Layout principal: zona de juego arriba / paneles memes y video abajo ──
    let main_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // zona de juego
            Constraint::Length(5), // panel de memes
            Constraint::Length(5), // panel de video
        ])
        .split(frame.area());

    // ── Layout de juego: columna izquierda / columna derecha ──
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(main_rows[0]);

    // ── Columna izquierda: título / tablero / estado / controles ──
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // título
            Constraint::Length(11), // tablero 3x3
            Constraint::Length(3),  // estado
            Constraint::Length(3),  // controles
        ])
        .split(columns[0]);

    render_title(frame, areas[0], game);
    render_board(frame, areas[1], game);
    render_status(frame, areas[2], game);
    render_help(frame, areas[3]);

    // ── Columna derecha: panel Historia ──
    render_history_panel(frame, columns[1], game);

    // ── Panel inferior: transferencia de memes ──
    render_memes_panel(frame, main_rows[1], transfer);

    // ── Panel inferior: streaming de video ──
    render_video_panel(frame, main_rows[2], transfer);

    // ── Overlay del explorador de archivos (encima de todo) ──
    if let Some(explorer) = &transfer.explorer {
        let area = centered_rect(70, 75, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget_ref(explorer.widget(), area);
    }
}

// ─────────────────────────────────────────────────────────
// Título y símbolo del jugador local
// ─────────────────────────────────────────────────────────
fn render_title(frame: &mut Frame, area: Rect, game: &Game) {
    let (my_color, rival_color) = player_colors(game);

    let title = Line::from(vec![
        Span::styled("JUEGO DEL GATO  ", Style::default().fg(COLOR_TITLE).add_modifier(Modifier::BOLD)),
        Span::styled("@", Style::default().fg(COLOR_DIM)),
        Span::styled(game.usuario.as_str(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled("  Tú: ", Style::default().fg(COLOR_DIM)),
        Span::styled(game.my_symbol(), Style::default().fg(my_color).add_modifier(Modifier::BOLD)),
        Span::styled("  Rival: ", Style::default().fg(COLOR_DIM)),
        Span::styled(game.rival_symbol(), Style::default().fg(rival_color).add_modifier(Modifier::BOLD)),
        Span::styled("  P2P — RPC", Style::default().fg(COLOR_DIM)),
    ]);

    let block = Paragraph::new(title)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    frame.render_widget(block, area);
}

// ─────────────────────────────────────────────────────────
// Tablero 3x3
// ─────────────────────────────────────────────────────────
fn render_board(frame: &mut Frame, area: Rect, game: &Game) {
    let (my_color, rival_color) = player_colors(game);
    let winner_cells = winning_cells(game);

    let rows: Vec<Row> = (0..3)
        .map(|row| {
            let cells: Vec<RataCell> = (0..3)
                .map(|col| {
                    let idx = row * 3 + col;
                    let cell_content = &game.board[idx];

                    let base_style = match cell_content {
                        Cell::X => Style::default().fg(my_color_for(Cell::X, my_color, rival_color)),
                        Cell::O => Style::default().fg(my_color_for(Cell::O, my_color, rival_color)),
                        Cell::Empty => Style::default().fg(COLOR_DIM),
                    };

                    let style = if winner_cells.contains(&idx) {
                        base_style.fg(COLOR_WIN).add_modifier(Modifier::BOLD)
                    } else {
                        base_style.add_modifier(Modifier::BOLD)
                    };

                    let label = match cell_content {
                        Cell::Empty => format!("  {}  ", idx + 1),
                        Cell::X => "  X  ".to_string(),
                        Cell::O => "  O  ".to_string(),
                    };

                    RataCell::from(label).style(style)
                })
                .collect();

            Row::new(cells).height(3)
        })
        .collect();

    let widths = [
        Constraint::Percentage(33),
        Constraint::Percentage(33),
        Constraint::Percentage(34),
    ];

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Tablero ")
                .title_alignment(Alignment::Center),
        )
        .column_spacing(1);

    frame.render_widget(table, area);
}

// ─────────────────────────────────────────────────────────
// Panel de estado del juego
// ─────────────────────────────────────────────────────────
fn render_status(frame: &mut Frame, area: Rect, game: &Game) {
    let (text, color) = match &game.result {
        GameResult::Ongoing => {
            let msg = game.status_message();
            let color = if game.is_my_turn() { Color::Green } else { Color::Yellow };
            (msg, color)
        }
        GameResult::Win(winner) => {
            if *winner == game.my_player {
                ("¡Ganaste! — Presiona R para jugar de nuevo o Q para salir".to_string(), COLOR_WIN)
            } else {
                ("Perdiste. El rival ganó — Presiona R para jugar de nuevo o Q para salir".to_string(), COLOR_LOSE)
            }
        }
        GameResult::Draw => (
            "¡Empate! — Presiona R para jugar de nuevo o Q para salir".to_string(),
            COLOR_DRAW,
        ),
    };

    let status = Paragraph::new(text)
        .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title(" Estado "))
        .alignment(Alignment::Center);

    frame.render_widget(status, area);
}

// ─────────────────────────────────────────────────────────
// Panel de controles
// ─────────────────────────────────────────────────────────
fn render_help(frame: &mut Frame, area: Rect) {
    let help = Paragraph::new("  1-9: elegir casilla   |   R: reiniciar   |   M: enviar meme   |   Q: salir")
        .style(Style::default().fg(COLOR_DIM))
        .block(Block::default().borders(Borders::ALL).title(" Controles "))
        .alignment(Alignment::Center);

    frame.render_widget(help, area);
}

// ─────────────────────────────────────────────────────────
// Panel lateral de historial — siempre visible
// ─────────────────────────────────────────────────────────
fn render_history_panel(frame: &mut Frame, area: Rect, game: &Game) {
    let (my_color, rival_color) = player_colors(game);

    let mut lines: Vec<Line> = Vec::new();

    if game.move_history.is_empty() {
        lines.push(Line::from(Span::styled(
            " Sin movidas aún...",
            Style::default().fg(COLOR_DIM),
        )));
    } else {
        for (i, &(player, cell)) in game.move_history.iter().enumerate() {
            let symbol = if player == 1 { "X" } else { "O" };
            let color = if player == 1 {
                my_color_for(Cell::X, my_color, rival_color)
            } else {
                my_color_for(Cell::O, my_color, rival_color)
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:>2}. ", i + 1),
                    Style::default().fg(COLOR_DIM),
                ),
                Span::styled(
                    format!("J{} ({})", player, symbol),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  →  casilla {}", cell + 1),
                    Style::default().fg(COLOR_TITLE),
                ),
            ]));
        }
    }

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Historia ")
                .title_alignment(Alignment::Center),
        )
        .alignment(Alignment::Left);

    frame.render_widget(panel, area);
}

// ─────────────────────────────────────────────────────────
// Panel inferior de transferencia de memes
// ─────────────────────────────────────────────────────────
fn render_memes_panel(frame: &mut Frame, area: Rect, transfer: &TransferState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" MEMES — Transferencia P2P ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Magenta));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // ── Explorador abierto ──
    if transfer.explorer.is_some() {
        frame.render_widget(
            Paragraph::new("Selecciona un archivo en el explorador (Enter = enviar, Esc = cancelar)")
                .style(Style::default().fg(COLOR_DIM)),
            inner,
        );
        return;
    }

    // ── Transferencia en curso: barra de progreso ──
    if let Some(TransferProgress::Sending { current, total, file_name }) = &transfer.progress {
        let ratio = if *total > 0 { *current as f64 / *total as f64 } else { 0.0 };
        let label = format!("Enviando: {}  {}/{} chunks", file_name, current, total);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);

        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(Color::Cyan)),
            layout[0],
        );
        frame.render_widget(
            Gauge::default()
                .ratio(ratio)
                .style(Style::default().fg(Color::Cyan))
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray)),
            layout[1],
        );
        return;
    }

    // ── Estado inactivo: último evento o ayuda ──
    let msg = if let Some(ev) = &transfer.last_event {
        ev.clone()
    } else {
        "[M] Enviar meme — abre el explorador de archivos".to_string()
    };

    frame.render_widget(
        Paragraph::new(msg).style(Style::default().fg(COLOR_DIM)),
        inner,
    );
}

// ─────────────────────────────────────────────────────────
// Panel inferior de streaming de video
// ─────────────────────────────────────────────────────────
fn render_video_panel(frame: &mut Frame, area: Rect, transfer: &TransferState) {
    let border_color = match &transfer.video_state {
        VideoState::Error(_) => Color::Red,
        VideoState::Reproduciendo => Color::Green,
        _ => Color::Blue,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" VIDEO — Streaming P2P ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    match &transfer.video_state {
        VideoState::Transmitiendo { chunk_actual, total } => {
            let ratio = if *total > 0 { *chunk_actual as f64 / *total as f64 } else { 0.0 };
            let label = format!("Enviando video...  {}/{} chunks", chunk_actual, total);

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1)])
                .split(inner);

            frame.render_widget(
                Paragraph::new(label).style(Style::default().fg(Color::Cyan)),
                layout[0],
            );
            frame.render_widget(
                Gauge::default()
                    .ratio(ratio)
                    .style(Style::default().fg(Color::Cyan))
                    .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray)),
                layout[1],
            );
        }
        VideoState::Inactivo => {
            frame.render_widget(
                Paragraph::new("[V] Enviar video — abre el explorador de archivos")
                    .style(Style::default().fg(COLOR_DIM)),
                inner,
            );
        }
        VideoState::Explorando => {
            frame.render_widget(
                Paragraph::new("Selecciona un video en el explorador (Enter = enviar, Esc = cancelar)")
                    .style(Style::default().fg(COLOR_DIM)),
                inner,
            );
        }
        VideoState::Reconstruyendo => {
            frame.render_widget(
                Paragraph::new("Reconstruyendo video recibido...")
                    .style(Style::default().fg(Color::Yellow)),
                inner,
            );
        }
        VideoState::Reproduciendo => {
            frame.render_widget(
                Paragraph::new("▶ Reproduciendo — [Q] para detener")
                    .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                inner,
            );
        }
        VideoState::Error(msg) => {
            frame.render_widget(
                Paragraph::new(format!("✗ Error: {}", msg)).style(Style::default().fg(Color::Red)),
                inner,
            );
        }
    }
}

// ─────────────────────────────────────────────────────────
// Utilidades
// ─────────────────────────────────────────────────────────

/// Retorna los colores (mi color, color rival) según quién es J1/J2
fn player_colors(game: &Game) -> (Color, Color) {
    if game.my_player == 1 {
        (COLOR_X, COLOR_O)
    } else {
        (COLOR_O, COLOR_X)
    }
}

/// Retorna el color correspondiente a una celda X u O
fn my_color_for(cell: Cell, my_color: Color, rival_color: Color) -> Color {
    match cell {
        Cell::X => my_color,
        Cell::O => rival_color,
        Cell::Empty => COLOR_DIM,
    }
}

/// Retorna los índices de las celdas ganadoras para resaltarlas
fn winning_cells(game: &Game) -> Vec<usize> {
    const WINNING_LINES: [[usize; 3]; 8] = [
        [0, 1, 2], [3, 4, 5], [6, 7, 8],
        [0, 3, 6], [1, 4, 7], [2, 5, 8],
        [0, 4, 8], [2, 4, 6],
    ];

    for line in &WINNING_LINES {
        let [a, b, c] = *line;
        if game.board[a] != Cell::Empty
            && game.board[a] == game.board[b]
            && game.board[b] == game.board[c]
        {
            return vec![a, b, c];
        }
    }
    vec![]
}

/// Retorna un Rect centrado con los porcentajes dados respecto al área padre
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let margin_v = (100 - percent_y) / 2;
    let margin_h = (100 - percent_x) / 2;

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(margin_v),
            Constraint::Percentage(percent_y),
            Constraint::Percentage(margin_v),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(margin_h),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(margin_h),
        ])
        .split(vert[1])[1]
}
