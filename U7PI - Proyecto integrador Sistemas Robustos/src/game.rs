// ─────────────────────────────────────────────────────────
// game.rs — Lógica del juego del gato (Tic-Tac-Toe)
//
// Diccionario de funciones internas que simplifica la
// extensión de paquetes y comunica cambios en la interfaz.
// ─────────────────────────────────────────────────────────

/// Resultado posible de una partida
#[derive(Debug, Clone, PartialEq)]
pub enum GameResult {
    Ongoing,       // El juego continúa
    Win(u8),       // Ganó el jugador 1 u 2
    Draw,          // Empate
}

/// Contenido de una casilla
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum Cell {
    Empty,
    X,  // Jugador 1
    O,  // Jugador 2
}

impl Cell {
    #[allow(dead_code)]
    pub fn symbol(&self) -> &str {
        match self {
            Cell::Empty => " ",
            Cell::X => "X",
            Cell::O => "O",
        }
    }
}

/// Estado completo del juego
pub struct Game {
    pub board: [Cell; 9],              // Tablero lineal, índices 0-8
    pub current_player: u8,            // 1 = X, 2 = O
    pub my_player: u8,                 // Quién soy yo en esta instancia
    pub result: GameResult,
    pub move_history: Vec<(u8, usize)>, // (jugador, índice de casilla)
    pub usuario: String,               // Nombre del operador autenticado
}

impl Game {
    /// Crea un nuevo juego. my_player: 1 si eres J1, 2 si eres J2.
    pub fn new(my_player: u8, usuario: String) -> Self {
        Game {
            board: [Cell::Empty; 9],
            current_player: 1, // Siempre empieza el J1
            my_player,
            usuario,
            result: GameResult::Ongoing,
            move_history: Vec::new(),
        }
    }

    /// Verifica si es el turno del jugador local
    pub fn is_my_turn(&self) -> bool {
        self.current_player == self.my_player
    }

    /// Verifica si una casilla está disponible (índice 0-8)
    pub fn is_cell_available(&self, index: usize) -> bool {
        index < 9 && self.board[index] == Cell::Empty
    }

    /// Aplica un movimiento al tablero.
    /// Retorna true si el movimiento fue válido.
    pub fn apply_move(&mut self, index: usize) -> bool {
        if self.result != GameResult::Ongoing {
            return false;
        }
        if !self.is_cell_available(index) {
            return false;
        }

        // Registrar movida en el historial
        self.move_history.push((self.current_player, index));

        // Colocar pieza del jugador actual
        self.board[index] = if self.current_player == 1 {
            Cell::X
        } else {
            Cell::O
        };

        // Verificar resultado
        self.result = self.check_winner();

        // Cambiar turno solo si el juego sigue
        if self.result == GameResult::Ongoing {
            self.current_player = if self.current_player == 1 { 2 } else { 1 };
        }

        true
    }

    /// Verifica si hay ganador, empate o si el juego continúa.
    /// Comprueba las 8 combinaciones ganadoras posibles.
    pub fn check_winner(&self) -> GameResult {
        // Combinaciones ganadoras: filas, columnas y diagonales
        const WINNING_LINES: [[usize; 3]; 8] = [
            [0, 1, 2], // fila superior
            [3, 4, 5], // fila media
            [6, 7, 8], // fila inferior
            [0, 3, 6], // columna izquierda
            [1, 4, 7], // columna central
            [2, 5, 8], // columna derecha
            [0, 4, 8], // diagonal principal
            [2, 4, 6], // diagonal inversa
        ];

        for line in &WINNING_LINES {
            let [a, b, c] = *line;
            if self.board[a] != Cell::Empty
                && self.board[a] == self.board[b]
                && self.board[b] == self.board[c]
            {
                let winner = if self.board[a] == Cell::X { 1 } else { 2 };
                return GameResult::Win(winner);
            }
        }

        // Empate: todas las casillas ocupadas sin ganador
        if self.board.iter().all(|c| *c != Cell::Empty) {
            return GameResult::Draw;
        }

        GameResult::Ongoing
    }

    /// Reinicia el tablero para una nueva partida
    pub fn reset(&mut self) {
        self.board = [Cell::Empty; 9];
        self.current_player = 1;
        self.result = GameResult::Ongoing;
        self.move_history.clear();
    }

    /// Retorna el símbolo del jugador local
    pub fn my_symbol(&self) -> &str {
        if self.my_player == 1 { "X" } else { "O" }
    }

    /// Retorna el símbolo del jugador rival
    pub fn rival_symbol(&self) -> &str {
        if self.my_player == 1 { "O" } else { "X" }
    }

    /// Texto descriptivo del estado actual del juego
    pub fn status_message(&self) -> String {
        match &self.result {
            GameResult::Ongoing => {
                if self.is_my_turn() {
                    "Tu turno — elige una casilla (1-9)".to_string()
                } else {
                    "Esperando movimiento del rival...".to_string()
                }
            }
            GameResult::Win(winner) => {
                if *winner == self.my_player {
                    "¡Ganaste! 🎉".to_string()
                } else {
                    "Perdiste. El rival ganó.".to_string()
                }
            }
            GameResult::Draw => "¡Empate!".to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────
// Verificación de ganador para el gato embebido en la TUI (U7PI)
//
// Función libre e independiente de `Game`/`Cell`: el gato embebido en el
// chat usa su propia representación de tablero (`[Option<char>; 9]`,
// 'X'/'O') en `client::app::GameState`, más simple que la de `Game` (no
// necesita historial de movidas ni turno-por-jugador-numérico). Se agrega
// aparte para no modificar el `Game` existente.
// ─────────────────────────────────────────────────────────

/// Retorna `Some('X')`/`Some('O')` si esa marca completó una línea ganadora
/// en `board`, o `None` si no hay ganador todavía (sigue en juego o empate).
pub fn verificar_ganador(board: &[Option<char>; 9]) -> Option<char> {
    const LINEAS_GANADORAS: [[usize; 3]; 8] = [
        [0, 1, 2], [3, 4, 5], [6, 7, 8], // filas
        [0, 3, 6], [1, 4, 7], [2, 5, 8], // columnas
        [0, 4, 8], [2, 4, 6],            // diagonales
    ];

    for [a, b, c] in LINEAS_GANADORAS {
        if let (Some(x), Some(y), Some(z)) = (board[a], board[b], board[c]) {
            if x == y && y == z {
                return Some(x);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests_gato_embebido {
    use super::verificar_ganador;

    #[test]
    fn game_detectar_ganador_fila() {
        let board = [
            Some('X'), Some('X'), Some('X'),
            None, None, None,
            None, None, None,
        ];
        assert_eq!(verificar_ganador(&board), Some('X'));
    }

    #[test]
    fn game_detectar_ganador_columna() {
        let board = [
            Some('O'), None, None,
            Some('O'), Some('X'), Some('X'),
            Some('O'), None, None,
        ];
        assert_eq!(verificar_ganador(&board), Some('O'));
    }

    #[test]
    fn game_detectar_ganador_diagonal() {
        let board = [
            Some('X'), Some('O'), None,
            None, Some('X'), Some('O'),
            None, None, Some('X'),
        ];
        assert_eq!(verificar_ganador(&board), Some('X'));
    }

    #[test]
    fn game_sin_ganador_tablero_vacio() {
        let board: [Option<char>; 9] = [None; 9];
        assert_eq!(verificar_ganador(&board), None);
    }

    #[test]
    fn game_sin_ganador_empate() {
        // X O X / X O O / O X X — lleno, sin línea ganadora
        let board = [
            Some('X'), Some('O'), Some('X'),
            Some('X'), Some('O'), Some('O'),
            Some('O'), Some('X'), Some('X'),
        ];
        assert_eq!(verificar_ganador(&board), None);
    }
}
