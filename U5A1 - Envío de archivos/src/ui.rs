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
    widgets::{Block, Borders, Cell as RataCell, Gauge, Paragraph, Row, Table},
    Frame,
};

use crate::game::{Cell, Game, GameResult};
use crate::network::TransferProgress;

/// Estado del panel de transferencia de memes que se pasa desde main.rs
pub struct TransferState {
    /// true cuando el campo de ruta está activo (el usuario está escribiendo)
    pub input_active: bool,
    /// Ruta que el usuario está escribiendo
    pub input_path: String,
    /// Progreso de la transferencia en curso (None si no hay ninguna activa)
    pub progress: Option<TransferProgress>,
    /// Último mensaje a mostrar cuando no hay transferencia activa
    pub last_event: Option<String>,
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
    // ── Layout principal: zona de juego arriba / panel memes abajo ──
    let main_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // zona de juego
            Constraint::Length(5), // panel de memes
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

    // Construir las 3 filas del tablero
    let rows: Vec<Row> = (0..3)
        .map(|row| {
            let cells: Vec<RataCell> = (0..3)
                .map(|col| {
                    let idx = row * 3 + col;
                    let cell_content = &game.board[idx];

                    // Color base de la celda
                    let base_style = match cell_content {
                        Cell::X => Style::default().fg(my_color_for(Cell::X, my_color, rival_color)),
                        Cell::O => Style::default().fg(my_color_for(Cell::O, my_color, rival_color)),
                        Cell::Empty => Style::default().fg(COLOR_DIM),
                    };

                    // Resaltar celdas ganadoras
                    let style = if winner_cells.contains(&idx) {
                        base_style.fg(COLOR_WIN).add_modifier(Modifier::BOLD)
                    } else {
                        base_style.add_modifier(Modifier::BOLD)
                    };

                    // Mostrar número de casilla si está vacía, símbolo si está ocupada
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

    // Anchos iguales para las 3 columnas
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
    let help = Paragraph::new("  1-9: elegir casilla   |   R: reiniciar   |   Q: salir")
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

    // ── Campo de input activo: el usuario está escribiendo la ruta ──
    if transfer.input_active {
        let path_display = format!("> {}_", transfer.input_path);
        let lines = vec![
            Line::from(Span::styled(
                "Escribe la ruta del archivo y presiona Enter (Esc para cancelar):",
                Style::default().fg(COLOR_DIM),
            )),
            Line::from(Span::styled(
                path_display,
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
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
        "[M] Enviar meme — presiona M para activar el campo de ruta".to_string()
    };

    frame.render_widget(
        Paragraph::new(msg).style(Style::default().fg(COLOR_DIM)),
        inner,
    );
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
    // X siempre es J1 (COLOR_X), O siempre es J2 (COLOR_O)
    // pero desde la perspectiva de cada peer, coloreamos
    // "mi color" vs "color rival"
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
