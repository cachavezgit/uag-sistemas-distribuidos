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
