// ─────────────────────────────────────────────────────────
// crypto.rs — Cifrado Vigenère sobre ASCII imprimible
//
// Opera sobre el rango de caracteres ASCII imprimibles
// (códigos 32–126, 95 caracteres en total), garantizando
// que tanto el texto cifrado como el original sean
// legibles en terminal sin generar caracteres de control.
//
// Algoritmo:
//   La clave se repite cíclicamente sobre el texto.
//   Cifrado:   c' = ((c - 32) + (k - 32)) % 95 + 32
//   Descifrado: c = ((c' - 32) - (k - 32) + 95) % 95 + 32
//
// Donde c es el código ASCII del carácter original,
// k es el código ASCII del carácter de la clave, y
// c' es el código ASCII del carácter cifrado.
// ─────────────────────────────────────────────────────────

const BASE: u8 = 32;   // Primer carácter ASCII imprimible (espacio)
const RANGO: u8 = 95;  // Cantidad de caracteres imprimibles (32..=126)

/// Cifra `texto` usando Vigenère con `clave`.
/// Caracteres fuera del rango ASCII imprimible se dejan sin cifrar.
pub fn cifrar(texto: &str, clave: &str) -> String {
    if clave.is_empty() {
        return texto.to_string();
    }

    let clave_bytes: Vec<u8> = clave.bytes().collect();
    let mut ki = 0; // índice cíclico sobre la clave

    texto
        .bytes()
        .map(|c| {
            if c >= BASE && c <= 126 {
                let k = clave_bytes[ki % clave_bytes.len()];
                ki += 1;
                let desplazamiento = if k >= BASE { k - BASE } else { 0 };
                ((c - BASE + desplazamiento) % RANGO + BASE) as char
            } else {
                // Carácter fuera del rango imprimible: pasa sin modificar
                c as char
            }
        })
        .collect()
}

/// Descifra `texto` cifrado con Vigenère usando `clave`.
/// Invierte exactamente la operación de `cifrar`.
pub fn descifrar(texto: &str, clave: &str) -> String {
    if clave.is_empty() {
        return texto.to_string();
    }

    let clave_bytes: Vec<u8> = clave.bytes().collect();
    let mut ki = 0;

    texto
        .bytes()
        .map(|c| {
            if c >= BASE && c <= 126 {
                let k = clave_bytes[ki % clave_bytes.len()];
                ki += 1;
                let desplazamiento = if k >= BASE { k - BASE } else { 0 };
                ((c - BASE + RANGO - desplazamiento) % RANGO + BASE) as char
            } else {
                c as char
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────
// Tests unitarios
// ─────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cifrar_y_descifrar_es_identidad() {
        let clave = "clave123";
        let original = "hola mundo";
        let cifrado = cifrar(original, clave);
        assert_ne!(cifrado, original, "El texto cifrado debe diferir del original");
        assert_eq!(descifrar(&cifrado, clave), original);
    }

    #[test]
    fn cifrado_varia_por_posicion() {
        // "aaa" con clave "abc" debe producir 3 caracteres distintos
        // (desplazamientos 'a'-32=65, 'b'-32=66, 'c'-32=67)
        let cifrado = cifrar("aaa", "abc");
        let bytes: Vec<u8> = cifrado.bytes().collect();
        assert_ne!(bytes[0], bytes[1]);
        assert_ne!(bytes[1], bytes[2]);
    }

    #[test]
    fn clave_vacia_no_modifica() {
        assert_eq!(cifrar("hola", ""), "hola");
        assert_eq!(descifrar("hola", ""), "hola");
    }

    #[test]
    fn resultado_siempre_en_rango_imprimible() {
        let clave = "Z~! abc";
        let texto: String = (32u8..=126).map(|c| c as char).collect();
        let cifrado = cifrar(&texto, clave);
        for c in cifrado.bytes() {
            assert!(c >= 32 && c <= 126, "Carácter fuera del rango imprimible: {}", c);
        }
    }

    #[test]
    fn casilla_como_string() {
        // Simula el flujo real: serializar casilla → cifrar → descifrar → parsear
        let clave = "pass1";
        for casilla in 0usize..9 {
            let texto = casilla.to_string();
            let cifrado = cifrar(&texto, clave);
            let descifrado = descifrar(&cifrado, clave);
            assert_eq!(descifrado.parse::<usize>().unwrap(), casilla);
        }
    }
}
