// ─────────────────────────────────────────────────────────
// emoji.rs — Traducción de signos reservados a emojis
//
// El prefijo "/e " se procesa en el cliente ANTES de enviar por RPC.
// El receptor recibe el texto ya con emojis Unicode, sin lógica especial.
// ─────────────────────────────────────────────────────────

use std::collections::HashMap;

pub fn tabla() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert(":)", "😊");
    m.insert(":D", "😄");
    m.insert(":(", "😞");
    m.insert(":P", "😛");
    m.insert(";)", "😉");
    m.insert(">:(", "😠");
    m.insert("<3", "❤️");
    m.insert("SAD", "😢");
    m.insert("LOL", "😂");
    m.insert("lluvia", "🌧️");
    m.insert("calor", "🔥");
    m.insert("fiesta", "🎉");
    m
}

/// Si el mensaje empieza con "/e ", traduce los signos conocidos en el resto del texto.
/// Ejemplo: "/e hola :) como estas LOL" → "hola 😊 como estas 😂"
pub fn procesar(input: &str) -> String {
    let tabla = tabla();
    let mut resultado = input.to_string();
    for (signo, emoji) in &tabla {
        resultado = resultado.replace(&format!("/e {}", signo), emoji);
    }
    resultado
}

#[cfg(test)]
mod tests {
    #[test]
    fn emoji_procesar_con_prefijo() {
        let resultado = crate::emoji::procesar("Si aqui tambien hace /e calor y /e :)");
        assert_eq!(resultado, "Si aqui tambien hace 🔥 y 😊");
    }

    #[test]
    fn emoji_sin_prefijo_no_modifica() {
        let input = "hola :) mundo";
        assert_eq!(crate::emoji::procesar(input), input);
    }

    #[test]
    fn emoji_tabla_tiene_diez_entradas() {
        assert!(crate::emoji::tabla().len() >= 10);
    }
}
