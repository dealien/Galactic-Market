use rand::seq::SliceRandom;
use rand::Rng;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct NameDictionary {
    pub system: HashMap<String, NameCategory>,
    pub planet: HashMap<String, NameCategory>,
    pub city: HashMap<String, NameCategory>,
    pub company: HashMap<String, NameCategory>,
}

#[derive(Debug, Deserialize)]
pub struct NameCategory {
    #[serde(default)]
    pub prefixes: Vec<String>,
    #[serde(default)]
    pub cores: Vec<String>,
    #[serde(default)]
    pub suffixes: Vec<String>,
    #[serde(default)]
    pub industries: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationType {
    Core,
    Outpost,
}

impl LocationType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Outpost => "outpost",
        }
    }
}

static DICTIONARY: OnceLock<NameDictionary> = OnceLock::new();

/// Initialize the name dictionary from a JSON file.
///
/// This function MUST be called before any name generation functions are used.
/// If initialization fails, all subsequent name generation calls will panic with a clear error message.
///
/// # Errors
/// Returns an error if the file cannot be read or if the JSON is invalid.
pub fn init_dictionary(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let dict: NameDictionary = serde_json::from_str(&content)?;
    // Treat "already initialized" as a no-op so that calling this function
    // multiple times in the same process (e.g., across test runs) is safe.
    let _ = DICTIONARY.set(dict);
    Ok(())
}

fn get_dict() -> &'static NameDictionary {
    DICTIONARY
        .get()
        .expect("Name dictionary not initialized. Call init_dictionary first.")
}

pub fn generate_system_name(loc_type: LocationType, rng: &mut impl rand::Rng) -> String {
    let dict = get_dict();
    if let Some(cat) = dict.system.get(loc_type.as_str()) {
        generate_standard_name(cat, rng)
    } else {
        format!("System-{}", rng.gen_range(100..999))
    }
}

pub fn generate_planet_name(loc_type: LocationType, rng: &mut impl rand::Rng) -> String {
    let dict = get_dict();
    if let Some(cat) = dict.planet.get(loc_type.as_str()) {
        generate_standard_name(cat, rng)
    } else {
        format!("Planet-{}", rng.gen_range(100..999))
    }
}

pub fn generate_city_name(loc_type: LocationType, rng: &mut impl rand::Rng) -> String {
    let dict = get_dict();
    if let Some(cat) = dict.city.get(loc_type.as_str()) {
        generate_standard_name(cat, rng)
    } else {
        format!("City-{}", rng.gen_range(100..999))
    }
}

pub fn generate_company_name(loc_type: LocationType, rng: &mut impl rand::Rng) -> String {
    let dict = get_dict();
    let cat = match dict.company.get(loc_type.as_str()) {
        Some(c) => c,
        None => return format!("Company-{}", rng.gen_range(100..999)),
    };
    let prefix = cat.prefixes.choose(rng).map(|s| s.as_str()).unwrap_or("");
    let industry = cat
        .industries
        .choose(rng)
        .map(|s| s.as_str())
        .unwrap_or("Unknown");
    let suffix = cat.suffixes.choose(rng).map(|s| s.as_str()).unwrap_or("");

    // Core companies are more "formal" with prefixes; Outposts are often just [Industry] [Suffix]
    match loc_type {
        LocationType::Core => {
            if !prefix.is_empty() && rng.gen_bool(0.7) {
                if !suffix.is_empty() {
                    format!("{} {} {}", prefix, industry, suffix)
                } else {
                    format!("{} {}", prefix, industry)
                }
            } else {
                format!("{} {}", industry, suffix)
            }
        }
        LocationType::Outpost => {
            if !prefix.is_empty() && rng.gen_bool(0.4) {
                if !suffix.is_empty() {
                    format!("{} {} {}", prefix, industry, suffix)
                } else {
                    format!("{} {}", prefix, industry)
                }
            } else {
                format!("{} {}", industry, suffix)
            }
        }
    }
}

fn generate_standard_name(category: &NameCategory, rng: &mut impl rand::Rng) -> String {
    let prefix = category
        .prefixes
        .choose(rng)
        .map(|s| s.as_str())
        .unwrap_or("");
    let core = category
        .cores
        .choose(rng)
        .map(|s| s.as_str())
        .unwrap_or("Unknown");
    let suffix = category
        .suffixes
        .choose(rng)
        .map(|s| s.as_str())
        .unwrap_or("");

    // Weighted patterns
    match rng.gen_range(0..10) {
        0..=2 => core.to_string(), // 30% core only
        3..=5 => {
            if !prefix.is_empty() {
                format!("{} {}", prefix, core)
            } else {
                core.to_string()
            }
        }
        6..=8 => {
            if !suffix.is_empty() {
                format!("{} {}", core, suffix)
            } else {
                core.to_string()
            }
        }
        _ => {
            let mut parts = Vec::new();
            if !prefix.is_empty() {
                parts.push(prefix);
            }
            parts.push(core);
            if !suffix.is_empty() {
                parts.push(suffix);
            }
            parts.join(" ")
        }
    }
}
