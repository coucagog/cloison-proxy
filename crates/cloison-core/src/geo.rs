//! Whitelist géo (S16) : les **noms de pays** ne sont pas des PII — les
//! masquer dégrade les réponses générales (rapport client 09/09 §6.4 :
//! « Sénégal » masqué → réponse « Laos »). Cette liste (français + anglais,
//! minuscules, accents conservés) est appliquée aux spans NER `LOCATION`
//! UNIQUEMENT quand `Policy.geo_whitelist` est actif (défaut) — les
//! toponymes sénégalais fins (`ville_sn`) restent couverts par leur
//! généralisation `[VILLE_SN]`.

/// Pays (FR + EN, minuscules). Couvre les noms courants et les formes
/// composées usuelles ; la comparaison est une égalité exacte après
/// `trim + lowercase`.
pub const COUNTRY_WHITELIST: &[&str] = &[
    // Afrique
    "afrique du sud", "south africa",
    "algérie", "algeria",
    "angola",
    "bénin", "benin",
    "botswana",
    "burkina faso",
    "burundi",
    "cameroun", "cameroon",
    "cap-vert", "cape verde",
    "centrafrique", "central african republic",
    "comores", "comoros",
    "congo",
    "république démocratique du congo", "democratic republic of the congo", "rdc", "drc",
    "côte d'ivoire", "cote d'ivoire", "ivory coast",
    "djibouti",
    "égypte", "egypt",
    "érythrée", "eritrea",
    "éthiopie", "ethiopia",
    "gabon",
    "gambie", "gambia",
    "ghana",
    "guinée", "guinea",
    "guinée-bissau", "guinea-bissau",
    "guinée équatoriale", "equatorial guinea",
    "kenya",
    "lesotho",
    "libéria", "liberia",
    "libye", "libya",
    "madagascar",
    "malawi",
    "mali",
    "maroc", "morocco",
    "maurice", "mauritius",
    "mauritanie", "mauritania",
    "mozambique",
    "namibie", "namibia",
    "niger",
    "nigéria", "nigeria",
    "ouganda", "uganda",
    "rwanda",
    "sao tomé-et-principe", "sao tome and principe",
    "sénégal", "senegal",
    "seychelles",
    "sierra leone",
    "somalie", "somalia",
    "soudan", "sudan",
    "soudan du sud", "south sudan",
    "tanzanie", "tanzania",
    "tchad", "chad",
    "togo",
    "tunisie", "tunisia",
    "zambie", "zambia",
    "zimbabwe",
    // Amériques
    "argentine", "argentina",
    "bolivie", "bolivia",
    "brésil", "brazil",
    "canada",
    "chili", "chile",
    "colombie", "colombia",
    "costa rica",
    "cuba",
    "équateur", "ecuador",
    "états-unis", "etats-unis", "united states", "usa",
    "guatemala",
    "haïti", "haiti",
    "honduras",
    "jamaïque", "jamaica",
    "mexique", "mexico",
    "nicaragua",
    "panama",
    "paraguay",
    "pérou", "perou", "peru",
    "république dominicaine", "dominican republic",
    "salvador", "el salvador",
    "uruguay",
    "vénézuéla", "venezuela",
    // Asie
    "afghanistan",
    "arabie saoudite", "saudi arabia",
    "arménie", "armenia",
    "azerbaïdjan", "azerbaijan",
    "bahreïn", "bahrain",
    "bangladesh",
    "bhoutan", "bhutan",
    "birmanie", "myanmar",
    "brunei",
    "cambodge", "cambodia",
    "chine", "china",
    "corée du nord", "north korea",
    "corée du sud", "south korea",
    "émirats arabes unis", "united arab emirates", "eau", "uae",
    "géorgie", "georgia",
    "inde", "india",
    "indonésie", "indonesia",
    "irak", "iraq",
    "iran",
    "israël", "israel",
    "japon", "japan",
    "jordanie", "jordan",
    "kazakhstan",
    "kirghizistan", "kyrgyzstan",
    "koweït", "kuwait",
    "laos",
    "liban", "lebanon",
    "malaisie", "malaysia",
    "maldives",
    "mongolie", "mongolia",
    "népal", "nepal",
    "oman",
    "ouzbékistan", "uzbekistan",
    "pakistan",
    "palestine",
    "philippines",
    "qatar",
    "singapour", "singapore",
    "sri lanka",
    "syrie", "syria",
    "tadjikistan", "tajikistan",
    "taïwan", "taiwan",
    "thaïlande", "thailand",
    "timor oriental", "timor-leste", "east timor",
    "turkménistan", "turkmenistan",
    "turquie", "turkey",
    "viêt nam", "vietnam", "viêtnam", "viet nam",
    "yémen", "yemen",
    // Europe
    "albanie", "albania",
    "allemagne", "germany",
    "andorre", "andorra",
    "autriche", "austria",
    "belgique", "belgium",
    "biélorussie", "bielorussie", "belarus",
    "bosnie-herzégovine", "bosnia and herzegovina",
    "bulgarie", "bulgaria",
    "chypre", "cyprus",
    "croatie", "croatia",
    "danemark", "denmark",
    "espagne", "spain",
    "estonie", "estonia",
    "finlande", "finland",
    "france",
    "grèce", "grece", "greece",
    "hongrie", "hungary",
    "irlande", "ireland",
    "islande", "iceland",
    "italie", "italy",
    "kosovo",
    "lettonie", "latvia",
    "liechtenstein",
    "lituanie", "lithuania",
    "luxembourg",
    "macédoine du nord", "north macedonia",
    "malte", "malta",
    "moldavie", "moldova",
    "monaco",
    "monténégro", "montenegro",
    "norvège", "norvege", "norway",
    "pays-bas", "pays bas", "netherlands",
    "pologne", "poland",
    "portugal",
    "roumanie", "romania",
    "royaume-uni", "royaume uni", "united kingdom", "uk",
    "russie", "russia",
    "saint-marin", "san marino",
    "serbie", "serbia",
    "slovaquie", "slovakia",
    "slovénie", "slovenie", "slovenia",
    "suède", "suede", "sweden",
    "suisse", "switzerland",
    "tchéquie", "tchequie", "czech republic", "czechia",
    "ukraine",
    "vatican", "holy see",
    // Océanie
    "australie", "australia",
    "fidji", "fiji",
    "nouvelle-zélande", "nouvelle zelande", "new zealand",
    "papouasie-nouvelle-guinée", "papua new guinea",
    // Antarctique
    "antarctique", "antarctica",
];

/// Un nom de pays (après trim + minuscules) est-il dans la whitelist ?
pub fn is_whitelisted_country(value: &str) -> bool {
    let v = value.trim().to_lowercase();
    COUNTRY_WHITELIST.contains(&v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_whitelist_matches_fr_and_en() {
        assert!(is_whitelisted_country("Sénégal"));
        assert!(is_whitelisted_country(" senegal "));
        assert!(is_whitelisted_country("France"));
        assert!(is_whitelisted_country("UNITED STATES"));
        assert!(is_whitelisted_country("Côte d'Ivoire"));
    }

    #[test]
    fn country_whitelist_rejects_cities_and_other() {
        // Les villes (y compris SN) ne sont PAS dans la whitelist : elles
        // restent couvertes (masquage ou généralisation [VILLE_SN]).
        assert!(!is_whitelisted_country("Dakar"));
        assert!(!is_whitelisted_country("Ziguinchor"));
        assert!(!is_whitelisted_country("Paris"));
        assert!(!is_whitelisted_country("Aminata Diop"));
    }
}
