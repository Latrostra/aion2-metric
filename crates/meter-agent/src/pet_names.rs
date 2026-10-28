use std::collections::HashMap;

const IDS_URL: &str =
    "https://raw.githubusercontent.com/MikeZeen/aion2-pet-tracker/master/src/data/pet_ids.json";
const NAMES_URL: &str =
    "https://raw.githubusercontent.com/MikeZeen/aion2-pet-tracker/master/src/data/pet_names.json";
const DEFAULT_LANG: &str = "en";

pub fn build(ids_json: &str, names_json: &str, lang: &str) -> Option<HashMap<u32, String>> {
    let keys_by_id: HashMap<String, String> = serde_json::from_str(ids_json).ok()?;
    let names_by_lang: HashMap<String, HashMap<String, String>> =
        serde_json::from_str(names_json).ok()?;
    let names_by_key = names_by_lang
        .get(lang)
        .or_else(|| names_by_lang.get(DEFAULT_LANG))?;
    let names: HashMap<u32, String> = keys_by_id
        .iter()
        .filter_map(|(id, key)| Some((id.parse().ok()?, names_by_key.get(key)?.clone())))
        .collect();
    (!names.is_empty()).then_some(names)
}

fn fetch(url: &str) -> Option<String> {
    ureq::get(url).call().ok()?.body_mut().read_to_string().ok()
}

pub fn fetch_names() -> HashMap<u32, String> {
    fetch(IDS_URL)
        .zip(fetch(NAMES_URL))
        .and_then(|(ids_json, names_json)| build(&ids_json, &names_json, DEFAULT_LANG))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_map_to_names_through_the_internal_key() {
        let ids = r#"{"1214": "DarkEle_01", "1183": "CorpeSpider_01_V01", "9999": "Missing_01"}"#;
        let names = r#"{"en": {"DarkEle_01": "Black Spirit", "CorpeSpider_01_V01": "Soulless Corpse Gnat"}}"#;
        let map = build(ids, names, "pl").unwrap();
        assert_eq!(map.get(&1214).map(String::as_str), Some("Black Spirit"));
        assert_eq!(
            map.get(&1183).map(String::as_str),
            Some("Soulless Corpse Gnat")
        );
        assert!(!map.contains_key(&9999));
        assert!(build("{}", names, "en").is_none());
    }
}
