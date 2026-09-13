use crate::{
    error::{AppError, Result},
    models::{OcrResponse, SubmitLabelRequest},
};
use serde_json::json;
use sqlx::PgPool;

/// Common allergens as they conventionally appear on packaging (often
/// capitalized/bolded by regulation) — a keyword match against the raw text
/// is a cheap, good-enough heuristic without pulling in an NLP model.
const KNOWN_ALLERGENS: &[&str] = &[
    "milk", "wheat", "soy", "soya", "peanut", "peanuts", "tree nut", "tree nuts", "egg", "eggs",
    "fish", "shellfish", "sesame", "gluten", "mustard", "celery", "lupin", "sulphites",
    "sulfites",
];

pub struct OcrService {
    db: PgPool,
}

impl OcrService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    #[tracing::instrument(skip(self, req), fields(barcode = %req.barcode, country = %req.country))]
    pub async fn submit_label(&self, req: &SubmitLabelRequest) -> Result<OcrResponse> {
        if req.extracted_text.trim().is_empty() {
            tracing::warn!("rejected: no text recognized in image");
            return Err(AppError::OcrFailed("No text recognized in image".to_string()));
        }
        if req.reviewed_ingredients.trim().is_empty() {
            tracing::warn!("rejected: empty ingredients");
            return Err(AppError::InvalidRequest("Ingredients cannot be empty".to_string()));
        }

        let guessed_name = req
            .product_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| n.chars().take(255).collect::<String>())
            .unwrap_or_else(|| guess_product_name(&req.extracted_text));
        let brand = req
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(|b| b.chars().take(255).collect::<String>());
        let nutrition = sanitize_nutrition(req.nutrition.as_ref());
        let ocr_ingredients = parse_ingredients(&req.extracted_text);
        let ocr_allergens = detect_allergens(&req.extracted_text);
        let confidence = ingredient_overlap_ratio(&ocr_ingredients, &req.reviewed_ingredients);
        let dropped_allergen = removed_an_ocr_detected_allergen(&ocr_allergens, &req.reviewed_allergens);

        // Genuine corrections (fixing one bad OCR line, reordering) leave
        // most of what OCR actually found intact, so confidence stays high.
        // Wholesale fabrication doesn't — that's the signal we can actually
        // check without any auth/reputation system in place yet. Silently
        // dropping an allergen OCR clearly found is flagged regardless of
        // ingredient-text confidence — that's a safety issue, not a style edit.
        let status = if dropped_allergen {
            "flagged_allergen_mismatch"
        } else if confidence < LOW_CONFIDENCE_THRESHOLD {
            "flagged_low_confidence"
        } else {
            "pending_verification"
        };

        if status == "pending_verification" {
            tracing::info!(confidence, status, "OCR submission received");
        } else {
            // Flagged submissions are the ones worth a human noticing —
            // warn! keeps them visible in a log stream filtered above info.
            tracing::warn!(confidence, status, dropped_allergen, "OCR submission flagged for review");
        }

        let parsed_nutrition = json!({
            "guessed_name": guessed_name,
            "brand": brand,
            "nutrition": nutrition,
            "ingredients": req.reviewed_ingredients,
            "allergens": req.reviewed_allergens,
            "ocr_parsed_ingredients": ocr_ingredients,
            "ocr_detected_allergens": ocr_allergens,
            "match_confidence": confidence,
        });

        sqlx::query(
            "INSERT INTO ocr_submissions (barcode, country, extracted_text, parsed_nutrition, confidence_score, status)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&req.barcode)
        .bind(&req.country)
        .bind(&req.extracted_text)
        .bind(&parsed_nutrition)
        .bind(confidence)
        .bind(status)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Without this, a contribution only ever lands in the review queue —
        // scanning the same barcode again still finds nothing, which is
        // exactly the "help us add it" flow's whole point defeated. This
        // makes the barcode searchable immediately, same as an
        // Open-Food-Facts-sourced product: `verified = false` until three
        // independent verifications confirm it, through the same mechanism
        // that already gates every other product. `ON CONFLICT DO NOTHING`
        // rather than overwriting — if the barcode already exists (someone
        // else's contribution landed first, or Open Food Facts has it after
        // all), unverified OCR data shouldn't clobber it.
        let allergens_text = if req.reviewed_allergens.is_empty() {
            None
        } else {
            Some(req.reviewed_allergens.join(", "))
        };
        let product_name = if guessed_name.trim().is_empty() {
            "Unknown"
        } else {
            &guessed_name
        };
        let insert_result = sqlx::query(
            "INSERT INTO products (barcode, country, product_name, brand, ingredients, allergens, nutrition_facts, source, verified, verification_count)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'user_contributed', false, 0)
             ON CONFLICT (barcode) DO NOTHING",
        )
        .bind(&req.barcode)
        .bind(&req.country)
        .bind(product_name)
        .bind(&brand)
        .bind(&req.reviewed_ingredients)
        .bind(&allergens_text)
        .bind(&nutrition)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if insert_result.rows_affected() == 0 {
            tracing::info!("barcode already existed, contribution kept in review queue only");
        } else {
            tracing::info!(product_name = %product_name, "created product from contribution");
        }

        Ok(OcrResponse {
            guessed_name,
            ingredients: req.reviewed_ingredients.clone(),
            allergens: req.reviewed_allergens.clone(),
        })
    }
}

const LOW_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// The only nutrition keys `products.nutrition_facts` ever carries — same
/// set `ProductService` writes for Open Food Facts products.
const NUTRITION_KEYS: &[&str] = &[
    "energy_kcal", "protein", "carbs", "fat", "saturated_fat", "trans_fat", "fiber",
    "sugar", "sodium", "cholesterol", "potassium", "calcium", "iron",
];

/// Keep only known keys with finite, non-negative, plausible per-100g
/// numbers. A client typo ("sugar": 2400) is dropped rather than stored as
/// truth; an unknown key is dropped rather than polluting the schema.
fn sanitize_nutrition(input: Option<&serde_json::Value>) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    if let Some(obj) = input.and_then(|v| v.as_object()) {
        for key in NUTRITION_KEYS {
            if let Some(n) = obj.get(*key).and_then(|v| v.as_f64())
                && n.is_finite()
                && (0.0..=1000.0).contains(&n)
            {
                out.insert((*key).to_string(), json!(n));
            }
        }
    }
    serde_json::Value::Object(out)
}

/// Fraction of the ingredients OCR actually found (from the untouched raw
/// text) that also appear in what the user ultimately submitted. Deliberately
/// one-directional — added detail (e.g. "may contain traces of...") isn't
/// penalized, only *missing/replaced* OCR-backed content is.
fn ingredient_overlap_ratio(ocr_derived: &str, reviewed: &str) -> f32 {
    let ocr_items: Vec<String> = ocr_derived
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    if ocr_items.is_empty() {
        return 1.0;
    }

    let reviewed_lower = reviewed.to_lowercase();
    let matched = ocr_items
        .iter()
        .filter(|item| reviewed_lower.contains(item.as_str()))
        .count();

    matched as f32 / ocr_items.len() as f32
}

/// True if OCR found an allergen in the raw text that's absent from the
/// user's final allergen list — the dangerous direction of editing. Adding
/// an allergen OCR missed is never flagged; only silently dropping one is.
fn removed_an_ocr_detected_allergen(ocr_detected: &[String], reviewed: &[String]) -> bool {
    let reviewed_lower: Vec<String> = reviewed.iter().map(|a| a.to_lowercase()).collect();
    ocr_detected
        .iter()
        .any(|allergen| !reviewed_lower.contains(&allergen.to_lowercase()))
}

/// Section headers that commonly follow the ingredients list on packaging —
/// used to stop the ingredients scan before it swallows unrelated text.
const SECTION_BOUNDARIES: &[&str] = &[
    "contains",
    "may contain",
    "allergen",
    "manufactured",
    "distributed by",
    "storage",
    "best before",
    "net wt",
    "nutrition facts",
    "nutritional information",
];

/// Ingredient lists are conventionally comma-separated, often introduced by
/// an "Ingredients:" label — strip that prefix if present, stop at the next
/// section header, then split.
fn parse_ingredients(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let body = match lower.find("ingredients") {
        Some(idx) => {
            let after_label = &raw[idx..];
            after_label.splitn(2, ':').nth(1).unwrap_or(after_label)
        }
        None => raw,
    };
    let body = truncate_at_next_section(body);

    body.split(|c: char| c == ',' || c == '\n')
        .map(|s| s.trim().trim_end_matches('.').trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn truncate_at_next_section(body: &str) -> &str {
    let lower = body.to_lowercase();
    let cutoff = SECTION_BOUNDARIES
        .iter()
        .filter_map(|marker| lower.find(marker))
        .filter(|&idx| idx > 0)
        .min();

    match cutoff {
        Some(idx) => &body[..idx],
        None => body,
    }
}

fn detect_allergens(raw: &str) -> Vec<String> {
    let lower = raw.to_lowercase();
    let mut found: Vec<String> = KNOWN_ALLERGENS
        .iter()
        .filter(|allergen| lower.contains(*allergen))
        .map(|allergen| capitalize(allergen))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Ingredient lists don't reliably say what the product itself is called,
/// so this is a rough guess: the first non-empty line that isn't the
/// ingredients label itself. Good enough for a pre-filled review field the
/// user confirms or edits, not meant to be authoritative.
fn guess_product_name(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.to_lowercase().starts_with("ingredient"))
        .unwrap_or("")
        .to_string()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_nutrition_to_known_plausible_keys() {
        let input = json!({"sugar": 12.5, "sodium": -1, "energy_kcal": 546, "unicorn": 3, "fat": "9"});
        let out = sanitize_nutrition(Some(&input));
        assert_eq!(out, json!({"sugar": 12.5, "energy_kcal": 546.0}));
        assert_eq!(sanitize_nutrition(None), json!({}));
    }

    #[test]
    fn parses_comma_separated_ingredients_after_label() {
        let raw = "Nutella\nIngredients: Sugar, Palm Oil, Hazelnuts 13%, Cocoa.";
        assert_eq!(parse_ingredients(raw), "Sugar, Palm Oil, Hazelnuts 13%, Cocoa");
    }

    #[test]
    fn detects_known_allergens_case_insensitively() {
        let raw = "Contains MILK and Soy. May contain traces of PEANUT.";
        assert_eq!(detect_allergens(raw), vec!["Milk", "Peanut", "Soy"]);
    }

    #[test]
    fn guesses_name_from_first_non_ingredients_line() {
        assert_eq!(guess_product_name("Nutella\nIngredients: Sugar, Cocoa"), "Nutella");
    }

    #[test]
    fn stops_ingredients_at_the_next_section_header() {
        let raw = "Nutella\nIngredients: Sugar, Palm Oil, Cocoa.\nContains MILK, SOY.\nBest before 12/2027";
        assert_eq!(parse_ingredients(raw), "Sugar, Palm Oil, Cocoa");
    }

    #[test]
    fn high_overlap_when_user_only_removes_a_bad_ocr_line() {
        // The exact reported case: OCR swept in a trailing non-ingredient
        // line, user deletes just that one item.
        let ocr = "Sugar, Palm Oil, Cocoa, Store In A Cool Place";
        let reviewed = "Sugar, Palm Oil, Cocoa";
        assert!(ingredient_overlap_ratio(ocr, reviewed) >= LOW_CONFIDENCE_THRESHOLD);
    }

    #[test]
    fn low_overlap_when_ingredients_are_wholesale_replaced() {
        let ocr = "Sugar, Palm Oil, Cocoa, Hazelnuts";
        let reviewed = "Organic Kale, Spirulina, Chia Seeds";
        assert!(ingredient_overlap_ratio(ocr, reviewed) < LOW_CONFIDENCE_THRESHOLD);
    }

    #[test]
    fn flags_when_an_ocr_detected_allergen_is_dropped() {
        let ocr_allergens = vec!["Milk".to_string(), "Soy".to_string()];
        let reviewed = vec!["Soy".to_string()]; // Milk silently removed
        assert!(removed_an_ocr_detected_allergen(&ocr_allergens, &reviewed));
    }

    #[test]
    fn does_not_flag_when_user_adds_an_allergen_ocr_missed() {
        let ocr_allergens = vec!["Milk".to_string()];
        let reviewed = vec!["Milk".to_string(), "Peanut".to_string()];
        assert!(!removed_an_ocr_detected_allergen(&ocr_allergens, &reviewed));
    }
}
