//! Synthetic name tables (spec Fase 7B). A player's sheet only keeps indices
//! (`PlayerBio::first_name` / `last_name`); these are the tables they index.
//! Invented names, on purpose: the real database comes in a later phase.

/// Given names; index with `first_name % FIRST_NAMES.len()`.
pub const FIRST_NAMES: [&str; 40] = [
    "Aldo", "Bento", "Caio", "Dário", "Elói", "Fausto", "Gael", "Heitor", "Ivo", "Jonas", "Kauê",
    "Lauro", "Mauro", "Nilo", "Otto", "Paulo", "Quirino", "Raul", "Saulo", "Tales", "Ugo", "Vítor",
    "Wagner", "Xavier", "Yago", "Zeca", "Abel", "Breno", "Ciro", "Dante", "Enzo", "Flávio", "Gil",
    "Hugo", "Iago", "Joel", "Leo", "Milton", "Noel", "Omar",
];

/// Family names; index with `last_name % LAST_NAMES.len()`.
pub const LAST_NAMES: [&str; 48] = [
    "Abrantes",
    "Barreto",
    "Cardim",
    "Dornelas",
    "Esteves",
    "Falcão",
    "Galvão",
    "Henriques",
    "Imperial",
    "Jardim",
    "Lacerda",
    "Maciel",
    "Nogueira",
    "Ornelas",
    "Pacheco",
    "Quintela",
    "Rangel",
    "Sardinha",
    "Taveira",
    "Uchoa",
    "Valadares",
    "Xisto",
    "Zanetti",
    "Alencar",
    "Bulhões",
    "Castelo",
    "Damasceno",
    "Evangelista",
    "Figueira",
    "Godói",
    "Holanda",
    "Inácio",
    "Junqueira",
    "Leitão",
    "Meireles",
    "Novais",
    "Outeiro",
    "Portela",
    "Queiroga",
    "Rebelo",
    "Seixas",
    "Trindade",
    "Urbano",
    "Vilela",
    "Arruda",
    "Bragança",
    "Coutinho",
    "Dias",
];

/// The 20 clubs of the league: `(name, short name)`, by club id.
pub const CLUB_NAMES: [(&str, &str); 20] = [
    ("Atlético Aurora", "AUR"),
    ("Boa Vista do Sul", "BVS"),
    ("Clube do Porto Novo", "CPN"),
    ("Desportivo Serrano", "DSE"),
    ("Esporte Clube Litoral", "LIT"),
    ("Ferroviário do Norte", "FER"),
    ("Grêmio das Palmeiras", "GDP"),
    ("Harmonia Futebol Clube", "HAR"),
    ("Independente da Barra", "IDB"),
    ("Juventude Mineira", "JUV"),
    ("Lagoa Azul", "LAG"),
    ("Mirante Esporte Clube", "MIR"),
    ("Nacional do Vale", "NAC"),
    ("Operário Central", "OPE"),
    ("Pedra Branca", "PBR"),
    ("Quinze de Outubro", "QUI"),
    ("Real Campestre", "RCA"),
    ("Santa Luzia", "SLU"),
    ("Tupi do Planalto", "TUP"),
    ("União Ribeirinha", "URI"),
];

/// "Given Family" for the name indices of a player's sheet.
#[must_use]
pub fn player_name(first_name: u16, last_name: u16) -> String {
    let first = FIRST_NAMES[usize::from(first_name) % FIRST_NAMES.len()];
    let last = LAST_NAMES[usize::from(last_name) % LAST_NAMES.len()];
    format!("{first} {last}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn tables_have_no_repeats() {
        assert_eq!(
            FIRST_NAMES.iter().collect::<HashSet<_>>().len(),
            FIRST_NAMES.len()
        );
        assert_eq!(
            LAST_NAMES.iter().collect::<HashSet<_>>().len(),
            LAST_NAMES.len()
        );
        assert_eq!(
            CLUB_NAMES.iter().map(|c| c.0).collect::<HashSet<_>>().len(),
            20
        );
        assert_eq!(
            CLUB_NAMES.iter().map(|c| c.1).collect::<HashSet<_>>().len(),
            20
        );
        assert!(CLUB_NAMES.iter().all(|c| c.1.len() == 3));
    }

    #[test]
    fn any_index_names_a_player() {
        assert_eq!(player_name(0, 0), "Aldo Abrantes");
        assert_eq!(player_name(40, 48), "Aldo Abrantes");
        assert_eq!(
            player_name(u16::MAX, u16::MAX),
            player_name(65_535 % 40, 65_535 % 48)
        );
    }
}
